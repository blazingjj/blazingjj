/*! The keybindings tab lists every action a key can be bound to in the
main panel and what the selected one does in the details panel.

It is the settings tab's, opened from the row for `blazingjj.keybinds`
and left again for it, so it has no place of its own in the tab bar. What
it writes are the keys under that table, in the user's own config file,
just as the settings tab writes the options beside it.
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
use crate::env::get_env;
use crate::event::Mouse;
use crate::keybinds::Binding;
use crate::keybinds::Context;
use crate::keybinds::KeybindingsTabEvent;
use crate::keybinds::KeybindingsTabKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::BindKey;
use crate::ui::dialog::BindKeyPopup;
use crate::ui::dialog::ChoicePopup;
use crate::ui::panel::ListPane;
use crate::ui::panel::MouseInput;
use crate::ui::panel::PanelMouseInput;
use crate::ui::panel::Row as SectionRow;
use crate::ui::panel::Sections;
use crate::ui::panel::copy_marked;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::error_text;

/// What every row of the list is indented by, headings apart.
const INDENT: &str = "   ";

/// An action the tab can bind a key to: what it does, the config key
/// that binds it, and the keys it answers to as the row shows them.
struct Bindable {
    binding: Binding,
    config_key: String,
    shown_keys: String,
}

impl Bindable {
    /// The action `binding` stands for, for one that is the user's to
    /// bind.
    fn of(binding: Binding) -> Option<Self> {
        let config_key = binding.key()?;
        let shown_keys = binding.keys_text();

        Some(Self {
            binding,
            config_key,
            shown_keys,
        })
    }
}

/// What there is to bind and what the configuration binds it to.
#[derive(Default)]
struct Bindings {
    /// The actions under the context they take effect in.
    rows: Sections<Bindable>,
    /// What the user's own config file binds, which is the layer the tab
    /// writes and the only one it can take a binding out of.
    user: toml::Table,
}

impl Bindings {
    /// Every action there is to bind, gathered under the context it
    /// takes effect in. An action the user cannot bind is left out.
    fn read(user: toml::Table) -> Self {
        let bindings = Context::ORDER
            .into_iter()
            .flat_map(Context::bindings)
            .filter_map(Bindable::of);

        Self {
            rows: Sections::new(bindings, |bindable| bindable.binding.context.title()),
            user,
        }
    }

    /// Whether the user's own config file is what binds `bindable`,
    /// which is what makes it the tab's to take back out.
    fn is_users(&self, bindable: &Bindable) -> bool {
        config_value(&self.user, &bindable.config_key).is_some()
    }
}

pub struct KeybindingsTab {
    /// What there is to bind, or why the configuration could not be read.
    bindings: Result<Bindings>,

    bindings_pane: ListPane,
    bindings_list_state: ListState,

    keybinds: KeybindingsTabKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

impl KeybindingsTab {
    /// A stale tab, holding nothing of what the configuration binds yet.
    #[instrument(level = "info", name = "Initializing keybindings tab", parent = None)]
    pub fn new() -> Self {
        Self {
            bindings: Ok(Bindings::default()),

            bindings_pane: ListPane::default(),
            bindings_list_state: ListState::default(),

            keybinds: KeybindingsTabKeybinds::new(),
            pane_divider: PaneDivider::default(),

            stale: true,
        }
    }

    fn selected(&self) -> Option<&Bindable> {
        self.bindings.as_ref().ok()?.rows.selected()
    }

    /// Move the selection by `scroll` rows, which a tab that could not
    /// read the configuration has none of.
    fn scroll_bindings(&mut self, scroll: isize) {
        if let Ok(bindings) = self.bindings.as_mut() {
            bindings.rows.scroll(scroll);
        }
    }

    /// Which row is selected, of the headings and the actions alike.
    fn selected_row(&self) -> usize {
        self.bindings
            .as_ref()
            .map_or(0, |bindings| bindings.rows.selected_row())
    }

    /// Select the row at `index`, which is the mouse landing on it.
    fn select_row(&mut self, index: usize) {
        if let Ok(bindings) = self.bindings.as_mut() {
            bindings.rows.select_row(index);
        }
    }

    /// Ask for a key the selected action is to answer to.
    fn bind_selected(&self, bind: BindKey) -> Option<AppAction> {
        let bindable = self.selected()?;

        Some(AppAction::SetPopup(Box::new(BindKeyPopup::new(
            bindable.binding.clone(),
            bind,
        ))))
    }

    /// Leave the selected action bound to nothing, which is what the
    /// configuration says of an action it disables.
    fn disable_selected(&self) -> Option<AppAction> {
        let bindable = self.selected()?;

        (!bindable.binding.keys.is_empty()).then(|| {
            AppAction::Run(Command::SetSetting {
                key: bindable.config_key.clone(),
                value: "false".to_owned(),
            })
        })
    }

    /// Take the selected binding out of the user's config file, leaving
    /// the action bound to what it comes bound to.
    fn unset_selected(&self) -> Option<AppAction> {
        let bindings = self.bindings.as_ref().ok()?;
        let bindable = self.selected()?;

        bindings.is_users(bindable).then(|| {
            AppAction::Run(Command::UnsetSetting {
                key: bindable.config_key.clone(),
            })
        })
    }

    /// The menu of what can be done to the selected action, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> Option<AppAction> {
        let mut items = vec![(Line::raw("Bind a key"), self.bind_selected(BindKey::Only)?)];
        // Another key beside none is just the one key, which the menu
        // offers already.
        if self
            .selected()
            .is_some_and(|bindable| !bindable.binding.keys.is_empty())
            && let Some(besides) = self.bind_selected(BindKey::Besides)
        {
            items.push((Line::raw("Bind another key besides"), besides));
        }
        if let Some(disable) = self.disable_selected() {
            items.push((Line::raw("Leave it bound to nothing"), disable));
        }
        if let Some(unset) = self.unset_selected() {
            items.push((Line::raw("Take out of your config"), unset));
        }

        Some(AppAction::SetPopup(Box::new(ChoicePopup::new(
            get_env().jj_config.clone(),
            anchor,
            "Keybinding actions",
            items,
        ))))
    }

    fn handle_event(&mut self, event: KeybindingsTabEvent) -> Option<AppAction> {
        match event {
            KeybindingsTabEvent::Bind => self.bind_selected(BindKey::Only),
            KeybindingsTabEvent::BindBesides => self.bind_selected(BindKey::Besides),
            KeybindingsTabEvent::Disable => self.disable_selected(),
            KeybindingsTabEvent::Unset => self.unset_selected(),
            KeybindingsTabEvent::Back => Some(AppAction::ViewTab(TabId::Settings)),
            // Not an operation of its own; the key handler deals with it.
            KeybindingsTabEvent::Unbound => None,
        }
    }

    /// One row per action: what it does and the keys it answers to,
    /// under the heading of the context it takes effect in, in a
    /// panel `width` columns wide.
    fn binding_lines(&self, bindings: &Bindings, width: u16) -> Vec<Line<'static>> {
        let keys_width = bindings
            .rows
            .rows()
            .iter()
            .filter_map(|row| match row {
                SectionRow::Item(bindable) => Some(bindable.shown_keys.chars().count()),
                SectionRow::Heading(_) => None,
            })
            .max()
            .unwrap_or(0);
        // What an action does is what the list is read by, so it takes
        // what is left of the row once the keys have their column.
        let description_width = (width as usize).saturating_sub(INDENT.len() + keys_width + 2);

        bindings
            .rows
            .rows()
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let line = match row {
                    // The indent is no part of the heading, so it is no
                    // part of what is underlined either.
                    SectionRow::Heading(heading) => Line::from(vec![
                        Span::raw(" "),
                        Span::raw(*heading).bold().underlined(),
                    ]),
                    SectionRow::Item(bindable) => {
                        let description: String = bindable
                            .binding
                            .description
                            .chars()
                            .take(description_width)
                            .collect();
                        let keys = Span::raw(bindable.shown_keys.clone());
                        let keys = if bindable.binding.keys.is_empty() {
                            keys.fg(Color::DarkGray)
                        } else {
                            keys
                        };

                        Line::from(vec![
                            Span::raw(format!("{INDENT}{description:description_width$}  ")),
                            keys,
                        ])
                    }
                };

                if index == bindings.rows.selected_row() {
                    line.bg(get_env().jj_config.highlight_color())
                } else {
                    line
                }
            })
            .collect()
    }

    /// What the selected action does, what it answers to and where that
    /// comes from.
    fn details_text(&self, bindings: &Bindings) -> Text<'static> {
        let Some(bindable) = self.selected() else {
            return Text::default();
        };

        // A binding the app did not come with and your own config does
        // not hold comes from a layer of the configuration the tab does
        // not write, the repo's among them.
        let source = if bindings.is_users(bindable) {
            "  (in your config)"
        } else if bindable.binding.keys == bindable.binding.defaults {
            "  (default)"
        } else {
            "  (elsewhere in your configuration)"
        };

        Text::from(vec![
            Line::raw(bindable.binding.description).bold(),
            Line::raw(""),
            Line::raw(format!(
                "Takes effect:    {}",
                bindable.binding.context.title()
            )),
            Line::raw(format!("Configured as:   {}", bindable.config_key)),
            Line::raw(""),
            Line::from(vec![
                Span::raw("Current binding: "),
                Span::raw(bindable.shown_keys.clone()).bold(),
                Span::raw(source).fg(Color::DarkGray),
            ]),
            Line::raw(format!(
                "Default binding: {}",
                bindable.binding.defaults_text()
            )),
        ])
    }
}

impl Tab for KeybindingsTab {
    fn refresh(&mut self) -> Result<()> {
        // Reading the bindings again is what a rebinding asks for, and
        // what was rebound is what is selected, so the list comes back
        // with the selection where it was left.
        let selected = self.selected_row();
        self.bindings = new_commander().get_user_config().map(Bindings::read);
        if let Ok(bindings) = self.bindings.as_mut() {
            bindings.rows.select_row(selected);
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
        self.keybinds = KeybindingsTabKeybinds::new();
    }

    fn toggle_layout(&mut self) {
        self.pane_divider.toggle_layout();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        self.scroll_bindings(scroll.distance(self.bindings_pane.visible_items()));

        Ok(())
    }

    fn open_context_menu(&self) -> Result<Option<AppAction>> {
        Ok(self.context_menu(self.bindings_pane.item_anchor(self.selected_row(), 1)))
    }

    fn main_panel_bindings(&self) -> Vec<Binding> {
        self.keybinds.bindings()
    }

    /// The details panel only says what the selected action does, so
    /// there is nothing to do to it.
    fn details_panel_bindings(&self) -> Vec<Binding> {
        Vec::new()
    }
}

impl Component for KeybindingsTab {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = self.pane_divider.split(area);

        let (rows, details) = match self.bindings.as_ref() {
            Ok(bindings) => (
                self.binding_lines(bindings, chunks[0].width.saturating_sub(2)),
                self.details_text(bindings),
            ),
            Err(err) => (
                error_text("Error getting the configuration", err)?.lines,
                Text::default(),
            ),
        };

        // The keys of the tab itself are in the list like any others,
        // which is nowhere to look for the key that gets you out of it.
        // The hint goes between the corners, with a space to either side.
        let hint_width = chunks[0].width.saturating_sub(4) as usize;
        let block = Block::bordered()
            .title(" Settings / Keybindings ")
            .title_bottom(
                Line::raw(format!(" {} ", self.keybinds.hint(hint_width)))
                    .centered()
                    .fg(Color::DarkGray),
            )
            .border_type(BorderType::Rounded);
        *self.bindings_list_state.selected_mut() = Some(self.selected_row());
        self.bindings_pane.render(
            f,
            chunks[0],
            block,
            List::new(rows).scroll_padding(3),
            &mut self.bindings_list_state,
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
                KeybindingsTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event).into()),
            };
        }

        Ok(ComponentInputResult::Handled)
    }

    fn input_mouse(&mut self, mouse: Mouse) -> Result<ComponentInputResult> {
        if self.pane_divider.handle_mouse(mouse) {
            return Ok(ComponentInputResult::Handled);
        }
        match self.bindings_pane.input_mouse(mouse) {
            MouseInput::Scroll(delta) => self.scroll_bindings(delta),
            MouseInput::Select(index) => self.select_row(index),
            MouseInput::Context(index) => {
                self.select_row(index);
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

    /// A tab holding the bindings as they are, with `config` for the
    /// user's own config file, which the tests have in place of a repo
    /// to read one from.
    fn tab(config: &str) -> KeybindingsTab {
        set_test_env();
        let mut tab = KeybindingsTab::new();
        tab.bindings = Ok(Bindings::read(
            config.parse().expect("the configuration parses"),
        ));
        tab
    }

    /// What the main panel says, as one string per row.
    fn rows(tab: &KeybindingsTab) -> Vec<String> {
        let bindings = tab.bindings.as_ref().expect("the configuration was read");

        tab.binding_lines(bindings, 100)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// What the whole tab says, as one string per row of the terminal.
    fn screen(tab: &mut KeybindingsTab) -> Vec<String> {
        drawn(tab, 100, 20)
    }

    /// The keys the tab itself answers to are listed in it like any
    /// others, which is nowhere to look for the way out of it.
    #[test]
    fn the_tab_says_what_it_answers_to() {
        let screen = screen(&mut tab(""));

        assert!(
            screen
                .iter()
                .any(|row| row.contains("Enter: bind") && row.contains("Esc: back")),
            "{screen:?}"
        );
    }

    /// The details panel says what the selected action does and what it
    /// answers to.
    #[test]
    fn the_details_panel_is_about_the_selected_action() {
        let screen = screen(&mut tab(""));

        assert!(
            screen.iter().any(|row| row.contains("Everywhere")),
            "{screen:?}"
        );
        assert!(
            screen
                .iter()
                .any(|row| row.contains("blazingjj.keybinds.scroll-down")),
            "{screen:?}"
        );
    }

    #[test]
    fn every_context_heads_the_actions_it_takes_the_keys_of() {
        let rows = rows(&tab(""));

        assert!(rows.iter().any(|row| row.trim() == "Log tab"), "{rows:?}");
        assert!(
            rows.iter().any(|row| row.contains("abandon change")),
            "{rows:?}"
        );
    }

    /// Going to the top lands on the first action rather than on the
    /// heading above it, which is the first row there is.
    #[test]
    fn going_to_either_end_lands_on_an_action() {
        let mut tab = tab("");

        tab.scroll_bindings(isize::MAX);
        assert_eq!(tab.selected_row(), rows(&tab).len() - 1);

        tab.scroll_bindings(-isize::MAX);
        assert_eq!(tab.selected_row(), 1);
    }

    #[test]
    fn the_selection_passes_over_the_headings_rather_than_onto_them() {
        let mut tab = tab("");

        for _ in 0..rows(&tab).len() {
            assert!(tab.selected().is_some(), "row {}", tab.selected_row());
            tab.scroll_bindings(1);
        }
        for _ in 0..rows(&tab).len() {
            assert!(tab.selected().is_some(), "row {}", tab.selected_row());
            tab.scroll_bindings(-1);
        }
    }

    /// Only what your own config binds is yours to take back out; the
    /// keys the app ships with are not in it to begin with.
    #[test]
    fn a_binding_is_only_taken_out_of_the_config_that_holds_it() {
        let mut tab = tab("blazingjj.keybinds.log-tab.abandon = \"ctrl+a\"\n");
        while !tab
            .selected()
            .is_some_and(|bindable| bindable.binding.description == "abandon change")
        {
            tab.scroll_bindings(1);
        }
        assert!(tab.unset_selected().is_some());

        tab.scroll_bindings(1);
        assert!(tab.unset_selected().is_none());
    }

    /// What the two operations write is what the configuration reads as
    /// an action bound to nothing, and as an action the user's config
    /// says nothing about.
    #[test]
    fn taking_a_binding_out_and_leaving_it_bound_to_nothing_write_the_key() {
        let mut tab = tab("blazingjj.keybinds.log-tab.abandon = \"ctrl+a\"\n");
        while !tab
            .selected()
            .is_some_and(|bindable| bindable.binding.description == "abandon change")
        {
            tab.scroll_bindings(1);
        }

        assert!(matches!(
            tab.disable_selected(),
            Some(AppAction::Run(Command::SetSetting { key, value }))
                if key == "blazingjj.keybinds.log-tab.abandon" && value == "false"
        ));
        assert!(matches!(
            tab.unset_selected(),
            Some(AppAction::Run(Command::UnsetSetting { key }))
                if key == "blazingjj.keybinds.log-tab.abandon"
        ));
    }

    /// A repo that has moved says nothing about the bindings, so
    /// reading them again is for a configuration that has changed.
    #[test]
    fn the_tab_reads_the_bindings_again_when_the_configuration_changes() {
        let mut tab = tab("");
        tab.stale = false;

        tab.mark_stale();
        assert!(!tab.is_stale());

        tab.config_changed();
        assert!(tab.is_stale());
    }
}
