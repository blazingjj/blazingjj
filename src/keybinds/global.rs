use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::keybinds_store::KeybindsStore;
use crate::make_keybinds_help;
use crate::set_keybinds;

#[derive(Debug)]
pub struct GlobalKeybinds {
    keys: KeybindsStore<GlobalEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GlobalEvent {
    Quit,
    NextTab,
    PrevTab,
    Tab1,
    Tab2,
    Tab3,
    CommandPopup,
    OpenHelp,
    Unbound,
}

impl Default for GlobalKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<GlobalEvent>::default();
        set_keybinds!(
            keys,
            GlobalEvent::Quit => "q",
            GlobalEvent::Quit => "ctrl+c",
            GlobalEvent::Quit => "esc",
            GlobalEvent::NextTab => "l",
            GlobalEvent::PrevTab => "h",
            GlobalEvent::Tab1 => "1",
            GlobalEvent::Tab2 => "2",
            GlobalEvent::Tab3 => "3",
            GlobalEvent::CommandPopup => ":",
            GlobalEvent::OpenHelp => "?",
        );
        Self { keys }
    }
}

impl GlobalKeybinds {
    pub fn match_event(&self, event: KeyEvent) -> GlobalEvent {
        self.keys.match_event(event).unwrap_or(GlobalEvent::Unbound)
    }

    pub fn make_help(&self) -> Vec<(String, String)> {
        make_keybinds_help!(
            self.keys,
            GlobalEvent::Quit => "quit",
            GlobalEvent::NextTab => "next tab",
            GlobalEvent::PrevTab => "previous tab",
            GlobalEvent::Tab1 => "log tab",
            GlobalEvent::Tab2 => "files tab",
            GlobalEvent::Tab3 => "bookmarks tab",
            GlobalEvent::CommandPopup => "run jj command",
            GlobalEvent::OpenHelp => "open help",
        )
    }
}

#[test]
fn test_global_keybinds_default() {
    let _ = GlobalKeybinds::default();
}
