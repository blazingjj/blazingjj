use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::KeybindsConfig;
use super::config::MenusTabKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct MenusTabKeybinds {
    keys: KeybindsStore<MenusTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MenusTabEvent {
    Toggle,
    MoveUp,
    MoveDown,
    Unset,
    Back,

    Unbound,
}

impl Default for MenusTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<MenusTabEvent>::default();
        set_keybinds!(
            keys,
            MenusTabEvent::Toggle => "enter",
            MenusTabEvent::MoveUp => "shift+k",
            MenusTabEvent::MoveDown => "shift+j",
            MenusTabEvent::Unset => "x",
            MenusTabEvent::Back => "esc",
        );
        Self { keys }
    }
}

impl MenusTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.menus_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &MenusTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            MenusTabEvent::Toggle => config.toggle,
            MenusTabEvent::MoveUp => config.move_up,
            MenusTabEvent::MoveDown => config.move_down,
            MenusTabEvent::Unset => config.unset,
            MenusTabEvent::Back => config.back,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> MenusTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(MenusTabEvent::Unbound)
    }

    /// The line under the list saying what it answers to, in as much of
    /// `width` as it takes. Where there is no room for all of it, what
    /// is least worth saying goes first, down to putting an item in a
    /// menu and leaving the list again.
    pub fn hint(&self, width: usize) -> String {
        let mut parts: Vec<String> = [
            (MenusTabEvent::Toggle, "in or out"),
            (MenusTabEvent::MoveUp, "up"),
            (MenusTabEvent::MoveDown, "down"),
            (MenusTabEvent::Unset, "take out"),
            (MenusTabEvent::Back, "back"),
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
    fn shortcut(&self, event: MenusTabEvent) -> Option<Shortcut> {
        self.keys.get_shortcuts(event).into_iter().next()
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::MenusTab,
            MenusTabEvent::Toggle => "toggle", Some(Section::Settings), "put the item in the menu, or take it out",
            MenusTabEvent::MoveUp => "move-up", Some(Section::Settings), "move the item up the menu",
            MenusTabEvent::MoveDown => "move-down", Some(Section::Settings), "move the item down the menu",
            MenusTabEvent::Unset => "unset", Some(Section::Settings), "take the menu out of your config",
            MenusTabEvent::Back => "back", Some(Section::Settings), "go back to the settings",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menus_tab_keybinds_default() {
        let _ = MenusTabKeybinds::default();
    }

    /// The hint says as much as there is room for, and what it drops is
    /// what is least worth saying rather than what comes last.
    #[test]
    fn test_the_hint_drops_what_there_is_no_room_for() {
        let keybinds = MenusTabKeybinds::default();

        assert_eq!(
            keybinds.hint(80),
            "Enter: in or out | Shift+k: up | Shift+j: down | x: take out | Esc: back"
        );
        assert_eq!(
            keybinds.hint(60),
            "Enter: in or out | Shift+k: up | Shift+j: down | Esc: back"
        );
        assert_eq!(keybinds.hint(1), "Enter: in or out | Esc: back");
    }
}
