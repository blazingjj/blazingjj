use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::KeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
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
    ScrollToTop,
    ScrollToBottom,

    FocusCurrent,
    Refresh,
    /// Comes unbound, so the key it answers to is the user's to pick.
    ToggleLayout,

    NextTab,
    PrevTab,
    // Not configurable: a tab is selected by its number in the tab bar.
    LogTab,
    FilesTab,
    BookmarksTab,
    EvologTab,
    OpLogTab,
    SettingsTab,

    OpenContextMenu,
    CommandPopup,
    InteractiveCommandPopup,
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
            GlobalEvent::ScrollToTop => "ctrl+home",
            GlobalEvent::ScrollToBottom => "ctrl+end",
            GlobalEvent::FocusCurrent => "@",
            GlobalEvent::Refresh => "shift+r",
            GlobalEvent::Refresh => "f5",
            GlobalEvent::NextTab => "l",
            GlobalEvent::PrevTab => "h",
            GlobalEvent::LogTab => "1",
            GlobalEvent::FilesTab => "2",
            GlobalEvent::BookmarksTab => "3",
            GlobalEvent::EvologTab => "4",
            GlobalEvent::OpLogTab => "5",
            GlobalEvent::SettingsTab => "0",
            GlobalEvent::OpenContextMenu => "menu",
            GlobalEvent::CommandPopup => ":",
            GlobalEvent::InteractiveCommandPopup => "!",
            GlobalEvent::OpenHelp => "?",
            GlobalEvent::Quit => "q",
            GlobalEvent::Quit => "ctrl+c",
        );
        Self { keys }
    }
}

impl GlobalKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    pub fn match_event(&self, event: KeyEvent) -> GlobalEvent {
        self.keys.match_event(event).unwrap_or(GlobalEvent::Unbound)
    }

    fn extend_from_config(&mut self, config: &KeybindsConfig) {
        update_keybinds!(
            self.keys,
            GlobalEvent::ScrollDown => config.scroll_down,
            GlobalEvent::ScrollUp => config.scroll_up,
            GlobalEvent::ScrollDownHalf => config.scroll_down_half,
            GlobalEvent::ScrollUpHalf => config.scroll_up_half,
            GlobalEvent::ScrollToTop => config.scroll_to_top,
            GlobalEvent::ScrollToBottom => config.scroll_to_bottom,
            GlobalEvent::FocusCurrent => config.focus_current,
            GlobalEvent::Refresh => config.refresh,
            GlobalEvent::ToggleLayout => config.toggle_layout,
            GlobalEvent::OpenHelp => config.open_help,
            GlobalEvent::NextTab => config.next_tab,
            GlobalEvent::PrevTab => config.prev_tab,
            GlobalEvent::OpenContextMenu => config.open_context_menu,
            GlobalEvent::CommandPopup => config.command_popup,
            GlobalEvent::InteractiveCommandPopup => config.interactive_command_popup,
            GlobalEvent::Quit => config.quit,
        );
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::Global,
            GlobalEvent::ScrollDown => "scroll-down", Some(Section::Navigation), "scroll down",
            GlobalEvent::ScrollUp => "scroll-up", Some(Section::Navigation), "scroll up",
            GlobalEvent::ScrollDownHalf => "scroll-down-half", Some(Section::Navigation), "scroll down by ½ page",
            GlobalEvent::ScrollUpHalf => "scroll-up-half", Some(Section::Navigation), "scroll up by ½ page",
            GlobalEvent::ScrollToTop => "scroll-to-top", Some(Section::Navigation), "go to the top of the list",
            GlobalEvent::ScrollToBottom => "scroll-to-bottom", Some(Section::Navigation), "go to the bottom of the list",
            GlobalEvent::FocusCurrent => "focus-current", Some(Section::Navigation), "go to current change",
            GlobalEvent::OpenContextMenu => "open-context-menu", Some(Section::Navigation), "open the context menu",

            GlobalEvent::Refresh => "refresh", Some(Section::App), "refresh",
            GlobalEvent::ToggleLayout => "toggle-layout", Some(Section::App), "toggle horizontal/vertical split",
            GlobalEvent::NextTab => "next-tab", Some(Section::App), "next tab",
            GlobalEvent::PrevTab => "prev-tab", Some(Section::App), "previous tab",
            GlobalEvent::LogTab => _, Some(Section::App), "log tab",
            GlobalEvent::FilesTab => _, Some(Section::App), "files tab",
            GlobalEvent::BookmarksTab => _, Some(Section::App), "bookmarks tab",
            GlobalEvent::EvologTab => _, Some(Section::App), "evolog tab",
            GlobalEvent::OpLogTab => _, Some(Section::App), "operation log tab",
            GlobalEvent::SettingsTab => _, Some(Section::App), "settings tab",
            GlobalEvent::CommandPopup => "command-popup", Some(Section::App), "run jj command",
            GlobalEvent::InteractiveCommandPopup => "interactive-command-popup", Some(Section::App), "run jj command interactively",
            GlobalEvent::OpenHelp => "open-help", Some(Section::App), "open help",
            GlobalEvent::Quit => "quit", Some(Section::App), "quit",
        )
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyCode;
    use ratatui::crossterm::event::KeyModifiers;

    use super::*;
    use crate::keybinds::Keybind;

    /// What toggling the layout says it does, it being the one action
    /// that comes unbound.
    const TOGGLE_LAYOUT: &str = "toggle horizontal/vertical split";

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
            keybinds.match_event(KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL)),
            GlobalEvent::ScrollToTop
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
            keybinds.match_event(key(KeyCode::Menu)),
            GlobalEvent::OpenContextMenu
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
        let help = binding(&keybinds, "open help");
        assert_eq!(help.keys_text(), "[disabled]");
        // What it comes bound to is what taking it back out restores.
        assert_eq!(help.defaults_text(), "?");
    }

    #[test]
    fn test_the_bindings_list_every_default_binding() {
        let keybinds = GlobalKeybinds::default();

        assert!(
            keybinds
                .bindings()
                .iter()
                .filter(|binding| binding.description != TOGGLE_LAYOUT)
                .all(|binding| binding.keys_text() != "[disabled]")
        );
        assert_eq!(binding(&keybinds, "refresh").keys_text(), "Shift+r/F5");
    }

    #[test]
    fn test_toggling_the_layout_comes_unbound() {
        let keybinds = GlobalKeybinds::default();

        assert_eq!(binding(&keybinds, TOGGLE_LAYOUT).keys_text(), "[disabled]");
    }

    /// The binding for the action `description` says it does.
    fn binding(keybinds: &GlobalKeybinds, description: &str) -> Binding {
        keybinds
            .bindings()
            .into_iter()
            .find(|binding| binding.description == description)
            .expect("the action is one the app has")
    }
}
