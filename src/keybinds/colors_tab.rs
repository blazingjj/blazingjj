use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::ColorsTabKeybindsConfig;
use super::config::KeybindsConfig;
use super::hint_line;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct ColorsTabKeybinds {
    keys: KeybindsStore<ColorsTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ColorsTabEvent {
    ChangeForeground,
    ChangeBackground,
    Unset,
    Back,

    Unbound,
}

impl Default for ColorsTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<ColorsTabEvent>::default();
        set_keybinds!(
            keys,
            ColorsTabEvent::ChangeForeground => "enter",
            ColorsTabEvent::ChangeBackground => "b",
            ColorsTabEvent::Unset => "x",
            ColorsTabEvent::Back => "esc",
        );
        Self { keys }
    }
}

impl ColorsTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.colors_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &ColorsTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            ColorsTabEvent::ChangeForeground => config.change_foreground,
            ColorsTabEvent::ChangeBackground => config.change_background,
            ColorsTabEvent::Unset => config.unset,
            ColorsTabEvent::Back => config.back,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> ColorsTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(ColorsTabEvent::Unbound)
    }

    /// The line under the list saying what it answers to, in as much of
    /// `width` as it takes. Where there is no room for all of it, what
    /// is least worth saying goes first, down to changing a colour and
    /// leaving the list again.
    pub fn hint(&self, width: usize) -> String {
        hint_line(
            [
                (ColorsTabEvent::ChangeForeground, "foreground"),
                (ColorsTabEvent::ChangeBackground, "background"),
                (ColorsTabEvent::Unset, "take out"),
                (ColorsTabEvent::Back, "back"),
            ]
            .into_iter()
            .filter_map(|(event, what)| Some((self.shortcut(event)?, what))),
            width,
        )
    }

    /// The shortcut to name `event` by, of those bound to it.
    fn shortcut(&self, event: ColorsTabEvent) -> Option<Shortcut> {
        self.keys.get_shortcuts(event).into_iter().next()
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::ColorsTab,
            ColorsTabEvent::ChangeForeground => "change-foreground", Some(Section::Settings), "change what the element is drawn in",
            ColorsTabEvent::ChangeBackground => "change-background", Some(Section::Settings), "change what the element is drawn on",
            ColorsTabEvent::Unset => "unset", Some(Section::Settings), "take the element's colors out of your config",
            ColorsTabEvent::Back => "back", Some(Section::Settings), "go back to the settings",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colors_tab_keybinds_default() {
        let _ = ColorsTabKeybinds::default();
    }

    /// The hint says as much as there is room for, and what it drops is
    /// what is least worth saying rather than what comes last.
    #[test]
    fn test_the_hint_drops_what_there_is_no_room_for() {
        let keybinds = ColorsTabKeybinds::default();

        assert_eq!(
            keybinds.hint(80),
            "Enter: foreground | b: background | x: take out | Esc: back"
        );
        assert_eq!(keybinds.hint(1), "Enter: foreground | Esc: back");
    }
}
