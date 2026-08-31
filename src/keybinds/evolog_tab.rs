use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::EvologTabKeybindsConfig;
use super::config::KeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct EvologTabKeybinds {
    keys: KeybindsStore<EvologTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum EvologTabEvent {
    OpenFiles,
    Duplicate,
    CopyRev,

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
        );
        Self { keys }
    }
}

impl EvologTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.evolog_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &EvologTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            EvologTabEvent::OpenFiles => config.open_files,
            EvologTabEvent::Duplicate => config.duplicate,
            EvologTabEvent::CopyRev => config.copy_rev,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> EvologTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(EvologTabEvent::Unbound)
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::EvologTab,
            EvologTabEvent::OpenFiles => "open-files", Some(Section::Navigation), "see files of this version",
            EvologTabEvent::Duplicate => "duplicate", Some(Section::Changes), "duplicate this version as a new change",
            EvologTabEvent::CopyRev => "copy-rev", Some(Section::Clipboard), "yank revision to clipboard",
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

    #[test]
    fn test_evolog_tab_keybinds_default() {
        let _ = EvologTabKeybinds::default();
    }

    #[test]
    fn test_extend_from_config_replaces_bindings() {
        let config = EvologTabKeybindsConfig {
            duplicate: Some(Keybind::Single(
                Shortcut::from_str("ctrl+d").expect("shortcut should parse"),
            )),
            open_files: None,
            copy_rev: None,
        };

        let mut keybinds = EvologTabKeybinds::default();
        keybinds.extend_from_config(&config);

        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            EvologTabEvent::Duplicate
        );
        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT)),
            EvologTabEvent::Unbound
        );

        // Anything the config leaves out keeps its default.
        assert_eq!(
            keybinds.match_event(key(KeyCode::Enter)),
            EvologTabEvent::OpenFiles
        );
    }
}
