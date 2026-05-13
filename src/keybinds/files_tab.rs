use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::config::KeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::make_keybinds_help;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct FilesTabKeybinds {
    keys: KeybindsStore<FilesTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FilesTabEvent {
    ScrollDown,
    ScrollUp,
    ScrollDownHalf,
    ScrollUpHalf,

    ToggleDiffFormat,
    Untrack,
    Restore,
    Refresh,
    FocusCurrent,

    OpenHelp,

    Unbound,
}

impl Default for FilesTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<FilesTabEvent>::default();
        set_keybinds!(
            keys,
            FilesTabEvent::ScrollDown => "j",
            FilesTabEvent::ScrollDown => "down",
            FilesTabEvent::ScrollUp => "k",
            FilesTabEvent::ScrollUp => "up",
            FilesTabEvent::ScrollDownHalf => "shift+j",
            FilesTabEvent::ScrollUpHalf => "shift+k",
            FilesTabEvent::ToggleDiffFormat => "w",
            FilesTabEvent::Untrack => "x",
            FilesTabEvent::Restore => "r",
            FilesTabEvent::Refresh => "shift+r",
            FilesTabEvent::Refresh => "f5",
            FilesTabEvent::FocusCurrent => "@",
            FilesTabEvent::OpenHelp => "?",
        );
        Self { keys }
    }
}

impl FilesTabKeybinds {
    pub fn match_event(&self, event: KeyEvent) -> FilesTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(FilesTabEvent::Unbound)
    }

    pub fn extend_from_config(&mut self, config: &KeybindsConfig) {
        update_keybinds!(
            self.keys,
            FilesTabEvent::ScrollDown => config.scroll_down,
            FilesTabEvent::ScrollUp => config.scroll_up,
            FilesTabEvent::ScrollDownHalf => config.scroll_down_half,
            FilesTabEvent::ScrollUpHalf => config.scroll_up_half,
        );
    }

    pub fn make_help(&self) -> Vec<(String, String)> {
        make_keybinds_help!(
            self.keys,
            FilesTabEvent::ScrollDown => "scroll down",
            FilesTabEvent::ScrollUp => "scroll up",
            FilesTabEvent::ScrollDownHalf => "scroll down by ½ page",
            FilesTabEvent::ScrollUpHalf => "scroll up by ½ page",
            FilesTabEvent::Untrack => "untrack file",
            FilesTabEvent::Restore => "restore file",
            FilesTabEvent::FocusCurrent => "view current change files",
        )
    }
}

#[test]
fn test_files_tab_keybinds_default() {
    let _ = FilesTabKeybinds::default();
}
