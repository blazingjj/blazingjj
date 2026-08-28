use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::keybinds_store::KeybindsStore;
use crate::make_keybinds_help;
use crate::set_keybinds;

#[derive(Debug)]
pub struct FilesTabKeybinds {
    keys: KeybindsStore<FilesTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FilesTabEvent {
    Untrack,
    Restore,

    OpenContextMenu,

    Unbound,
}

impl Default for FilesTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<FilesTabEvent>::default();
        set_keybinds!(
            keys,
            FilesTabEvent::Untrack => "x",
            FilesTabEvent::Restore => "r",
            FilesTabEvent::OpenContextMenu => "menu",
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

    pub fn make_help(&self) -> Vec<(String, String)> {
        make_keybinds_help!(
            self.keys,
            FilesTabEvent::Untrack => "untrack file",
            FilesTabEvent::Restore => "restore file",
            FilesTabEvent::OpenContextMenu => "open the context menu",
        )
    }
}

#[test]
fn test_files_tab_keybinds_default() {
    let _ = FilesTabKeybinds::default();
}
