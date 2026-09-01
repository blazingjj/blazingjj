use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::KeybindsConfig;
use super::config::SettingsTabKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct SettingsTabKeybinds {
    keys: KeybindsStore<SettingsTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SettingsTabEvent {
    Change,
    Unset,

    Unbound,
}

impl Default for SettingsTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<SettingsTabEvent>::default();
        set_keybinds!(
            keys,
            SettingsTabEvent::Change => "enter",
            SettingsTabEvent::Unset => "x",
        );
        Self { keys }
    }
}

impl SettingsTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.settings_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &SettingsTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            SettingsTabEvent::Change => config.change,
            SettingsTabEvent::Unset => config.unset,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> SettingsTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(SettingsTabEvent::Unbound)
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::SettingsTab,
            SettingsTabEvent::Change => "change", Some(Section::Settings), "change the setting",
            SettingsTabEvent::Unset => "unset", Some(Section::Settings), "take the setting out of your config",
        )
    }
}

#[test]
fn test_settings_tab_keybinds_default() {
    let _ = SettingsTabKeybinds::default();
}
