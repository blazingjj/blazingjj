use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::KeybindsConfig;
use super::config::OpLogTabKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct OpLogTabKeybinds {
    keys: KeybindsStore<OpLogTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OpLogTabEvent {
    CopyId,
    LoadMore,

    Unbound,
}

impl Default for OpLogTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<OpLogTabEvent>::default();
        set_keybinds!(
            keys,
            OpLogTabEvent::CopyId => "shift+y",
            OpLogTabEvent::LoadMore => "m",
        );
        Self { keys }
    }
}

impl OpLogTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.op_log_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &OpLogTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            OpLogTabEvent::CopyId => config.copy_id,
            OpLogTabEvent::LoadMore => config.load_more,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> OpLogTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(OpLogTabEvent::Unbound)
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::OpLogTab,
            OpLogTabEvent::LoadMore => "load-more", Some(Section::Navigation), "read further back in the operation log",
            OpLogTabEvent::CopyId => "copy-id", Some(Section::Clipboard), "yank operation id to clipboard",
        )
    }
}

#[test]
fn test_op_log_tab_keybinds_default() {
    let _ = OpLogTabKeybinds::default();
}
