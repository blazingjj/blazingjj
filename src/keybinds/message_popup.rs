use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::config::KeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MessagePopupEvent {
    ScrollDown,
    ScrollUp,
    ScrollDownHalf,
    ScrollUpHalf,
    ScrollDownPage,
    ScrollUpPage,
    Unbound,
}

#[derive(Debug)]
pub struct MessagePopupKeybinds {
    keys: KeybindsStore<MessagePopupEvent>,
}

impl Default for MessagePopupKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<MessagePopupEvent>::default();
        set_keybinds!(
            keys,
            MessagePopupEvent::ScrollDown => "j",
            MessagePopupEvent::ScrollDown => "down",
            MessagePopupEvent::ScrollUp => "k",
            MessagePopupEvent::ScrollUp => "up",
            MessagePopupEvent::ScrollDownHalf => "ctrl+d",
            MessagePopupEvent::ScrollUpHalf => "ctrl+u",
            MessagePopupEvent::ScrollDownPage => "ctrl+f",
            MessagePopupEvent::ScrollDownPage => "space",
            MessagePopupEvent::ScrollDownPage => "pagedown",
            MessagePopupEvent::ScrollUpPage => "ctrl+b",
            MessagePopupEvent::ScrollUpPage => "pageup",
        );
        Self { keys }
    }
}

impl MessagePopupKeybinds {
    pub fn from_config(config: &KeybindsConfig) -> Self {
        let mut keybinds = Self::default();
        update_keybinds!(
            keybinds.keys,
            MessagePopupEvent::ScrollDown => config.scroll_down,
            MessagePopupEvent::ScrollUp => config.scroll_up,
            MessagePopupEvent::ScrollDownHalf => config.scroll_down_half,
            MessagePopupEvent::ScrollUpHalf => config.scroll_up_half,
        );
        if let Some(ref popup_config) = config.message_popup {
            update_keybinds!(
                keybinds.keys,
                MessagePopupEvent::ScrollDown => popup_config.scroll_down,
                MessagePopupEvent::ScrollUp => popup_config.scroll_up,
                MessagePopupEvent::ScrollDownHalf => popup_config.scroll_down_half,
                MessagePopupEvent::ScrollUpHalf => popup_config.scroll_up_half,
                MessagePopupEvent::ScrollDownPage => popup_config.scroll_down_page,
                MessagePopupEvent::ScrollUpPage => popup_config.scroll_up_page,
            );
        }
        keybinds
    }

    pub fn match_event(&self, event: KeyEvent) -> MessagePopupEvent {
        self.keys
            .match_event(event)
            .unwrap_or(MessagePopupEvent::Unbound)
    }
}

#[test]
fn test_message_popup_keybinds_default() {
    let _ = MessagePopupKeybinds::default();
}
