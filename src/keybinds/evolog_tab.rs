use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::keybinds_store::KeybindsStore;
use crate::make_keybinds_help;
use crate::set_keybinds;

#[derive(Debug)]
pub struct EvologTabKeybinds {
    keys: KeybindsStore<EvologTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum EvologTabEvent {
    OpenFiles,
    Duplicate,
    CopyRev,

    OpenContextMenu,

    Unbound,
}

impl Default for EvologTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<EvologTabEvent>::default();
        set_keybinds!(
            keys,
            EvologTabEvent::OpenFiles => "enter",
            EvologTabEvent::Duplicate => "shift+d",
            EvologTabEvent::CopyRev => "shift+y",
            EvologTabEvent::OpenContextMenu => "menu",
        );
        Self { keys }
    }
}

impl EvologTabKeybinds {
    pub fn match_event(&self, event: KeyEvent) -> EvologTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(EvologTabEvent::Unbound)
    }

    pub fn make_help(&self) -> Vec<(String, String)> {
        make_keybinds_help!(
            self.keys,
            EvologTabEvent::OpenFiles => "see files of this version",
            EvologTabEvent::Duplicate => "duplicate this version as a new change",
            EvologTabEvent::CopyRev => "yank revision to clipboard",
            EvologTabEvent::OpenContextMenu => "open the context menu",
        )
    }
}

#[test]
fn test_evolog_tab_keybinds_default() {
    let _ = EvologTabKeybinds::default();
}
