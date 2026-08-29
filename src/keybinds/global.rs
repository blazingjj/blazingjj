use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::config::KeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::make_keybinds_help;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct GlobalKeybinds {
    keys: KeybindsStore<GlobalEvent>,
}

/// Keys that mean the same thing in every tab.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GlobalEvent {
    ScrollDown,
    ScrollUp,
    ScrollDownHalf,
    ScrollUpHalf,

    FocusCurrent,
    Refresh,

    NextTab,
    PrevTab,
    // Not configurable: a tab is selected by its number in the tab bar.
    LogTab,
    FilesTab,
    BookmarksTab,
    EvologTab,

    CommandPopup,
    OpenHelp,
    Quit,

    Unbound,
}

impl Default for GlobalKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<GlobalEvent>::default();
        set_keybinds!(
            keys,
            GlobalEvent::ScrollDown => "j",
            GlobalEvent::ScrollDown => "down",
            GlobalEvent::ScrollUp => "k",
            GlobalEvent::ScrollUp => "up",
            GlobalEvent::ScrollDownHalf => "shift+j",
            GlobalEvent::ScrollUpHalf => "shift+k",
            GlobalEvent::FocusCurrent => "@",
            GlobalEvent::Refresh => "shift+r",
            GlobalEvent::Refresh => "f5",
            GlobalEvent::NextTab => "l",
            GlobalEvent::PrevTab => "h",
            GlobalEvent::LogTab => "1",
            GlobalEvent::FilesTab => "2",
            GlobalEvent::BookmarksTab => "3",
            GlobalEvent::EvologTab => "4",
            GlobalEvent::CommandPopup => ":",
            GlobalEvent::OpenHelp => "?",
            GlobalEvent::Quit => "q",
            GlobalEvent::Quit => "ctrl+c",
            GlobalEvent::Quit => "esc",
        );
        Self { keys }
    }
}

impl GlobalKeybinds {
    pub fn match_event(&self, event: KeyEvent) -> GlobalEvent {
        self.keys.match_event(event).unwrap_or(GlobalEvent::Unbound)
    }

    pub fn extend_from_config(&mut self, config: &KeybindsConfig) {
        update_keybinds!(
            self.keys,
            GlobalEvent::ScrollDown => config.scroll_down,
            GlobalEvent::ScrollUp => config.scroll_up,
            GlobalEvent::ScrollDownHalf => config.scroll_down_half,
            GlobalEvent::ScrollUpHalf => config.scroll_up_half,
            GlobalEvent::FocusCurrent => config.focus_current,
            GlobalEvent::Refresh => config.refresh,
            GlobalEvent::OpenHelp => config.open_help,
            GlobalEvent::NextTab => config.next_tab,
            GlobalEvent::PrevTab => config.prev_tab,
            GlobalEvent::CommandPopup => config.command_popup,
            GlobalEvent::Quit => config.quit,
        );
    }

    pub fn make_help(&self) -> Vec<(String, String)> {
        make_keybinds_help!(
            self.keys,
            GlobalEvent::ScrollDown => "scroll down",
            GlobalEvent::ScrollUp => "scroll up",
            GlobalEvent::ScrollDownHalf => "scroll down by ½ page",
            GlobalEvent::ScrollUpHalf => "scroll up by ½ page",
            GlobalEvent::FocusCurrent => "go to current change",
            GlobalEvent::Refresh => "refresh",
            GlobalEvent::NextTab => "next tab",
            GlobalEvent::PrevTab => "previous tab",
            GlobalEvent::LogTab => "log tab",
            GlobalEvent::FilesTab => "files tab",
            GlobalEvent::BookmarksTab => "bookmarks tab",
            GlobalEvent::EvologTab => "evolog tab",
            GlobalEvent::CommandPopup => "run jj command",
            GlobalEvent::OpenHelp => "open help",
            GlobalEvent::Quit => "quit",
        )
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

    fn shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }

    fn bind(shortcut: &str) -> Keybind {
        Keybind::Single(Shortcut::from_str(shortcut).expect("shortcut should parse"))
    }

    #[test]
    fn test_match_event_defaults() {
        let keybinds = GlobalKeybinds::default();

        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('j'))),
            GlobalEvent::ScrollDown
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Down)),
            GlobalEvent::ScrollDown
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('@'))),
            GlobalEvent::FocusCurrent
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::F(5))),
            GlobalEvent::Refresh
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('2'))),
            GlobalEvent::FilesTab
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char(':'))),
            GlobalEvent::CommandPopup
        );
        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            GlobalEvent::Quit
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('z'))),
            GlobalEvent::Unbound
        );
    }

    #[test]
    fn test_shifted_keys_are_distinct() {
        let keybinds = GlobalKeybinds::default();

        // Terminals report a shifted character in upper case, which the store
        // normalizes, so the two must still end up as different events.
        assert_eq!(
            keybinds.match_event(shift(KeyCode::Char('J'))),
            GlobalEvent::ScrollDownHalf
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('j'))),
            GlobalEvent::ScrollDown
        );
    }

    #[test]
    fn test_extend_from_config_replaces_bindings() {
        let config = KeybindsConfig {
            refresh: Some(bind("ctrl+r")),
            quit: Some(bind("x")),
            command_popup: Some(bind("ctrl+p")),
            prev_tab: Some(bind("pageup")),
            ..Default::default()
        };

        let mut keybinds = GlobalKeybinds::default();
        keybinds.extend_from_config(&config);

        // A replaced binding drops every default shortcut for its event.
        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)),
            GlobalEvent::Refresh
        );
        assert_eq!(
            keybinds.match_event(shift(KeyCode::Char('R'))),
            GlobalEvent::Unbound
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::F(5))),
            GlobalEvent::Unbound
        );

        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('x'))),
            GlobalEvent::Quit
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('q'))),
            GlobalEvent::Unbound
        );

        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            GlobalEvent::CommandPopup
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::PageUp)),
            GlobalEvent::PrevTab
        );

        // Anything the config leaves out keeps its default.
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('l'))),
            GlobalEvent::NextTab
        );
    }

    #[test]
    fn test_extend_from_config_disables_bindings() {
        let config = KeybindsConfig {
            open_help: Some(Keybind::Enable(false)),
            ..Default::default()
        };

        let mut keybinds = GlobalKeybinds::default();
        keybinds.extend_from_config(&config);

        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('?'))),
            GlobalEvent::Unbound
        );
        assert!(
            keybinds
                .make_help()
                .contains(&("[disabled]".to_owned(), "open help".to_owned()))
        );
    }

    #[test]
    fn test_make_help_lists_every_default_binding() {
        let help = GlobalKeybinds::default().make_help();

        assert!(help.iter().all(|(keys, _)| keys != "[disabled]"));
        let (keys, _) = help
            .iter()
            .find(|(_, desc)| desc == "refresh")
            .expect("refresh should be listed");
        assert_eq!(keys, "Shift+r/F5");
    }
}
