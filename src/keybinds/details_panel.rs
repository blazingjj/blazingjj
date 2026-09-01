use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::DetailsPanelKeybindsConfig;
use super::config::KeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

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
    /// The bindings as the configuration has them, which every tab's
    /// details panel answers to alike.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.details_panel.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    pub fn match_event(&self, event: KeyEvent) -> DetailsPanelEvent {
        self.keys
            .match_event(event)
            .unwrap_or(DetailsPanelEvent::Unbound)
    }

    fn extend_from_config(&mut self, config: &DetailsPanelKeybindsConfig) {
        update_keybinds!(
            self.keys,
            DetailsPanelEvent::ScrollDown => config.scroll_down,
            DetailsPanelEvent::ScrollUp => config.scroll_up,
            DetailsPanelEvent::ScrollDownHalfPage => config.scroll_down_half,
            DetailsPanelEvent::ScrollUpHalfPage => config.scroll_up_half,
            DetailsPanelEvent::ScrollDownPage => config.scroll_down_page,
            DetailsPanelEvent::ScrollUpPage => config.scroll_up_page,
            DetailsPanelEvent::ToggleWrap => config.toggle_wrap,
            DetailsPanelEvent::ToggleDiffFormat => config.toggle_diff_format,
        );
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::DetailsPanel,
            DetailsPanelEvent::ScrollDown => "scroll-down", Some(Section::DetailsPanel), "scroll down",
            DetailsPanelEvent::ScrollUp => "scroll-up", Some(Section::DetailsPanel), "scroll up",
            DetailsPanelEvent::ScrollDownHalfPage => "scroll-down-half", Some(Section::DetailsPanel), "scroll down by ½ page",
            DetailsPanelEvent::ScrollUpHalfPage => "scroll-up-half", Some(Section::DetailsPanel), "scroll up by ½ page",
            DetailsPanelEvent::ScrollDownPage => "scroll-down-page", Some(Section::DetailsPanel), "scroll down by page",
            DetailsPanelEvent::ScrollUpPage => "scroll-up-page", Some(Section::DetailsPanel), "scroll up by page",
            DetailsPanelEvent::ToggleDiffFormat => "toggle-diff-format", Some(Section::DetailsPanel), "toggle diff format",
            DetailsPanelEvent::ToggleWrap => "toggle-wrap", Some(Section::DetailsPanel), "toggle wrapping",
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

    fn bind(shortcut: &str) -> Keybind {
        Keybind::Single(Shortcut::from_str(shortcut).expect("shortcut should parse"))
    }

    #[test]
    fn test_details_panel_keybinds_default() {
        let _ = DetailsPanelKeybinds::default();
    }

    #[test]
    fn test_extend_from_config_replaces_bindings() {
        let config = DetailsPanelKeybindsConfig {
            toggle_diff_format: Some(bind("ctrl+w")),
            toggle_wrap: Some(Keybind::Enable(false)),
            scroll_down: None,
            scroll_up: None,
            scroll_down_half: None,
            scroll_up_half: None,
            scroll_down_page: None,
            scroll_up_page: None,
        };

        let mut keybinds = DetailsPanelKeybinds::default();
        keybinds.extend_from_config(&config);

        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            DetailsPanelEvent::ToggleDiffFormat
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('w'))),
            DetailsPanelEvent::Unbound
        );
        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('W'), KeyModifiers::SHIFT)),
            DetailsPanelEvent::Unbound
        );

        // Anything the config leaves out keeps its default.
        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            DetailsPanelEvent::ScrollDown
        );
    }
}
