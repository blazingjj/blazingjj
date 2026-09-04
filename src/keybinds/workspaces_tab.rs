use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::KeybindsConfig;
use super::config::WorkspacesTabKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct WorkspacesTabKeybinds {
    keys: KeybindsStore<WorkspacesTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum WorkspacesTabEvent {
    Add,
    Forget,
    Rename,
    Switch,

    Unbound,
}

impl Default for WorkspacesTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<WorkspacesTabEvent>::default();
        set_keybinds!(
            keys,
            WorkspacesTabEvent::Add => "a",
            WorkspacesTabEvent::Forget => "x",
            WorkspacesTabEvent::Rename => "r",
            WorkspacesTabEvent::Switch => "enter",
        );
        Self { keys }
    }
}

impl WorkspacesTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.workspaces_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &WorkspacesTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            WorkspacesTabEvent::Add => config.add,
            WorkspacesTabEvent::Forget => config.forget,
            WorkspacesTabEvent::Rename => config.rename,
            WorkspacesTabEvent::Switch => config.switch,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> WorkspacesTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(WorkspacesTabEvent::Unbound)
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::WorkspacesTab,
            WorkspacesTabEvent::Switch => "switch", Some(Section::Workspaces), "work in this workspace, restarting the app there",
            WorkspacesTabEvent::Add => "add", Some(Section::Workspaces), "add a workspace",
            WorkspacesTabEvent::Rename => "rename", Some(Section::Workspaces), "rename this workspace",
            WorkspacesTabEvent::Forget => "forget", Some(Section::Workspaces), "forget this workspace",
        )
    }
}

#[test]
fn test_workspaces_tab_keybinds_default() {
    let _ = WorkspacesTabKeybinds::default();
}
