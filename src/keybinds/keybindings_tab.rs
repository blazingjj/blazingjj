use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::KeybindingsTabKeybindsConfig;
use super::config::KeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct KeybindingsTabKeybinds {
    keys: KeybindsStore<KeybindingsTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum KeybindingsTabEvent {
    Bind,
    BindBesides,
    Disable,
    Unset,
    Back,

    Unbound,
}

impl Default for KeybindingsTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<KeybindingsTabEvent>::default();
        set_keybinds!(
            keys,
            KeybindingsTabEvent::Bind => "enter",
            KeybindingsTabEvent::BindBesides => "a",
            KeybindingsTabEvent::Disable => "shift+x",
            KeybindingsTabEvent::Unset => "x",
            KeybindingsTabEvent::Back => "esc",
        );
        Self { keys }
    }
}

impl KeybindingsTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.keybindings_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &KeybindingsTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            KeybindingsTabEvent::Bind => config.bind,
            KeybindingsTabEvent::BindBesides => config.bind_besides,
            KeybindingsTabEvent::Disable => config.disable,
            KeybindingsTabEvent::Unset => config.unset,
            KeybindingsTabEvent::Back => config.back,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> KeybindingsTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(KeybindingsTabEvent::Unbound)
    }

    /// The line under the list saying what it answers to, in as much of
    /// `width` as it takes. Where there is no room for all of it, what
    /// is least worth saying goes first, down to binding a key and
    /// leaving the list again.
    pub fn hint(&self, width: usize) -> String {
        let mut parts: Vec<String> = [
            (KeybindingsTabEvent::Bind, "bind"),
            (KeybindingsTabEvent::BindBesides, "add a key"),
            (KeybindingsTabEvent::Disable, "bind nothing"),
            (KeybindingsTabEvent::Unset, "take out"),
            (KeybindingsTabEvent::Back, "back"),
        ]
        .into_iter()
        .filter_map(|(event, what)| Some(format!("{}: {what}", self.shortcut(event)?)))
        .collect();

        while parts.len() > 2 && Self::hint_width(&parts) > width {
            parts.remove(parts.len() - 2);
        }

        parts.join(" | ")
    }

    /// How wide the hint `parts` are with what joins them.
    fn hint_width(parts: &[String]) -> usize {
        let joins = 3 * parts.len().saturating_sub(1);

        joins + parts.iter().map(|part| part.chars().count()).sum::<usize>()
    }

    /// The shortcut to name `event` by, of those bound to it.
    fn shortcut(&self, event: KeybindingsTabEvent) -> Option<Shortcut> {
        self.keys.get_shortcuts(event).into_iter().next()
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::KeybindingsTab,
            KeybindingsTabEvent::Bind => "bind", Some(Section::Settings), "bind a key to the action",
            KeybindingsTabEvent::BindBesides => "bind-besides", Some(Section::Settings), "bind another key to the action",
            KeybindingsTabEvent::Disable => "disable", Some(Section::Settings), "leave the action bound to nothing",
            KeybindingsTabEvent::Unset => "unset", Some(Section::Settings), "take the binding out of your config",
            KeybindingsTabEvent::Back => "back", Some(Section::Settings), "go back to the settings",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keybindings_tab_keybinds_default() {
        let _ = KeybindingsTabKeybinds::default();
    }

    /// The hint says as much as there is room for, and what it drops is
    /// what is least worth saying rather than what comes last.
    #[test]
    fn test_the_hint_drops_what_there_is_no_room_for() {
        let keybinds = KeybindingsTabKeybinds::default();

        assert_eq!(
            keybinds.hint(80),
            "Enter: bind | a: add a key | Shift+x: bind nothing | x: take out | Esc: back"
        );
        assert_eq!(keybinds.hint(40), "Enter: bind | a: add a key | Esc: back");
        assert_eq!(keybinds.hint(1), "Enter: bind | Esc: back");
    }
}
