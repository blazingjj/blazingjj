/*! Keys the confirmation popup has of its own, for the buttons it puts
up. They are matched after the keys every popup answers to.
*/

use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::config::ConfirmPopupKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ConfirmPopupEvent {
    /// Answer the question, without going by the selected button.
    Answer(bool),
    /// Make `Accept` answer the given way.
    Select(bool),

    Unbound,
}

#[derive(Debug)]
pub struct ConfirmPopupKeybinds {
    keys: KeybindsStore<ConfirmPopupEvent>,
}

impl Default for ConfirmPopupKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<ConfirmPopupEvent>::default();
        set_keybinds!(
            keys,
            ConfirmPopupEvent::Answer(true) => "y",
            ConfirmPopupEvent::Answer(false) => "n",
            ConfirmPopupEvent::Select(true) => "left",
            ConfirmPopupEvent::Select(false) => "right",
        );
        Self { keys }
    }
}

impl ConfirmPopupKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = keybinds_config().and_then(|config| config.confirm_popup.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    pub fn match_event(&self, event: KeyEvent) -> ConfirmPopupEvent {
        self.keys
            .match_event(event)
            .unwrap_or(ConfirmPopupEvent::Unbound)
    }

    /// The shortcut to name `event` by, of those bound to it.
    pub fn shortcut(&self, event: ConfirmPopupEvent) -> Option<Shortcut> {
        self.keys.get_shortcuts(event).into_iter().next()
    }

    fn extend_from_config(&mut self, config: &ConfirmPopupKeybindsConfig) {
        update_keybinds!(
            self.keys,
            ConfirmPopupEvent::Answer(true) => config.yes,
            ConfirmPopupEvent::Answer(false) => config.no,
            ConfirmPopupEvent::Select(true) => config.select_yes,
            ConfirmPopupEvent::Select(false) => config.select_no,
        );
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyCode;
    use ratatui::crossterm::event::KeyModifiers;

    use super::*;
    use crate::keybinds::Keybind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    #[test]
    fn test_match_event_defaults() {
        let keybinds = ConfirmPopupKeybinds::default();

        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('y'))),
            ConfirmPopupEvent::Answer(true)
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Left)),
            ConfirmPopupEvent::Select(true)
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('z'))),
            ConfirmPopupEvent::Unbound
        );
    }

    #[test]
    fn test_extend_from_config_replaces_bindings() {
        let config = ConfirmPopupKeybindsConfig {
            no: Some(Keybind::Single(
                Shortcut::from_str("x").expect("shortcut should parse"),
            )),
            yes: None,
            select_yes: None,
            select_no: None,
        };

        let mut keybinds = ConfirmPopupKeybinds::default();
        keybinds.extend_from_config(&config);

        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('x'))),
            ConfirmPopupEvent::Answer(false)
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('n'))),
            ConfirmPopupEvent::Unbound
        );

        // Anything the config leaves out keeps its default.
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('y'))),
            ConfirmPopupEvent::Answer(true)
        );
    }
}
