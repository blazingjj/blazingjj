/*! The settings tab lists the options the app can be configured with in
the main panel and what the selected one does in the details panel.

An option is a jj config key, so the tab reads the configuration jj
already keeps and writes to the user's own config file. What it shows is
what the configuration says now, across every layer jj reads; what it
takes an option out of is the one layer it writes.
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
use crate::keybinds::SettingsTabEvent;
use crate::keybinds::SettingsTabKeybinds;
use crate::settings::SETTINGS;
use crate::settings::Setting;
use crate::settings::SettingKind;
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
use crate::ui::utils::PaneDivider;
use crate::ui::utils::error_text;

/// What the configuration says about the options the tab shows.
#[derive(Default)]
struct Values {
    /// Everything the configuration sets, across the layers jj reads.
    all: toml::Table,
    /// What the user's own config file sets, which is the layer the tab
    /// writes and the only one it can take a value out of.
    user: toml::Table,
}

impl Values {
    /// What the configuration says now, and what of that the user's own
    /// config file is what says it. What every layer says has been read
    /// into the environment already; which layer a value comes from has
    /// not.
    fn read() -> Result<Self> {
        Ok(Self {
            all: get_env().config.clone(),
            user: new_commander().get_user_config()?,
        })
    }

    /// What `setting` is set to, as it reads on screen.
    fn value(&self, setting: &Setting) -> Option<String> {
        Some(setting.text_of(config_value(&self.all, setting.key)?))
    }

    /// Whether the user's own config file is what sets `setting`, which
    /// is what makes it the tab's to take back out.
    fn is_users(&self, setting: &Setting) -> bool {
        config_value(&self.user, setting.key).is_some()
    }
}

pub struct SettingsTab {
    /// What the configuration says now, or why it could not be read.
    values: Result<Values>,

    /// The options under the headings they are listed by.
    settings: Sections<&'static Setting>,
    settings_pane: ListPane,
    settings_list_state: ListState,

    keybinds: SettingsTabKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

impl SettingsTab {
    /// A stale tab, holding nothing of what the configuration says yet.
    #[instrument(level = "info", name = "Initializing settings tab", parent = None)]
    pub fn new() -> Self {
        Self {
            values: Ok(Values::default()),

            settings: Sections::new(SETTINGS, |setting| setting.section),
            settings_pane: ListPane::default(),
            settings_list_state: ListState::default(),

            keybinds: SettingsTabKeybinds::new(),
            pane_divider: PaneDivider::default(),

            stale: true,
        }
    }

    fn selected(&self) -> Option<&'static Setting> {
        self.settings.selected().copied()
    }

    /// Ask for a new value for the selected option: which of the values
    /// it takes, when it only takes one of a few, or what it is to be.
    fn change_selected(&self) -> Option<AppAction> {
        let setting = self.selected()?;
        if matches!(setting.kind, SettingKind::Keybindings) {
            return Some(AppAction::ViewTab(TabId::Keybindings));
        }

        let values = self.values.as_ref().ok()?;
        let Some(choices) = setting.choices() else {
            return Some(AppAction::SetPopup(Box::new(SettingValuePopup::new(
                setting,
                values.value(setting).unwrap_or_default(),
            ))));
        };

        let items: Vec<_> = choices
            .iter()
            .filter_map(|choice| {
                let value = setting.value_of(choice).ok()?;

                Some((
                    Line::raw(*choice),
                    AppAction::Run(Command::SetSetting {
                        key: setting.key.to_owned(),
                        value,
                    }),
                ))
            })
            .collect();
        // The value the option has now is the one to leave it at, so it
        // is the one the list opens on.
        let current = values
            .value(setting)
            .and_then(|value| choices.iter().position(|choice| *choice == value))
            .unwrap_or(0);

        Some(AppAction::SetPopup(Box::new(
            ChoicePopup::new(
                get_env().jj_config.clone(),
                self.settings_pane
                    .item_anchor(self.settings.selected_row(), 1),
                setting.key,
                items,
            )
            .selected(current),
        )))
    }

    /// Take the selected option out of the user's config file, leaving
    /// whatever the rest of the configuration says.
    fn unset_selected(&self) -> Option<AppAction> {
        let setting = self.selected()?;
        // Taking the keybindings out would be taking out every binding
        // at once, which is the keybindings tab's to do one at a time.
        if matches!(setting.kind, SettingKind::Keybindings) {
            return None;
        }

        self.values
            .as_ref()
            .is_ok_and(|values| values.is_users(setting))
            .then(|| {
                AppAction::Run(Command::UnsetSetting {
                    key: setting.key.to_owned(),
                })
            })
    }

    /// The menu of what can be done to the selected option, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> Option<AppAction> {
        let mut items = vec![(Line::raw("Change"), self.change_selected()?)];
        if let Some(unset) = self.unset_selected() {
            items.push((Line::raw("Take out of your config"), unset));
        }

        Some(AppAction::SetPopup(Box::new(ChoicePopup::new(
            get_env().jj_config.clone(),
            anchor,
            "Setting actions",
            items,
        ))))
    }

    fn handle_event(&mut self, event: SettingsTabEvent) -> Option<AppAction> {
        match event {
            SettingsTabEvent::Change => self.change_selected(),
            SettingsTabEvent::Unset => self.unset_selected(),
            // Not an operation of its own; the key handler deals with it.
            SettingsTabEvent::Unbound => None,
        }
    }

    /// One row per option: its key, and what it is set to or what the
    /// app goes by while it is not, under the heading of the section it
    /// belongs to.
    fn settings_lines(&self, values: &Values) -> Vec<Line<'static>> {
        let width = SETTINGS
            .iter()
            .map(|setting| setting.key.len())
            .max()
            .unwrap_or(0);

        self.settings
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
                    SectionRow::Item(setting) => {
                        let value = match values.value(setting) {
                            Some(value) => Span::raw(value),
                            None => Span::raw(setting.fallback.to_owned())
                                .fg(Color::DarkGray)
                                .italic(),
                        };

                        Line::from(vec![
                            Span::raw(format!("   {:width$}  ", setting.key)),
                            value,
                        ])
                    }
                };

                if index == self.settings.selected_row() {
                    line.bg(get_env().jj_config.highlight_color())
                } else {
                    line
                }
            })
            .collect()
    }

    /// What the selected option does, what it is set to and where that
    /// comes from.
    fn details_text(&self, values: &Values) -> Text<'static> {
        let Some(setting) = self.selected() else {
            return Text::default();
        };
        let mut lines = vec![
            Line::raw(setting.key).bold(),
            Line::raw(""),
            Line::raw(setting.doc),
            Line::raw(""),
        ];

        lines.push(match values.value(setting) {
            Some(value) => Line::from(vec![
                Span::raw("Set to:      "),
                Span::raw(value).bold(),
                Span::raw(if values.is_users(setting) {
                    "  (in your config)"
                } else {
                    "  (elsewhere in your configuration)"
                })
                .fg(Color::DarkGray),
            ]),
            None => Line::raw("Not set."),
        });
        lines.push(Line::raw(format!("When unset:  {}", setting.fallback)));

        if let Some(choices) = setting.choices() {
            lines.push(Line::raw(format!("One of:      {}", choices.join(", "))));
        }

        Text::from(lines)
    }
}

impl Tab for SettingsTab {
    fn refresh(&mut self) -> Result<()> {
        self.values = Values::read();
        self.stale = false;

        Ok(())
    }

    /// What the tab shows is the configuration, which a repo that has
    /// moved says nothing about.
    fn mark_stale(&mut self) {}

    fn config_changed(&mut self) {
        self.stale = true;
        self.keybinds = SettingsTabKeybinds::new();
    }

    fn toggle_layout(&mut self) {
        self.pane_divider.toggle_layout();
    }

    fn is_stale(&self) -> bool {
        self.stale
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        self.settings
            .scroll(scroll.distance(self.settings_pane.visible_items()));

        Ok(())
    }

    fn open_context_menu(&self) -> Result<Option<AppAction>> {
        Ok(self.context_menu(
            self.settings_pane
                .item_anchor(self.settings.selected_row(), 1),
        ))
    }

    fn main_panel_bindings(&self) -> Vec<Binding> {
        self.keybinds.bindings()
    }

    /// The details panel only says what the selected option is, so there
    /// is nothing to do to it.
    fn details_panel_bindings(&self) -> Vec<Binding> {
        Vec::new()
    }
}

impl Component for SettingsTab {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = self.pane_divider.split(area);

        let (settings, details) = match self.values.as_ref() {
            Ok(values) => (self.settings_lines(values), self.details_text(values)),
            Err(err) => (
                error_text("Error getting the configuration", err)?.lines,
                Text::default(),
            ),
        };

        let block = Block::bordered()
            .title(" Settings ")
            .border_type(BorderType::Rounded);
        *self.settings_list_state.selected_mut() = Some(self.settings.selected_row());
        self.settings_pane.render(
            f,
            chunks[0],
            block,
            List::new(settings).scroll_padding(3),
            &mut self.settings_list_state,
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
                SettingsTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event).into()),
            };
        }

        Ok(ComponentInputResult::Handled)
    }

    fn input_mouse(&mut self, mouse: Mouse) -> Result<ComponentInputResult> {
        if self.pane_divider.handle_mouse(mouse) {
            return Ok(ComponentInputResult::Handled);
        }
        match self.settings_pane.input_mouse(mouse) {
            MouseInput::Scroll(delta) => self.settings.scroll(delta),
            MouseInput::Select(index) => self.settings.select_row(index),
            MouseInput::Context(index) => {
                self.settings.select_row(index);
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

    fn tab(config: &str) -> SettingsTab {
        set_test_env();
        let mut tab = SettingsTab::new();
        tab.values = Ok(Values {
            all: config.parse().expect("the configuration parses"),
            user: config.parse().expect("the configuration parses"),
        });
        tab
    }

    /// What the whole tab says, as one string per row of the terminal.
    fn screen(tab: &mut SettingsTab) -> Vec<String> {
        drawn(tab, 100, 20)
    }

    /// What the main panel says, as one string per row.
    fn rows(tab: &SettingsTab) -> Vec<String> {
        let values = tab.values.as_ref().expect("the configuration was read");

        tab.settings_lines(values)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// A repo that has moved says nothing about the configuration, so
    /// reading it again is for a configuration that has changed.
    #[test]
    fn the_tab_reads_the_configuration_again_when_it_changes_rather_than_the_repo() {
        let mut tab = tab("");
        tab.stale = false;

        tab.mark_stale();
        assert!(!tab.is_stale());

        tab.config_changed();
        assert!(tab.is_stale());
    }

    /// The options are listed under what they are about, so that the
    /// one being looked for is found by the heading over it.
    #[test]
    fn every_option_is_listed_under_a_heading() {
        let rows = rows(&tab(""));

        assert!(
            rows.iter().any(|row| row.trim() == "Appearance"),
            "{rows:?}"
        );
        assert!(rows.iter().any(|row| row.trim() == "Diffs"), "{rows:?}");
    }

    #[test]
    fn an_option_the_configuration_says_nothing_about_shows_what_the_app_goes_by() {
        let rows = rows(&tab(""));

        assert!(
            rows.iter()
                .any(|row| row.contains("blazingjj.layout ") && row.ends_with("horizontal")),
            "{rows:?}"
        );
    }

    #[test]
    fn a_string_is_shown_as_it_reads_rather_than_as_it_is_quoted() {
        let rows = rows(&tab("blazingjj.layout = \"vertical\"\n"));

        assert!(
            rows.iter()
                .any(|row| row.contains("blazingjj.layout ") && row.ends_with("vertical")),
            "{rows:?}"
        );
    }

    #[test]
    fn the_details_panel_says_what_the_selected_option_does() {
        let screen = screen(&mut tab("blazingjj.highlight-color = \"#123456\"\n"));

        assert!(
            screen
                .iter()
                .any(|row| row.contains("Background colour of the selected row")),
            "{screen:?}"
        );
    }

    #[test]
    fn only_what_the_users_own_config_sets_can_be_taken_back_out() {
        let mut tab = tab("blazingjj.layout = \"vertical\"\n");
        while !tab
            .selected()
            .is_some_and(|setting| setting.key == "blazingjj.layout")
        {
            tab.settings.scroll(1);
        }

        assert!(tab.unset_selected().is_some());

        // The same value, but coming from a layer the tab does not write.
        tab.values.as_mut().unwrap().user = toml::Table::new();
        assert!(tab.unset_selected().is_none());
    }
}
