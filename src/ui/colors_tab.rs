/*! The colours tab lists every element the app draws and what it is
drawn in, and shows what the selected one is for in the details panel.

It is the settings tab's, opened from the row for `blazingjj.colors` and
left again for it, so it has no place of its own in the tab bar. What it
writes are the keys under that table, in the user's own config file, just
as the settings tab writes the options beside it.
*/

use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::prelude::*;
use ratatui::widgets::*;
use tracing::instrument;

use crate::app::TabId;
use crate::app::command::Command;
use crate::commander::config::config_value;
use crate::commander::new_commander;
use crate::env::check_config_value;
use crate::event::Mouse;
use crate::keybinds::Binding;
use crate::keybinds::ColorsTabEvent;
use crate::keybinds::ColorsTabKeybinds;
use crate::theme::Channel;
use crate::theme::Role;
use crate::theme::ThemeColor;
use crate::theme::theme;
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
use crate::ui::panel::Row as SectionRow;
use crate::ui::panel::Sections;
use crate::ui::panel::copy_marked;
use crate::ui::styles::panel_block;
use crate::ui::styles::panel_title;
use crate::ui::styles::patched;
use crate::ui::styles::section_heading;
use crate::ui::styles::swatch;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::error_text;

/// What every row of the list is indented by, headings apart.
const INDENT: &str = "   ";

/// What a column of colours is given: the widest a colour is written,
/// `bright magenta` and `ansi-color-255` both being fourteen, and the
/// blanks that keep the two columns apart.
const COLOR_WIDTH: usize = 16;

/// The config key `role`'s `channel` is written under.
fn key_of(role: Role, channel: Channel) -> String {
    format!("blazingjj.colors.{}.{}", role.key(), channel.key())
}

/// The table `role`'s colours are written under, which is a key of its
/// own for a role written as the single colour to draw it in.
fn table_of(role: Role) -> String {
    format!("blazingjj.colors.{}", role.key())
}

/// What the user's own config file says about the colours, which is the
/// layer the tab writes and the only one it can take a colour out of.
#[derive(Default)]
struct UserColors {
    /// Its `blazingjj.colors` table, empty where it has none.
    colors: toml::Table,
}

impl UserColors {
    fn read() -> Result<Self> {
        Ok(Self::of(&new_commander().get_user_config()?))
    }

    /// What `config`, the user's own config file, says about the colours.
    fn of(config: &toml::Table) -> Self {
        Self {
            colors: config_value(config, "blazingjj.colors")
                .and_then(toml::Value::as_table)
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// What the user's own config file says about `role`: a table of its
    /// two colours, or the one colour to draw it in, which is its
    /// foreground.
    fn said_about(&self, role: Role) -> Option<&toml::Value> {
        self.colors.get(role.key())
    }

    /// The key a colour asked for `role`'s `channel` is written under.
    /// jj refuses to set any key under a value that is not a table, so a
    /// role written as the one colour to draw it in is written afresh as
    /// a table of both rather than given a key under it.
    fn asked_key(&self, role: Role, channel: Channel) -> String {
        match self.said_alone(role) {
            Some(_) => table_of(role),
            None => key_of(role, channel),
        }
    }

    /// The one colour the user's own config file draws `role` in, for a
    /// role written as that rather than as a table of its two.
    fn said_alone(&self, role: Role) -> Option<&str> {
        match self.said_about(role)? {
            toml::Value::Table(_) => None,
            said => said.as_str(),
        }
    }

    /// Whether `said`, what the user's own config file says about a
    /// role, is what draws its `channel`.
    fn gives(said: Option<&toml::Value>, channel: Channel) -> bool {
        match said {
            Some(toml::Value::Table(colors)) => colors.contains_key(channel.key()),
            Some(_) => channel == Channel::Fg,
            None => false,
        }
    }

    /// Whether the user's own config file is what draws `role`'s
    /// `channel`, which is what makes it the tab's to take back out.
    fn is_users(&self, role: Role, channel: Channel) -> bool {
        Self::gives(self.said_about(role), channel)
    }

    /// The key that takes `role`'s `channel` back out of the user's own
    /// config file, where that is what draws it. A role written as the
    /// single colour to draw it in is a key of its own, so its
    /// foreground goes by taking the role out and there is nothing
    /// under it to take a background out of.
    fn taken_out_by(&self, role: Role, channel: Channel) -> Option<String> {
        self.is_users(role, channel)
            .then(|| self.asked_key(role, channel))
    }

    /// The keys the user's own config file draws `role` in, which are
    /// what there is to take back out of it.
    fn keys_of(&self, role: Role) -> Vec<String> {
        Channel::ALL
            .into_iter()
            .filter_map(|channel| self.taken_out_by(role, channel))
            .collect()
    }
}

pub struct ColorsTab {
    /// What the user's own config file says, or why it could not be read.
    colors: Result<UserColors>,

    /// The elements under the headings they are listed by.
    roles: Sections<Role>,
    roles_pane: ListPane,
    roles_list_state: ListState,

    keybinds: ColorsTabKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

impl ColorsTab {
    /// A stale tab, holding nothing of what the configuration says yet.
    #[instrument(level = "info", name = "Initializing colors tab", parent = None)]
    pub fn new() -> Self {
        Self {
            colors: Ok(UserColors::default()),

            roles: Sections::new(Role::ALL, |role: &Role| role.section()),
            roles_pane: ListPane::default(),
            roles_list_state: ListState::default(),

            keybinds: ColorsTabKeybinds::new(),
            pane_divider: PaneDivider::default(),

            stale: true,
        }
    }

    fn selected(&self) -> Option<Role> {
        self.roles.selected().copied()
    }

    /// Ask for a colour to draw the selected element in.
    fn change_selected(&self, channel: Channel) -> Option<AppAction> {
        let role = self.selected()?;
        let colors = self.colors.as_ref().ok()?;
        // The colour an element written as one was written as is its
        // foreground, so that is what the other one is written beside,
        // in the one spelling it has. One that reads as no colour is
        // written back as it stands rather than dropped.
        let beside = colors.said_alone(role).map(|said| {
            said.parse::<ThemeColor>()
                .map_or_else(|_| said.to_owned(), |color| color.to_string())
        });
        let key = colors.asked_key(role, channel);
        // What it is drawn in now is what to start from, so that a
        // colour is adjusted rather than typed out again.
        let current = theme()
            .color_of(role, channel)
            .map(|color| color.to_string())
            .unwrap_or_default();

        let asked = key.clone();
        Some(AppAction::SetPopup(Box::new(SettingValuePopup::for_key(
            key,
            colors.taken_out_by(role, channel),
            current,
            move |input| color_value(&asked, channel, beside.as_deref(), input),
        ))))
    }

    /// Take the selected element's colours out of the user's config
    /// file, leaving whatever the rest of the configuration says.
    fn unset_selected(&self) -> Option<AppAction> {
        let role = self.selected()?;
        let colors = self.colors.as_ref().ok()?;

        // Only the colours the user's own config gives, a colour at a
        // time: what the rest of the configuration says about the role
        // is not the tab's to take away.
        let taken_out: Vec<AppAction> = colors
            .keys_of(role)
            .into_iter()
            .map(|key| AppAction::Run(Command::UnsetSetting { key }))
            .collect();

        (!taken_out.is_empty()).then(|| AppAction::Multiple(taken_out))
    }

    /// The menu of what can be done to the selected element, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> Option<AppAction> {
        let mut items = vec![
            (
                Line::raw("Change what it is drawn in"),
                self.change_selected(Channel::Fg)?,
            ),
            (
                Line::raw("Change what it is drawn on"),
                self.change_selected(Channel::Bg)?,
            ),
        ];
        if let Some(unset) = self.unset_selected() {
            items.push((Line::raw("Take out of your config"), unset));
        }

        Some(AppAction::SetPopup(Box::new(ChoicePopup::new(
            anchor,
            "Color actions",
            items,
        ))))
    }

    fn handle_event(&mut self, event: ColorsTabEvent) -> Option<AppAction> {
        match event {
            ColorsTabEvent::ChangeForeground => self.change_selected(Channel::Fg),
            ColorsTabEvent::ChangeBackground => self.change_selected(Channel::Bg),
            ColorsTabEvent::Unset => self.unset_selected(),
            ColorsTabEvent::Back => Some(AppAction::ViewTab(TabId::Settings)),
            // Not an operation of its own; the key handler deals with it.
            ColorsTabEvent::Unbound => None,
        }
    }

    /// One row per element: a patch of what it is drawn in, its name,
    /// and the two colours as they are written, under the heading of the
    /// part of the app it belongs to.
    fn roles_lines(&self, colors: &UserColors) -> Vec<Line<'static>> {
        let theme = theme();
        let width = Role::ALL
            .iter()
            .map(|role| role.key().len())
            .max()
            .unwrap_or(0);

        self.roles
            .rows()
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let selected = index == self.roles.selected_row();
                let highlighted = |line| {
                    if selected {
                        patched(line, theme.style(Role::Highlight))
                    } else {
                        line
                    }
                };

                match row {
                    // The indent is no part of the heading, so it is no
                    // part of what is underlined either.
                    SectionRow::Heading(heading) => {
                        highlighted(Line::from(vec![Span::raw(" "), section_heading(*heading)]))
                    }
                    SectionRow::Item(role) => {
                        let by_user = colors.said_about(*role);
                        // What a colour is written as is drawn like the
                        // rest of the list; the swatch is where the
                        // element's own colours are shown.
                        let said = |channel: Channel| {
                            let color = theme.color_of(*role, channel);
                            let text =
                                color.map_or_else(|| "-".to_owned(), |color| color.to_string());
                            let span = Span::raw(format!("{text:COLOR_WIDTH$}"));

                            // What the user's own config does not give is dimmed
                            // as the settings tab dims an option it falls back
                            // on, that being the same thing said of a colour.
                            if UserColors::gives(by_user, channel) {
                                span
                            } else {
                                span.patch_style(theme.style(Role::Hint)).italic()
                            }
                        };

                        let mut line = highlighted(Line::from(vec![
                            Span::raw(INDENT),
                            Span::raw(format!("  {:width$}  ", role.key())),
                            said(Channel::Fg),
                            said(Channel::Bg),
                        ]));
                        // The swatch is what the row is about, so it
                        // keeps the element's own colours even where the
                        // highlight has taken the rest of the row.
                        line.spans.insert(1, swatch(theme.style(*role)));

                        line
                    }
                }
            })
            .collect()
    }

    /// What the selected element is drawn for, what it is drawn in, and
    /// where that comes from.
    fn details_text(&self, colors: &UserColors) -> Text<'static> {
        let Some(role) = self.selected() else {
            return Text::default();
        };
        let theme = theme();
        let mut lines = vec![
            Line::raw(format!("blazingjj.colors.{}", role.key())).bold(),
            Line::raw(""),
            Line::raw(role.doc()),
            Line::raw(""),
        ];

        let by_user = colors.said_about(role);
        for (what, channel) in [("Drawn in", Channel::Fg), ("Drawn on", Channel::Bg)] {
            let color = theme.color_of(role, channel);
            let mut spans = vec![
                Span::raw(format!("{what}:  ")),
                Span::raw(color.map_or_else(
                    || "whatever is underneath".to_owned(),
                    |color| color.to_string(),
                ))
                .bold(),
            ];
            if UserColors::gives(by_user, channel) {
                spans.push(Span::raw("  (in your config)").patch_style(theme.style(Role::Hint)));
            }

            lines.push(Line::from(spans));
        }

        lines.push(Line::raw(""));
        lines.push(
            Line::raw(
                "A color is one of the sixteen names, a #rrggbb code, ansi-color-0 to \
                 ansi-color-255, or \"default\" for the terminal's own.",
            )
            .patch_style(theme.style(Role::Hint)),
        );

        Text::from(lines)
    }
}

/// The TOML expression `input` stands for as `channel`'s colour, refused
/// when it names none. What is typed is written back in the one spelling
/// the colour has, so that the config file reads the same however it was
/// asked for.
///
/// `beside` is the colour the element was written as on its own, which
/// is written back as its foreground beside the one asked for. Asking
/// for the foreground is what replaces it, leaving the element the one
/// colour it was.
fn color_value(key: &str, channel: Channel, beside: Option<&str>, input: &str) -> Result<String> {
    let color: ThemeColor = input.trim().parse()?;
    let value = match beside {
        None => toml::Value::String(color.to_string()),
        Some(beside) => toml::Value::Table(toml::Table::from_iter([
            (
                Channel::Fg.key().to_owned(),
                toml::Value::String(beside.to_owned()),
            ),
            (
                channel.key().to_owned(),
                toml::Value::String(color.to_string()),
            ),
        ])),
    }
    .to_string();
    check_config_value(key, &value)?;

    Ok(value)
}

impl Tab for ColorsTab {
    fn refresh(&mut self) -> Result<()> {
        self.colors = UserColors::read();
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
        self.keybinds = ColorsTabKeybinds::new();
    }

    fn toggle_layout(&mut self) {
        self.pane_divider.toggle_layout();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        self.roles
            .scroll(scroll.distance(self.roles_pane.visible_items()));

        Ok(())
    }

    fn open_context_menu(&self) -> Result<Option<AppAction>> {
        Ok(self.context_menu(self.roles_pane.item_anchor(self.roles.selected_row(), 1)))
    }

    fn main_panel_bindings(&self) -> Vec<Binding> {
        self.keybinds.bindings()
    }

    /// The details panel only says what the selected element is for, so
    /// there is nothing to do to it.
    fn details_panel_bindings(&self) -> Vec<Binding> {
        Vec::new()
    }
}

impl Component for ColorsTab {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = self.pane_divider.split(area);

        let (rows, details) = match self.colors.as_ref() {
            Ok(colors) => (self.roles_lines(colors), self.details_text(colors)),
            Err(err) => (
                error_text("Error getting the configuration", err)?.lines,
                Text::default(),
            ),
        };

        // The hint goes between the corners, with a space to either side.
        let hint_width = chunks[0].width.saturating_sub(4) as usize;
        let block = panel_block()
            .title(panel_title(" Settings / Colors "))
            .title_bottom(
                Line::raw(format!(" {} ", self.keybinds.hint(hint_width)))
                    .centered()
                    .patch_style(Role::Hint.style()),
            );
        *self.roles_list_state.selected_mut() = Some(self.roles.selected_row());
        self.roles_pane.render(
            f,
            chunks[0],
            block,
            List::new(rows).scroll_padding(3),
            &mut self.roles_list_state,
        );

        f.render_widget(
            Paragraph::new(details).wrap(Wrap { trim: false }).block(
                panel_block()
                    .title(panel_title(" About "))
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
                ColorsTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event).into()),
            };
        }

        Ok(ComponentInputResult::Handled)
    }

    fn input_mouse(&mut self, mouse: Mouse) -> Result<ComponentInputResult> {
        if self.pane_divider.handle_mouse(mouse) {
            return Ok(ComponentInputResult::Handled);
        }
        match self.roles_pane.input_mouse(mouse) {
            MouseInput::Scroll(delta) => self.roles.scroll(delta),
            MouseInput::Select(index) => self.roles.select_row(index),
            MouseInput::Context(index) => {
                self.roles.select_row(index);
                return Ok(self.context_menu(Some(mouse.position())).into());
            }
            MouseInput::Copy(text) => return Ok(copy_marked(text)),
            // Nothing here has a second thing a double click could do.
            MouseInput::Activate | MouseInput::Handled => {}
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

    /// A tab holding the colours as they are, with `config` for the
    /// user's own config file, which the tests have in place of a repo
    /// to read one from.
    fn tab(config: &str) -> ColorsTab {
        set_test_env();
        let mut tab = ColorsTab::new();
        tab.colors = Ok(UserColors::of(
            &config.parse().expect("the configuration parses"),
        ));
        tab
    }

    /// What the main panel says, as one string per row.
    fn rows(tab: &ColorsTab) -> Vec<String> {
        let colors = tab.colors.as_ref().expect("the configuration was read");

        tab.roles_lines(colors)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// Move the selection to `role`, which is what the keys act on.
    fn select(tab: &mut ColorsTab, role: Role) {
        for _ in 0..Role::ALL.len() {
            if tab.selected() == Some(role) {
                return;
            }

            tab.roles.scroll(1);
        }

        panic!("the tab lists {}", role.key());
    }

    /// Every element the app draws is listed, under the heading of the
    /// part of the app it belongs to.
    #[test]
    fn every_element_is_listed_under_a_heading() {
        let rows = rows(&tab(""));

        for role in Role::ALL {
            assert!(
                rows.iter().any(|row| row.contains(role.key())),
                "{} is not listed: {rows:?}",
                role.key()
            );
        }
        assert!(rows.iter().any(|row| row.trim() == "The frame"), "{rows:?}");
        assert!(rows.iter().any(|row| row.trim() == "Files"), "{rows:?}");
    }

    /// The elements of a heading come one after another, that being how
    /// the list gathers them under it: one out of place would put its
    /// heading up a second time.
    #[test]
    fn a_heading_is_put_up_once() {
        let headings: Vec<&str> = Role::ALL.map(Role::section).to_vec();

        for (at, heading) in headings.iter().enumerate() {
            assert!(
                !headings[at + 1..].contains(heading) || headings[at + 1] == *heading,
                "{heading:?} is put up twice: {headings:?}"
            );
        }
    }

    /// A row says what the element is drawn in, so that the list is read
    /// rather than worked out.
    #[test]
    fn a_row_says_what_the_element_is_drawn_in() {
        let rows = rows(&tab(""));

        assert!(
            rows.iter()
                .any(|row| row.contains("error") && row.contains("red")),
            "{rows:?}"
        );
    }

    /// The keys the tab answers to are said at the foot of it, which is
    /// where the way out of it is to be found.
    #[test]
    fn the_tab_says_what_it_answers_to() {
        let screen = drawn(&mut tab(""), 100, 30);

        assert!(
            screen
                .iter()
                .any(|row| row.contains("Enter: foreground") && row.contains("Esc: back")),
            "{screen:?}"
        );
    }

    /// Only a colour the user's own config file gives can be taken back
    /// out; one that comes from the scheme or from nowhere is not the
    /// tab's to take out of a layer it does not write.
    #[test]
    fn only_what_the_users_own_config_gives_can_be_taken_back_out() {
        let mut tab = tab("blazingjj.colors.hint = { fg = \"#010203\" }\n");
        select(&mut tab, Role::Hint);
        assert!(tab.unset_selected().is_some());

        select(&mut tab, Role::Error);
        assert!(tab.unset_selected().is_none());
    }

    /// What the keys are is what `jj config unset` is asked to take out,
    /// and it refuses a key whose value is a table. A role written as
    /// one is taken out a colour at a time, and only the colours it
    /// actually gives.
    #[test]
    fn a_role_written_as_a_table_is_taken_out_a_colour_at_a_time() {
        let both = tab("blazingjj.colors.highlight = { fg = \"red\", bg = \"blue\" }\n");
        assert_eq!(
            both.colors.as_ref().unwrap().keys_of(Role::Highlight),
            [
                "blazingjj.colors.highlight.fg",
                "blazingjj.colors.highlight.bg"
            ]
        );

        let one = tab("blazingjj.colors.highlight = { bg = \"blue\" }\n");
        assert_eq!(
            one.colors.as_ref().unwrap().keys_of(Role::Highlight),
            ["blazingjj.colors.highlight.bg"]
        );
    }

    /// A role written as the one colour to draw it in is a key of its
    /// own, which is taken out as it stands.
    #[test]
    fn a_role_written_as_one_colour_is_taken_out_in_one() {
        let tab = tab("blazingjj.colors.hint = \"red\"\n");

        assert_eq!(
            tab.colors.as_ref().unwrap().keys_of(Role::Hint),
            ["blazingjj.colors.hint"]
        );
    }

    /// A role the user's own config says nothing about has nothing to
    /// take out of it.
    #[test]
    fn a_role_the_config_says_nothing_about_has_nothing_to_take_out() {
        let tab = tab("");

        assert!(tab.colors.as_ref().unwrap().keys_of(Role::Hint).is_empty());
    }

    /// A role written as the one colour to draw it in is a foreground,
    /// so that is what the tab reads it as having been given.
    #[test]
    fn a_role_written_as_one_colour_has_been_given_a_foreground() {
        let tab = tab("blazingjj.colors.hint = \"#010203\"\n");
        let colors = tab.colors.as_ref().expect("the configuration was read");

        assert!(colors.is_users(Role::Hint, Channel::Fg));
        assert!(!colors.is_users(Role::Hint, Channel::Bg));
    }

    /// Clearing a colour takes just that one out. A role written as the
    /// one colour to draw it in gives only a foreground, so clearing its
    /// background has nothing to take out rather than the role itself.
    #[test]
    fn clearing_a_colour_takes_out_that_colour_alone() {
        let alone = tab("blazingjj.colors.hint = \"red\"\n");
        let colors = alone.colors.as_ref().expect("the configuration was read");

        assert_eq!(
            colors.taken_out_by(Role::Hint, Channel::Fg).as_deref(),
            Some("blazingjj.colors.hint")
        );
        assert_eq!(colors.taken_out_by(Role::Hint, Channel::Bg), None);

        let both = tab("blazingjj.colors.hint = { fg = \"red\", bg = \"blue\" }\n");
        let colors = both.colors.as_ref().expect("the configuration was read");

        assert_eq!(
            colors.taken_out_by(Role::Hint, Channel::Bg).as_deref(),
            Some("blazingjj.colors.hint.bg")
        );
    }

    /// What is typed is written back in the one spelling a colour has,
    /// so that the config file reads the same however it was asked for.
    #[test]
    fn a_colour_is_written_in_the_one_spelling_it_has() {
        let written = |input| {
            color_value("blazingjj.colors.hint.fg", Channel::Fg, None, input)
                .expect("the colour is one that can be written")
        };

        assert_eq!(written(" BrightBlack "), "\"bright black\"");
        assert_eq!(written("208"), "\"ansi-color-208\"");
        assert_eq!(written("#0A141E"), "\"#0a141e\"");
    }

    /// jj refuses to set any key under a value that is not a table, so
    /// an element written as the one colour to draw it in is written
    /// afresh as a table: of both, keeping the one it had, or of the one
    /// asked for where that is the one it had. The one it had is written
    /// in the one spelling it has, as the one asked for is.
    #[test]
    fn an_element_written_as_one_colour_is_written_afresh_as_a_table() {
        let written = |channel, input| {
            color_value(
                "blazingjj.colors.hint",
                channel,
                Some("bright black"),
                input,
            )
            .expect("the colour is one that can be written")
        };

        assert_eq!(
            written(Channel::Bg, "bright blue"),
            "{ bg = \"bright blue\", fg = \"bright black\" }"
        );
        assert_eq!(
            written(Channel::Fg, "bright blue"),
            "{ fg = \"bright blue\" }"
        );
    }

    /// A colour that names nothing is refused where it is typed, with
    /// what one looks like.
    #[test]
    fn a_colour_that_names_nothing_is_refused() {
        let refusal = color_value("blazingjj.colors.hint.fg", Channel::Fg, None, "chartreuse")
            .expect_err("the colour is not one that can be written")
            .to_string();

        assert!(refusal.contains("#rrggbb"), "{refusal}");
    }

    /// A row is one element rather than one colour of one, so each of
    /// its two colours is written under a key of its own.
    #[test]
    fn each_of_the_two_colours_is_written_under_a_key_of_its_own() {
        assert_eq!(key_of(Role::Hint, Channel::Fg), "blazingjj.colors.hint.fg");
        assert_eq!(key_of(Role::Hint, Channel::Bg), "blazingjj.colors.hint.bg");
    }

    /// jj refuses to set a key under a value that is not a table, so a
    /// role written as the one colour to draw it in is written afresh
    /// as a table rather than given a key under that colour.
    #[test]
    fn a_role_written_as_one_colour_is_written_afresh_rather_than_under() {
        let alone = tab("blazingjj.colors.hint = \"red\"\n");
        let colors = alone.colors.as_ref().expect("the configuration was read");

        for channel in Channel::ALL {
            assert_eq!(
                colors.asked_key(Role::Hint, channel),
                "blazingjj.colors.hint"
            );
        }

        // A role written as a table, or written nothing about, takes the
        // colour under a key of its own.
        for config in ["blazingjj.colors.hint = { fg = \"red\" }\n", ""] {
            let colors = tab(config);
            let colors = colors.colors.as_ref().expect("the configuration was read");

            assert_eq!(
                colors.asked_key(Role::Hint, Channel::Bg),
                "blazingjj.colors.hint.bg"
            );
        }
    }

    /// Asking for a colour asks for one, whichever of the two was asked
    /// for and whether or not the configuration already gives it.
    #[test]
    fn either_colour_of_the_selected_element_can_be_asked_for() {
        let mut tab = tab("blazingjj.colors.hint = \"red\"\n");
        select(&mut tab, Role::Hint);

        for event in [
            ColorsTabEvent::ChangeForeground,
            ColorsTabEvent::ChangeBackground,
        ] {
            assert!(matches!(
                tab.handle_event(event),
                Some(AppAction::SetPopup(_))
            ));
        }
    }

    /// The tab is the settings tab's, so leaving it goes back to the row
    /// it was opened from rather than to wherever the tab bar leads.
    #[test]
    fn leaving_the_tab_goes_back_to_the_settings() {
        let mut tab = tab("");

        assert!(matches!(
            tab.handle_event(ColorsTabEvent::Back),
            Some(AppAction::ViewTab(TabId::Settings))
        ));
    }
}
