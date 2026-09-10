use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::KeybindsConfig;
use super::config::StylesTabKeybindsConfig;
use super::hint_line;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct StylesTabKeybinds {
    keys: KeybindsStore<StylesTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum StylesTabEvent {
    ChangeForeground,
    ChangeBackground,
    ToggleBold,
    ToggleDim,
    ToggleItalic,
    ToggleUnderline,
    Unset,
    Back,

    Unbound,
}

impl Default for StylesTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<StylesTabEvent>::default();
        set_keybinds!(
            keys,
            StylesTabEvent::ChangeForeground => "enter",
            StylesTabEvent::ChangeBackground => "b",
            // The attributes go by their own initial, shifted: the plain
            // letters are the background's and the list's to move by.
            StylesTabEvent::ToggleBold => "shift+b",
            StylesTabEvent::ToggleDim => "shift+d",
            StylesTabEvent::ToggleItalic => "shift+i",
            StylesTabEvent::ToggleUnderline => "shift+u",
            StylesTabEvent::Unset => "x",
            StylesTabEvent::Back => "esc",
        );
        Self { keys }
    }
}

impl StylesTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.styles_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &StylesTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            StylesTabEvent::ChangeForeground => config.change_foreground,
            StylesTabEvent::ChangeBackground => config.change_background,
            StylesTabEvent::ToggleBold => config.toggle_bold,
            StylesTabEvent::ToggleDim => config.toggle_dim,
            StylesTabEvent::ToggleItalic => config.toggle_italic,
            StylesTabEvent::ToggleUnderline => config.toggle_underline,
            StylesTabEvent::Unset => config.unset,
            StylesTabEvent::Back => config.back,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> StylesTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(StylesTabEvent::Unbound)
    }

    /// The line under the list saying what it answers to, in as much of
    /// `width` as it takes. Where there is no room for all of it, what
    /// is least worth saying goes first, down to changing a colour and
    /// leaving the list again.
    pub fn hint(&self, width: usize) -> String {
        hint_line(
            [
                (StylesTabEvent::ChangeForeground, "foreground"),
                (StylesTabEvent::ChangeBackground, "background"),
                (StylesTabEvent::Unset, "take out"),
                // The attributes go last of what there is room for: a
                // colour is what an element is usually given.
                (StylesTabEvent::ToggleBold, "bold"),
                (StylesTabEvent::ToggleDim, "dim"),
                (StylesTabEvent::ToggleItalic, "italic"),
                (StylesTabEvent::ToggleUnderline, "underline"),
                (StylesTabEvent::Back, "back"),
            ]
            .into_iter()
            .filter_map(|(event, what)| Some((self.shortcut(event)?, what))),
            width,
        )
    }

    /// The shortcut to name `event` by, of those bound to it.
    fn shortcut(&self, event: StylesTabEvent) -> Option<Shortcut> {
        self.keys.get_shortcuts(event).into_iter().next()
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::StylesTab,
            StylesTabEvent::ChangeForeground => "change-foreground", Some(Section::Settings), "change what the element is drawn in",
            StylesTabEvent::ChangeBackground => "change-background", Some(Section::Settings), "change what the element is drawn on",
            StylesTabEvent::ToggleBold => "toggle-bold", Some(Section::Settings), "draw the element bold or not, against what it inherits",
            StylesTabEvent::ToggleDim => "toggle-dim", Some(Section::Settings), "draw the element dim or not, against what it inherits",
            StylesTabEvent::ToggleItalic => "toggle-italic", Some(Section::Settings), "draw the element italic or not, against what it inherits",
            StylesTabEvent::ToggleUnderline => "toggle-underline", Some(Section::Settings), "draw the element underlined or not, against what it inherits",
            StylesTabEvent::Unset => "unset", Some(Section::Settings), "take the element's style out of your config",
            StylesTabEvent::Back => "back", Some(Section::Settings), "go back to the settings",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_styles_tab_keybinds_default() {
        let _ = StylesTabKeybinds::default();
    }

    /// The hint says as much as there is room for, and what it drops is
    /// what is least worth saying rather than what comes last.
    #[test]
    fn test_the_hint_drops_what_there_is_no_room_for() {
        let keybinds = StylesTabKeybinds::default();

        assert_eq!(
            keybinds.hint(80),
            "Enter: foreground | b: background | x: take out | Shift+b: bold | Esc: back"
        );
        assert_eq!(keybinds.hint(1), "Enter: foreground | Esc: back");
    }
}
