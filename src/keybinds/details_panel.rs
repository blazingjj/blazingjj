use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Keybind;
use super::Shortcut;
use super::keybinds_store::KeybindsStore;
use crate::make_keybinds_help;
use crate::set_keybinds;

#[derive(Debug)]
pub struct DetailsPanelKeybinds {
    keys: KeybindsStore<DetailsPanelEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DetailsPanelEvent {
    ScrollDown,
    ScrollUp,
    ScrollDownHalfPage,
    ScrollUpHalfPage,
    ScrollDownPage,
    ScrollUpPage,
    ToggleWrap,
    ToggleDiffFormat,
    Unbound,
}

impl Default for DetailsPanelKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<DetailsPanelEvent>::default();
        set_keybinds!(
            keys,
            DetailsPanelEvent::ScrollDown => "ctrl+e",
            DetailsPanelEvent::ScrollUp => "ctrl+y",
            DetailsPanelEvent::ScrollDownHalfPage => "ctrl+d",
            DetailsPanelEvent::ScrollUpHalfPage => "ctrl+u",
            DetailsPanelEvent::ScrollDownPage => "ctrl+f",
            DetailsPanelEvent::ScrollUpPage => "ctrl+b",
            DetailsPanelEvent::ToggleWrap => "shift+w",
            DetailsPanelEvent::ToggleDiffFormat => "w",
        );
        Self { keys }
    }
}

impl DetailsPanelKeybinds {
    pub fn match_event(&self, event: KeyEvent) -> DetailsPanelEvent {
        self.keys
            .match_event(event)
            .unwrap_or(DetailsPanelEvent::Unbound)
    }

    pub fn extend_from_config(&mut self, toggle_diff_format: Option<&Keybind>) {
        if let Some(k) = toggle_diff_format {
            self.keys
                .replace_action_from_config(DetailsPanelEvent::ToggleDiffFormat, k);
        }
    }

    pub fn make_help(&self) -> Vec<(String, String)> {
        make_keybinds_help!(
            self.keys,
            DetailsPanelEvent::ScrollDown => "scroll down",
            DetailsPanelEvent::ScrollUp => "scroll up",
            DetailsPanelEvent::ScrollDownHalfPage => "scroll down by ½ page",
            DetailsPanelEvent::ScrollUpHalfPage => "scroll up by ½ page",
            DetailsPanelEvent::ScrollDownPage => "scroll down by page",
            DetailsPanelEvent::ScrollUpPage => "scroll up by page",
            DetailsPanelEvent::ToggleDiffFormat => "toggle diff format",
            DetailsPanelEvent::ToggleWrap => "toggle wrapping",
        )
    }
}

#[test]
fn test_details_panel_keybinds_default() {
    let _ = DetailsPanelKeybinds::default();
}
