/*! Key bindings specific for rebase popup */

use std::str::FromStr; // used by set_keybinds macro

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Shortcut;
use super::config::KeybindsConfig;
use super::config::RebasePopupKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

/// How should rebase cut revisions from source
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CutOption {
    IncludeDescendants, // -s
    IncludeBranch,      // -b
    SingleRevision,     // -r
}

/// How should rebase paste revisions at target
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PasteOption {
    NewBranch,    // -d
    InsertAfter,  // -A
    InsertBefore, // -B
}

/// Actions available inside a RebasePopup
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PopupAction {
    None,
    SetSourceMode(CutOption),
    SetTargetMode(PasteOption),
}

fn default_keybinds() -> KeybindsStore<PopupAction> {
    let mut keys = KeybindsStore::<PopupAction>::default();
    set_keybinds!(
        keys,
        PopupAction::SetSourceMode(CutOption::IncludeDescendants) => "s",
        PopupAction::SetSourceMode(CutOption::IncludeBranch) => "b",
        PopupAction::SetSourceMode(CutOption::SingleRevision) => "r",
        PopupAction::SetTargetMode(PasteOption::NewBranch) => "d",
        PopupAction::SetTargetMode(PasteOption::InsertAfter) => "shift+a",
        PopupAction::SetTargetMode(PasteOption::InsertBefore) => "shift+b",
    );
    keys
}

#[derive(Debug)]
pub struct Keybinds {
    keys: KeybindsStore<PopupAction>,
}

impl Default for Keybinds {
    fn default() -> Self {
        Self {
            keys: default_keybinds(),
        }
    }
}

impl Keybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.rebase_popup.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    pub fn match_event(&self, event: KeyEvent) -> PopupAction {
        if let Some(action) = self.keys.match_event(event) {
            action
        } else {
            PopupAction::None
        }
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, default_keybinds(), Context::RebasePopup,
            PopupAction::SetSourceMode(CutOption::IncludeDescendants) => "source-with-descendants", None, "take the change and its descendants",
            PopupAction::SetSourceMode(CutOption::IncludeBranch) => "source-whole-branch", None, "take the whole branch",
            PopupAction::SetSourceMode(CutOption::SingleRevision) => "source-single-revision", None, "take the change on its own",
            PopupAction::SetTargetMode(PasteOption::NewBranch) => "target-new-branch", None, "put the source onto the destination",
            PopupAction::SetTargetMode(PasteOption::InsertAfter) => "target-insert-after", None, "put the source after the destination",
            PopupAction::SetTargetMode(PasteOption::InsertBefore) => "target-insert-before", None, "put the source before the destination",
        )
    }

    fn extend_from_config(&mut self, config: &RebasePopupKeybindsConfig) {
        update_keybinds!(
            self.keys,
            PopupAction::SetSourceMode(CutOption::IncludeDescendants) => config.source_with_descendants,
            PopupAction::SetSourceMode(CutOption::IncludeBranch) => config.source_whole_branch,
            PopupAction::SetSourceMode(CutOption::SingleRevision) => config.source_single_revision,
            PopupAction::SetTargetMode(PasteOption::NewBranch) => config.target_new_branch,
            PopupAction::SetTargetMode(PasteOption::InsertAfter) => config.target_insert_after,
            PopupAction::SetTargetMode(PasteOption::InsertBefore) => config.target_insert_before,
        );
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
    fn test_extend_from_config_replaces_bindings() {
        let config = RebasePopupKeybindsConfig {
            source_single_revision: Some(Keybind::Single(
                Shortcut::from_str("1").expect("shortcut should parse"),
            )),
            source_with_descendants: None,
            source_whole_branch: None,
            target_new_branch: None,
            target_insert_after: None,
            target_insert_before: None,
        };

        let mut keybinds = Keybinds::default();
        keybinds.extend_from_config(&config);

        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('1'))),
            PopupAction::SetSourceMode(CutOption::SingleRevision)
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('r'))),
            PopupAction::None
        );

        // Anything the config leaves out keeps its default.
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('b'))),
            PopupAction::SetSourceMode(CutOption::IncludeBranch)
        );
    }
}
