use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::config::FilesTabKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_keybinds_help;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct FilesTabKeybinds {
    keys: KeybindsStore<FilesTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FilesTabEvent {
    Untrack,
    Restore,

    Unbound,
}

impl Default for FilesTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<FilesTabEvent>::default();
        set_keybinds!(
            keys,
            FilesTabEvent::Untrack => "x",
            FilesTabEvent::Restore => "r",
        );
        Self { keys }
    }
}

impl FilesTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = keybinds_config().and_then(|config| config.files_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &FilesTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            FilesTabEvent::Untrack => config.untrack,
            FilesTabEvent::Restore => config.restore,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> FilesTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(FilesTabEvent::Unbound)
    }

    pub fn make_help(&self) -> Vec<(String, String)> {
        make_keybinds_help!(
            self.keys,
            FilesTabEvent::Untrack => "untrack file",
            FilesTabEvent::Restore => "restore file",
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
    fn test_files_tab_keybinds_default() {
        let _ = FilesTabKeybinds::default();
    }

    #[test]
    fn test_extend_from_config_replaces_bindings() {
        let config = FilesTabKeybindsConfig {
            untrack: Some(Keybind::Single(
                Shortcut::from_str("u").expect("shortcut should parse"),
            )),
            restore: Some(Keybind::Enable(false)),
        };

        let mut keybinds = FilesTabKeybinds::default();
        keybinds.extend_from_config(&config);

        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('u'))),
            FilesTabEvent::Untrack
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('x'))),
            FilesTabEvent::Unbound
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('r'))),
            FilesTabEvent::Unbound
        );
    }
}
