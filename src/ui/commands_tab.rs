/*! The commands tab lists the commands of your own in the main panel and
what the selected one runs in the details panel.

It is the settings tab's, opened from the row for `blazingjj.commands`
and left again for it, so it has no place of its own in the tab bar.
What it writes are the keys under that table, in the user's own config
file, just as the settings tab writes the options beside it.

A command is one key holding more than any one thing to type, so what
the tab writes is the whole of it every time: what is asked for is the
command line, the label or neither, and what is written is the command
as that leaves it.
*/

use anyhow::Result;
use anyhow::bail;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::prelude::*;
use ratatui::widgets::*;
use tracing::instrument;

use crate::app::TabId;
use crate::app::command::Command;
use crate::commander::config::config_value;
use crate::commander::new_commander;
use crate::commands::CustomCommand;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::event::Mouse;
use crate::keybinds::Binding;
use crate::keybinds::CommandsTabEvent;
use crate::keybinds::CommandsTabKeybinds;
use crate::menus::Menu;
use crate::selection::Placeholder;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::ChoicePopup;
use crate::ui::dialog::SettingValuePopup;
use crate::ui::panel::ListPane;
use crate::ui::panel::MouseInput;
use crate::ui::panel::PanelMouseInput;
use crate::ui::panel::copy_marked;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::error_text;

/// What the app says of a command whose output it captures, as against
/// one it hands the terminal to.
const CAPTURED: &str = "captured";
const INTERACTIVE: &str = "interactive";

/// One command the tab lists: the name it is configured under and what
/// the configuration says it is.
struct Listed {
    name: String,
    command: CustomCommand,
    /// Whether the user's own config file is what adds it, which is what
    /// makes it the tab's to take back out.
    is_users: bool,
    /// The menus that hold it, of which there may be none: a command
    /// goes in a menu by being listed in `blazingjj.context-menu`.
    menus: Vec<&'static str>,
}

impl Listed {
    /// The config key it is configured under.
    fn key(&self) -> String {
        CustomCommand::key(&self.name)
    }
}

/// The commands of your own, as the configuration has them.
#[derive(Default)]
struct Commands {
    listed: Vec<Listed>,
    selected: usize,
}

impl Commands {
    /// Every command `config` adds, in the order the names sort in, and
    /// what of them the user's own config file, `user`, is what adds.
    fn read(config: &JjConfig, user: &toml::Table) -> Self {
        let listed = config
            .commands()
            .iter()
            .map(|(name, command)| Listed {
                name: name.clone(),
                command: command.clone(),
                is_users: config_value(user, &CustomCommand::key(name)).is_some(),
                menus: Menu::ALL
                    .into_iter()
                    .filter(|menu| {
                        config
                            .context_menu()
                            .of(*menu)
                            .is_some_and(|ids| ids.iter().any(|id| id == name))
                    })
                    .map(Menu::key)
                    .collect(),
            })
            .collect();

        Self {
            listed,
            selected: 0,
        }
    }

    /// The names the commands go by, which are the names a new one
    /// cannot take.
    fn names(&self) -> Vec<String> {
        self.listed
            .iter()
            .map(|listed| listed.name.clone())
            .collect()
    }

    fn selected(&self) -> Option<&Listed> {
        self.listed.get(self.selected)
    }

    /// Move the selection by `scroll` rows, as far as there are rows to
    /// go.
    fn scroll(&mut self, scroll: isize) {
        let last = self.listed.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(scroll).min(last);
    }

    /// Select the row at `index`, which is the mouse landing on it.
    fn select_row(&mut self, index: usize) {
        if index < self.listed.len() {
            self.selected = index;
        }
    }
}

pub struct CommandsTab {
    /// The commands as the configuration has them, or why it could not
    /// be read.
    commands: Result<Commands>,

    commands_pane: ListPane,
    commands_list_state: ListState,

    keybinds: CommandsTabKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

impl CommandsTab {
    /// A stale tab, holding none of the commands the configuration adds
    /// yet.
    #[instrument(level = "info", name = "Initializing commands tab", parent = None)]
    pub fn new() -> Self {
        Self {
            commands: Ok(Commands::default()),

            commands_pane: ListPane::default(),
            commands_list_state: ListState::default(),

            keybinds: CommandsTabKeybinds::new(),
            pane_divider: PaneDivider::default(),

            stale: true,
        }
    }

    fn selected(&self) -> Option<&Listed> {
        self.commands.as_ref().ok()?.selected()
    }

    /// Which row is selected, for drawing the list.
    fn selected_row(&self) -> usize {
        self.commands
            .as_ref()
            .map_or(0, |commands| commands.selected)
    }

    fn scroll_commands(&mut self, scroll: isize) {
        if let Ok(commands) = self.commands.as_mut() {
            commands.scroll(scroll);
        }
    }

    fn select_row(&mut self, index: usize) {
        if let Ok(commands) = self.commands.as_mut() {
            commands.select_row(index);
        }
    }

    /// Ask for what the selected command is to run.
    fn change_command_line(&self) -> Option<AppAction> {
        let listed = self.selected()?;
        let command = listed.command.clone();

        Some(AppAction::SetPopup(Box::new(SettingValuePopup::of_key(
            listed.key(),
            command.command_line(),
            move |text| Ok(command.with_command_line(text)?.value()),
        ))))
    }

    /// Ask for what a menu is to call the selected command, which it
    /// goes by its name again without.
    fn change_label(&self) -> Option<AppAction> {
        let listed = self.selected()?;
        let command = listed.command.clone();

        Some(AppAction::SetPopup(Box::new(SettingValuePopup::of_key(
            listed.key(),
            command.configured_label().unwrap_or_default().to_owned(),
            move |text| Ok(command.with_label(text).value()),
        ))))
    }

    /// Run the selected command the other way round: with the terminal
    /// handed over to it if its output was captured, and the other way
    /// about.
    fn toggle_interactive(&self) -> Option<AppAction> {
        let listed = self.selected()?;

        Some(AppAction::Run(Command::SetSetting {
            key: listed.key(),
            value: listed.command.toggled_interactive().value(),
        }))
    }

    /// Ask for the name of a command to add, and then for what it is to
    /// run: a command is named by the name a menu holds it by, so there
    /// is no adding one without one.
    fn add(&self) -> AppAction {
        let taken = self
            .commands
            .as_ref()
            .map(Commands::names)
            .unwrap_or_default();

        AppAction::SetPopup(Box::new(SettingValuePopup::new(
            "blazingjj.commands",
            String::new(),
            move |name| {
                let name = check_name(name, &taken)?;

                Ok(AppAction::SetPopup(Box::new(SettingValuePopup::of_key(
                    CustomCommand::key(&name),
                    String::new(),
                    |text| Ok(CustomCommand::to_run(text)?.value()),
                ))))
            },
        )))
    }

    /// Take the selected command out of the user's config file, leaving
    /// whatever the rest of the configuration says.
    fn unset(&self) -> Option<AppAction> {
        let listed = self.selected()?;

        listed
            .is_users
            .then(|| AppAction::Run(Command::UnsetSetting { key: listed.key() }))
    }

    /// The menu of what can be done to the selected command, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> Option<AppAction> {
        let mut items = vec![(Line::raw("Add a command"), self.add())];
        if let Some(change) = self.change_command_line() {
            items.insert(0, (Line::raw("Change what it runs"), change));
        }
        if let Some(label) = self.change_label() {
            items.insert(1, (Line::raw("Change what a menu calls it"), label));
        }
        if let Some(listed) = self.selected()
            && let Some(toggle) = self.toggle_interactive()
        {
            let says = if listed.command.is_interactive() {
                "Capture its output"
            } else {
                "Hand the terminal to it"
            };
            items.insert(2, (Line::raw(says), toggle));
        }
        if let Some(unset) = self.unset() {
            items.push((Line::raw("Take out of your config"), unset));
        }

        Some(AppAction::SetPopup(Box::new(ChoicePopup::new(
            get_env().jj_config.clone(),
            anchor,
            "Command actions",
            items,
        ))))
    }

    fn handle_event(&mut self, event: CommandsTabEvent) -> Option<AppAction> {
        match event {
            CommandsTabEvent::ChangeCommandLine => self.change_command_line(),
            CommandsTabEvent::ChangeLabel => self.change_label(),
            CommandsTabEvent::ToggleInteractive => self.toggle_interactive(),
            CommandsTabEvent::Add => Some(self.add()),
            CommandsTabEvent::Unset => self.unset(),
            CommandsTabEvent::Back => Some(AppAction::ViewTab(TabId::Settings)),
            // Not an operation of its own; the key handler deals with it.
            CommandsTabEvent::Unbound => None,
        }
    }

    /// One row per command: its name and what it runs, in a panel
    /// `width` columns wide.
    fn command_lines(&self, commands: &Commands, width: u16) -> Vec<Line<'static>> {
        if commands.listed.is_empty() {
            return vec![
                Line::raw(""),
                Line::raw("  No commands of your own yet.").fg(Color::DarkGray),
            ];
        }

        let name_width = commands
            .listed
            .iter()
            .map(|listed| listed.name.chars().count())
            .max()
            .unwrap_or(0);
        // What a command runs is what the list is read by, so it takes
        // what is left of the row once the names have their column.
        let command_width = (width as usize).saturating_sub(name_width + 3);

        commands
            .listed
            .iter()
            .enumerate()
            .map(|(index, listed)| {
                let command_line: String = listed
                    .command
                    .command_line()
                    .chars()
                    .take(command_width)
                    .collect();
                let line = Line::from(vec![
                    Span::raw(format!(" {:name_width$}  ", listed.name)),
                    Span::raw(command_line),
                ]);

                if index == commands.selected {
                    line.bg(get_env().jj_config.highlight_color())
                } else {
                    line
                }
            })
            .collect()
    }

    /// What the selected command runs, what it is called, how it is run
    /// and which menus hold it.
    fn details_text(&self) -> Text<'static> {
        let Some(listed) = self.selected() else {
            return Text::from(vec![
                Line::raw("The commands of your own").bold(),
                Line::raw(""),
                Line::raw(
                    "A command of your own runs against what a tab has selected, and a \
                     context menu holds it by listing its name.",
                ),
                Line::raw(""),
                Line::raw("The arguments can name the selection by:"),
                Line::raw(""),
            ])
            .lines
            .into_iter()
            .chain(placeholder_lines())
            .collect();
        };

        let source = if listed.is_users {
            "  (in your config)"
        } else {
            "  (elsewhere in your configuration)"
        };
        let held_by = if listed.menus.is_empty() {
            "no menu holds it".to_owned()
        } else {
            listed.menus.join(", ")
        };

        Text::from(vec![
            Line::from(vec![
                Span::raw(listed.command.label(&listed.name)).bold(),
                Span::raw(source).fg(Color::DarkGray),
            ]),
            Line::raw(""),
            Line::raw(format!("Runs:           {}", listed.command.command_line())),
            Line::raw(format!(
                "Run as:         {}",
                if listed.command.is_interactive() {
                    INTERACTIVE
                } else {
                    CAPTURED
                }
            )),
            Line::raw(format!("Configured as:  {}", listed.key())),
            Line::raw(format!("In the menus:   {held_by}")),
            Line::raw(""),
            Line::raw("The arguments can name the selection by:"),
            Line::raw(""),
        ])
        .lines
        .into_iter()
        .chain(placeholder_lines())
        .collect()
    }
}

/// One row per placeholder, saying what it stands for.
fn placeholder_lines() -> impl Iterator<Item = Line<'static>> {
    Placeholder::ALL.into_iter().map(|placeholder| {
        Line::from(vec![
            Span::raw(format!("  {:<12}", placeholder.names().join(", "))),
            Span::raw(placeholder.doc()).fg(Color::DarkGray),
        ])
    })
}

/// The name a command is to be added under, refused when it is no name
/// to configure a command by or when `taken` holds it already.
fn check_name(name: &str, taken: &[String]) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("A command is named by the name a menu holds it by.");
    }
    // The name is one config key of its own, so anything a key cannot
    // hold would name something other than the command.
    if name.contains(['.', ' ', '"', '\'', '[', ']', '=']) {
        bail!("A command's name is one config key, so it holds none of . \" ' [ ] = or a space.");
    }
    if taken.iter().any(|held| held == name) {
        bail!("There is a command called {name} already.");
    }

    Ok(name.to_owned())
}

impl Tab for CommandsTab {
    fn refresh(&mut self) -> Result<()> {
        // Reading the commands again is what a change to one asks for,
        // and what was changed is what is selected, so the list comes
        // back with the selection where it was left.
        let selected = self.selected_row();
        self.commands = new_commander()
            .get_user_config()
            .map(|user| Commands::read(&get_env().jj_config, &user));
        if let Ok(commands) = self.commands.as_mut() {
            commands.select_row(selected);
        }
        self.stale = false;

        Ok(())
    }

    /// What the tab shows is the configuration, which a repo that has
    /// moved says nothing about.
    fn mark_stale(&mut self) {}

    fn is_stale(&self) -> bool {
        self.stale
    }

    fn config_changed(&mut self) {
        self.stale = true;
        self.keybinds = CommandsTabKeybinds::new();
    }

    fn toggle_layout(&mut self) {
        self.pane_divider.toggle_layout();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        self.scroll_commands(scroll.distance(self.commands_pane.visible_items()));

        Ok(())
    }

    fn open_context_menu(&self) -> Result<Option<AppAction>> {
        Ok(self.context_menu(self.commands_pane.item_anchor(self.selected_row(), 1)))
    }

    fn main_panel_bindings(&self) -> Vec<Binding> {
        self.keybinds.bindings()
    }

    /// The details panel only says what the selected command is, so
    /// there is nothing to do to it.
    fn details_panel_bindings(&self) -> Vec<Binding> {
        Vec::new()
    }
}

impl Component for CommandsTab {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = self.pane_divider.split(area);

        let (rows, details) = match self.commands.as_ref() {
            Ok(commands) => (
                self.command_lines(commands, chunks[0].width.saturating_sub(2)),
                self.details_text(),
            ),
            Err(err) => (
                error_text("Error getting the configuration", err)?.lines,
                Text::default(),
            ),
        };

        // The hint goes between the corners, with a space to either side.
        let hint_width = chunks[0].width.saturating_sub(4) as usize;
        let block = Block::bordered()
            .title(" Settings / Commands ")
            .title_bottom(
                Line::raw(format!(" {} ", self.keybinds.hint(hint_width)))
                    .centered()
                    .fg(Color::DarkGray),
            )
            .border_type(BorderType::Rounded);
        *self.commands_list_state.selected_mut() = Some(self.selected_row());
        self.commands_pane.render(
            f,
            chunks[0],
            block,
            List::new(rows).scroll_padding(3),
            &mut self.commands_list_state,
        );

        f.render_widget(
            Paragraph::new(details).wrap(Wrap { trim: false }).block(
                Block::bordered()
                    .title(" About ")
                    .border_type(BorderType::Rounded)
                    .padding(Padding::horizontal(1)),
            ),
            chunks[1],
        );

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                return Ok(ComponentInputResult::Handled);
            }

            return match self.keybinds.match_event(key) {
                // Not the tab's to act on, so whoever else wants the key
                // is welcome to it.
                CommandsTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event).into()),
            };
        }

        Ok(ComponentInputResult::Handled)
    }

    fn input_mouse(&mut self, mouse: Mouse) -> Result<ComponentInputResult> {
        if self.pane_divider.handle_mouse(mouse) {
            return Ok(ComponentInputResult::Handled);
        }
        match self.commands_pane.input_mouse(mouse) {
            MouseInput::Scroll(delta) => self.scroll_commands(delta),
            MouseInput::Select(index) => self.select_row(index),
            MouseInput::Context(index) => {
                self.select_row(index);
                return Ok(self.context_menu(Some(mouse.position())).into());
            }
            MouseInput::Copy(text) => return Ok(copy_marked(text)),
            MouseInput::Activate => {
                return Ok(self.change_command_line().into());
            }
            MouseInput::Handled => {}
            MouseInput::NotHandled => return Ok(ComponentInputResult::NotHandled),
        }
        Ok(ComponentInputResult::Handled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_test_env;
    use crate::ui::utils::drawn;

    /// Two commands of your own, one of them in the log menu
    const CONFIG: &str = "blazingjj.context-menu.log = [\"defaults\", \"create-pr\"]\n\
                          blazingjj.commands.show-marked = [\"jj\", \"show\", \"$marked\"]\n\
                          [blazingjj.commands.create-pr]\n\
                          command = [\"gh\", \"pr\", \"create\"]\n\
                          label = \"Create PR\"\n\
                          interactive = true\n";

    /// A tab holding the commands `config` adds, all of them the user's
    /// own, which the tests have in place of a repo to read from.
    fn tab(config: &str) -> CommandsTab {
        set_test_env();
        let mut tab = CommandsTab::new();
        tab.commands = Ok(Commands::read(
            &toml::from_str(config).expect("the configuration parses"),
            &config.parse().expect("the configuration parses"),
        ));
        tab
    }

    /// What the main panel says, as one string per row.
    fn rows(tab: &CommandsTab) -> Vec<String> {
        let commands = tab.commands.as_ref().expect("the configuration was read");

        tab.command_lines(commands, 100)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// What the whole tab says, as one string per row of the terminal.
    fn screen(tab: &mut CommandsTab) -> Vec<String> {
        drawn(tab, 100, 24)
    }

    /// Whether any row of `rows` holds `text`.
    fn says(rows: &[String], text: &str) -> bool {
        rows.iter().any(|row| row.contains(text))
    }

    /// One row per command, saying what it is named by and what it runs,
    /// in the order the names sort in.
    #[test]
    fn every_command_is_listed_by_what_it_is_named_by_and_what_it_runs() {
        let rows = rows(&tab(CONFIG));

        assert_eq!(rows.len(), 2);
        assert!(
            rows[0].starts_with(" create-pr    gh pr create"),
            "{rows:?}"
        );
        assert!(
            rows[1].starts_with(" show-marked  jj show $marked"),
            "{rows:?}"
        );
    }

    /// A list with nothing in it says as much, rather than leaving it to
    /// be read as a list that failed to come.
    #[test]
    fn a_tab_with_no_commands_says_there_are_none() {
        assert!(says(&rows(&tab("")), "No commands of your own"));
    }

    /// The details panel says what the selected command runs, how it is
    /// run and which menus hold it.
    #[test]
    fn the_details_panel_is_about_the_selected_command() {
        let screen = screen(&mut tab(CONFIG));

        assert!(says(&screen, "Create PR"), "{screen:?}");
        assert!(says(&screen, INTERACTIVE), "{screen:?}");
        assert!(says(&screen, "blazingjj.commands.create-pr"), "{screen:?}");
        assert!(says(&screen, "log"), "{screen:?}");
    }

    /// A command no menu holds is a command there is no picking, which
    /// is worth saying where it is listed.
    #[test]
    fn a_command_no_menu_holds_says_so() {
        let mut tab = tab(CONFIG);
        tab.scroll_commands(1);

        let screen = screen(&mut tab);

        assert!(says(&screen, "no menu holds it"), "{screen:?}");
    }

    /// The placeholders are what a command is written with, so the tab
    /// says what each of them stands for rather than leaving it to the
    /// documentation.
    #[test]
    fn the_details_panel_says_what_the_placeholders_stand_for() {
        let screen = screen(&mut tab(CONFIG));

        assert!(says(&screen, "$selected, $s"), "{screen:?}");
        assert!(says(&screen, "$marked, $m"), "{screen:?}");
    }

    /// The keys the tab answers to are worth saying where the list is,
    /// there being no other list of them in it.
    #[test]
    fn the_tab_says_what_it_answers_to() {
        let screen = screen(&mut tab(CONFIG));

        assert!(
            screen
                .iter()
                .any(|row| row.contains("Enter: change") && row.contains("Esc: back")),
            "{screen:?}"
        );
    }

    /// The value the popup asks for is one part of the command, and what
    /// is written is the whole of it, so the rest of it survives being
    /// asked about.
    #[test]
    fn changing_one_part_of_a_command_writes_the_whole_of_it() {
        let command = tab(CONFIG)
            .selected()
            .expect("the tab has a command selected")
            .command
            .clone();

        assert_eq!(
            command
                .with_command_line("gh pr create --web")
                .expect("the command line names a program")
                .value(),
            r#"{ command = ["gh", "pr", "create", "--web"], interactive = true, label = "Create PR" }"#
        );
        assert_eq!(
            command.toggled_interactive().value(),
            r#"{ command = ["gh", "pr", "create"], label = "Create PR" }"#
        );
    }

    /// A command with nothing to say beyond what it runs is written as
    /// the command line on its own, which is what it was configured as.
    #[test]
    fn a_command_with_nothing_else_to_say_is_written_as_a_command_line() {
        let mut tab = tab(CONFIG);
        tab.scroll_commands(1);
        let command = tab
            .selected()
            .expect("the tab has a command selected")
            .command
            .clone();

        assert_eq!(command.value(), r#"["jj", "show", "$marked"]"#);
    }

    /// A name is one config key, so a name that would read as more than
    /// one is refused where it is typed rather than written for jj to
    /// refuse in turn.
    #[test]
    fn a_name_that_is_no_config_key_is_refused() {
        let taken = vec!["create-pr".to_owned()];

        assert_eq!(check_name(" tug ", &taken).ok(), Some("tug".to_owned()));
        assert!(check_name("", &taken).is_err());
        assert!(check_name("   ", &taken).is_err());
        assert!(check_name("mine.own", &taken).is_err());
        assert!(check_name("two words", &taken).is_err());
        assert!(check_name("create-pr", &taken).is_err());
    }

    /// A repo that has moved says nothing about the configuration, so
    /// reading it again is for a configuration that has changed.
    #[test]
    fn the_tab_reads_the_configuration_again_when_it_changes_rather_than_the_repo() {
        let mut tab = tab(CONFIG);
        tab.stale = false;

        tab.mark_stale();
        assert!(!tab.is_stale());

        tab.config_changed();
        assert!(tab.is_stale());
    }

    /// Only what the user's own config file adds is the tab's to take
    /// back out.
    #[test]
    fn a_command_from_elsewhere_in_the_configuration_is_not_the_tabs_to_take_out() {
        set_test_env();
        let mut tab = tab(CONFIG);
        assert!(tab.unset().is_some());

        tab.commands = Ok(Commands::read(
            &toml::from_str(CONFIG).expect("the configuration parses"),
            &toml::Table::new(),
        ));

        assert!(tab.unset().is_none());
    }
}
