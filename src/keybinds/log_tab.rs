use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::config::LogTabKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_keybinds_help;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct LogTabKeybinds {
    // todo: probably split keys for different contexts, e.g when describe_textarea is opened
    keys: KeybindsStore<LogTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LogTabEvent {
    ScrollToBottom,
    ScrollToTop,

    ToggleHeadMark,

    GotoParent,

    CreateNew { describe: bool },
    Duplicate,
    Rebase,
    Squash { ignore_immutable: bool },
    EditChange { ignore_immutable: bool },
    Abandon,
    Absorb,
    Describe,
    EditRevset,
    SetBookmark,
    OpenFiles,
    OpenEvolog,
    CopyChangeId,
    CopyRev,

    Push(PushScope),
    Fetch { all_remotes: bool },

    Unbound,
}

/// Which bookmarks a push is to send.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PushScope {
    /// Those on the selected change that are tracked.
    Selected,
    /// Those on the selected change, tracking the new ones.
    SelectedWithNew,
    /// Every tracked bookmark.
    Tracked,
    /// Every bookmark, new ones included.
    All,
}

impl Default for LogTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<LogTabEvent>::default();
        set_keybinds!(
            keys,
            LogTabEvent::ScrollToBottom => "ctrl+end",
            LogTabEvent::ScrollToTop => "ctrl+home",
            LogTabEvent::ToggleHeadMark => "space",
            LogTabEvent::GotoParent => "-",
            LogTabEvent::Duplicate => "shift+d",
            LogTabEvent::CreateNew { describe: false } => "n",
            LogTabEvent::CreateNew { describe: true } => "shift+n",
            LogTabEvent::Rebase => "ctrl+r",
            LogTabEvent::Squash { ignore_immutable: false } => "s",
            LogTabEvent::Squash { ignore_immutable: true } => "shift+s",
            LogTabEvent::EditChange { ignore_immutable: false } => "e",
            LogTabEvent::EditChange { ignore_immutable: true } => "shift+e",
            LogTabEvent::Abandon => "a",
            LogTabEvent::Absorb => "shift+a",
            LogTabEvent::Describe => "d",
            LogTabEvent::EditRevset => "r",
            LogTabEvent::SetBookmark => "b",
            LogTabEvent::OpenFiles => "enter",
            LogTabEvent::OpenEvolog => "v",
            LogTabEvent::CopyChangeId => "y",
            LogTabEvent::CopyRev => "shift+y",
            LogTabEvent::Push(PushScope::Selected) => "p",
            LogTabEvent::Push(PushScope::SelectedWithNew) => "ctrl+p",
            LogTabEvent::Push(PushScope::Tracked) => "shift+p",
            LogTabEvent::Push(PushScope::All) => "ctrl+shift+p",
            LogTabEvent::Fetch { all_remotes: false } => "f",
            LogTabEvent::Fetch { all_remotes: true } => "shift+f",
        );

        Self { keys }
    }
}

impl LogTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = keybinds_config().and_then(|config| config.log_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    pub fn match_event(&self, event: KeyEvent) -> LogTabEvent {
        if let Some(action) = self.keys.match_event(event) {
            action
        } else {
            LogTabEvent::Unbound
        }
    }

    fn extend_from_config(&mut self, config: &LogTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            LogTabEvent::GotoParent => config.goto_parent,
            LogTabEvent::Duplicate => config.duplicate,
            LogTabEvent::CreateNew { describe: false } => config.create_new,
            LogTabEvent::CreateNew { describe: true } => config.create_new_describe,
            LogTabEvent::Squash { ignore_immutable: false } => config.squash,
            LogTabEvent::Squash { ignore_immutable: true } => config.squash_ignore_immutable,
            LogTabEvent::EditChange { ignore_immutable: false } => config.edit_change,
            LogTabEvent::EditChange { ignore_immutable: true } => config.edit_change_ignore_immutable,
            LogTabEvent::Abandon => config.abandon,
            LogTabEvent::Absorb => config.absorb,
            LogTabEvent::Describe => config.describe,
            LogTabEvent::EditRevset => config.edit_revset,
            LogTabEvent::SetBookmark => config.set_bookmark,
            LogTabEvent::OpenFiles => config.open_files,
            LogTabEvent::OpenEvolog => config.open_evolog,
            LogTabEvent::CopyChangeId => config.copy_change_id,
            LogTabEvent::CopyRev => config.copy_rev,
            LogTabEvent::Rebase => config.rebase,
            LogTabEvent::Push(PushScope::Selected) => config.push,
            LogTabEvent::Push(PushScope::SelectedWithNew) => config.push_new,
            LogTabEvent::Push(PushScope::Tracked) => config.push_all,
            LogTabEvent::Push(PushScope::All) => config.push_all_new,
            LogTabEvent::Fetch { all_remotes: false } => config.fetch,
            LogTabEvent::Fetch { all_remotes: true } => config.fetch_all,
        );
    }
    pub fn make_main_panel_help(&self) -> Vec<(String, String)> {
        make_keybinds_help!(
            self.keys,
            LogTabEvent::GotoParent => "go to parent commit",
            LogTabEvent::OpenFiles => "see files",
            LogTabEvent::OpenEvolog => "see how the change evolved",
            LogTabEvent::EditRevset => "set revset",
            LogTabEvent::Describe => "describe change",
            LogTabEvent::Duplicate => "duplicate change",
            LogTabEvent::EditChange { ignore_immutable: false } => "edit change",
            LogTabEvent::EditChange { ignore_immutable: true } => "edit change ignoring immutability",
            LogTabEvent::CreateNew { describe: false } => "new change",
            LogTabEvent::CreateNew { describe: true } => "new with message",
            LogTabEvent::Abandon => "abandon change",
            LogTabEvent::Absorb => "absorb selected change into its mutable ancestors",
            LogTabEvent::Rebase => "rebase @ to the selected change",
            LogTabEvent::Squash { ignore_immutable: false } => "squash @ into the selected change",
            LogTabEvent::Squash { ignore_immutable: true } => "squash @ into the selected change ignoring immutability",
            LogTabEvent::SetBookmark => "set bookmark",
            LogTabEvent::CopyChangeId => "yank change id to clipboard",
            LogTabEvent::CopyRev => "yank revision to clipboard",
            LogTabEvent::Fetch { all_remotes: false } => "git fetch",
            LogTabEvent::Fetch { all_remotes: true } => "git fetch all remotes",
            LogTabEvent::Push(PushScope::Selected) => "git push",
            LogTabEvent::Push(PushScope::SelectedWithNew) => "git push, tracking new bookmarks",
            LogTabEvent::Push(PushScope::Tracked) => "git push all tracked bookmarks",
            LogTabEvent::Push(PushScope::All) => "git push all bookmarks, new ones included",
        )
    }
}

#[test]
fn test_log_tab_keybinds_default() {
    let _ = LogTabKeybinds::default();
}
