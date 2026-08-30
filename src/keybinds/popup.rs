use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::config::KeybindsConfig;
use super::config::TextPopupKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct PopupKeybinds {
    keys: KeybindsStore<PopupEvent>,
}

/// Keys that mean the same thing in every popup.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PopupEvent {
    Accept,
    Cancel,

    ScrollDown,
    ScrollUp,
    ScrollDownHalf,
    ScrollUpHalf,
    ScrollDownPage,
    ScrollUpPage,

    Unbound,
}

impl PopupKeybinds {
    /// Keys for a popup that lists what there is to pick from.
    pub fn dialog() -> Self {
        let mut keys = KeybindsStore::<PopupEvent>::default();
        set_keybinds!(
            keys,
            PopupEvent::Accept => "enter",
            PopupEvent::Cancel => "esc",
            PopupEvent::Cancel => "q",
            PopupEvent::ScrollDown => "j",
            PopupEvent::ScrollDown => "down",
            PopupEvent::ScrollUp => "k",
            PopupEvent::ScrollUp => "up",
            PopupEvent::ScrollDownHalf => "ctrl+d",
            PopupEvent::ScrollUpHalf => "ctrl+u",
            PopupEvent::ScrollDownPage => "ctrl+f",
            PopupEvent::ScrollDownPage => "space",
            PopupEvent::ScrollDownPage => "pagedown",
            PopupEvent::ScrollUpPage => "ctrl+b",
            PopupEvent::ScrollUpPage => "pageup",
        );
        let mut keybinds = Self { keys };
        if let Some(config) = keybinds_config() {
            keybinds.extend_dialog_from_config(config);
        }
        keybinds
    }

    /// Keys for a popup holding a text field, where every key the field
    /// can take is the field's.
    pub fn text() -> Self {
        Self::text_field(false)
    }

    /// Keys for a popup holding a text field of a single line, which has
    /// no newline to put an Enter in.
    pub fn text_line() -> Self {
        Self::text_field(true)
    }

    fn text_field(single_line: bool) -> Self {
        let mut keys = KeybindsStore::<PopupEvent>::default();
        set_keybinds!(
            keys,
            PopupEvent::Accept => "ctrl+s",
            PopupEvent::Cancel => "esc",
        );
        if single_line {
            keys.add_action(
                Shortcut::from_str("enter").expect("shortcut should parse"),
                PopupEvent::Accept,
            );
        }

        let mut keybinds = Self { keys };
        if let Some(config) = keybinds_config().and_then(|config| config.text_popup.as_ref()) {
            keybinds.extend_text_from_config(config);
        }
        keybinds
    }

    pub fn match_event(&self, event: KeyEvent) -> PopupEvent {
        self.keys.match_event(event).unwrap_or(PopupEvent::Unbound)
    }

    /// The line under a popup saying what it answers to, with `accept`
    /// naming what accepting it does.
    pub fn hint(&self, accept: &str) -> String {
        [(PopupEvent::Accept, accept), (PopupEvent::Cancel, "cancel")]
            .into_iter()
            .filter_map(|(event, what)| Some(format!("{}: {what}", self.shortcut(event)?)))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// The same for a popup that is scrolled as well.
    pub fn scroll_hint(&self, accept: &str) -> String {
        let scroll = self
            .shortcut(PopupEvent::ScrollDown)
            .zip(self.shortcut(PopupEvent::ScrollUp))
            .map(|(down, up)| format!("{down}/{up}: scroll | "))
            .unwrap_or_default();

        format!("{scroll}{}", self.hint(accept))
    }

    /// The shortcut to name `event` by, of those bound to it.
    fn shortcut(&self, event: PopupEvent) -> Option<Shortcut> {
        self.keys.get_shortcuts(event).into_iter().next()
    }

    fn extend_dialog_from_config(&mut self, config: &KeybindsConfig) {
        // A line at a time is a line at a time wherever one is scrolled.
        update_keybinds!(
            self.keys,
            PopupEvent::ScrollDown => config.scroll_down,
            PopupEvent::ScrollUp => config.scroll_up,
        );

        let Some(config) = config.popup.as_ref() else {
            return;
        };
        update_keybinds!(
            self.keys,
            PopupEvent::Accept => config.accept,
            PopupEvent::Cancel => config.cancel,
            PopupEvent::ScrollDownHalf => config.scroll_down_half,
            PopupEvent::ScrollUpHalf => config.scroll_up_half,
            PopupEvent::ScrollDownPage => config.scroll_down_page,
            PopupEvent::ScrollUpPage => config.scroll_up_page,
        );
    }

    fn extend_text_from_config(&mut self, config: &TextPopupKeybindsConfig) {
        update_keybinds!(
            self.keys,
            PopupEvent::Accept => config.accept,
            PopupEvent::Cancel => config.cancel,
        );
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyCode;
    use ratatui::crossterm::event::KeyModifiers;

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn test_dialog_defaults() {
        let keybinds = PopupKeybinds::dialog();

        assert_eq!(
            keybinds.match_event(key(KeyCode::Enter)),
            PopupEvent::Accept
        );
        assert_eq!(keybinds.match_event(key(KeyCode::Esc)), PopupEvent::Cancel);
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('q'))),
            PopupEvent::Cancel
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('j'))),
            PopupEvent::ScrollDown
        );
        assert_eq!(
            keybinds.match_event(ctrl(KeyCode::Char('d'))),
            PopupEvent::ScrollDownHalf
        );
    }

    #[test]
    fn test_the_hint_names_one_shortcut_per_action() {
        assert_eq!(
            PopupKeybinds::dialog().scroll_hint("select"),
            "j/k: scroll | Enter: select | Esc: cancel"
        );
        assert_eq!(
            PopupKeybinds::text().hint("accept"),
            "Control+s: accept | Esc: cancel"
        );
    }

    #[test]
    fn test_a_text_field_keeps_the_printable_keys() {
        let keybinds = PopupKeybinds::text();

        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('q'))),
            PopupEvent::Unbound
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('j'))),
            PopupEvent::Unbound
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Enter)),
            PopupEvent::Unbound
        );
        assert_eq!(
            keybinds.match_event(ctrl(KeyCode::Char('s'))),
            PopupEvent::Accept
        );
        assert_eq!(keybinds.match_event(key(KeyCode::Esc)), PopupEvent::Cancel);
    }

    #[test]
    fn test_a_single_line_field_accepts_on_enter() {
        let keybinds = PopupKeybinds::text_line();

        assert_eq!(
            keybinds.match_event(key(KeyCode::Enter)),
            PopupEvent::Accept
        );
        assert_eq!(
            keybinds.match_event(ctrl(KeyCode::Char('s'))),
            PopupEvent::Accept
        );
    }
}
