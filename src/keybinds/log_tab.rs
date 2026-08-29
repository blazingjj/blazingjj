use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::config::KeybindsConfig;
use super::config::LogTabKeybindsConfig;
use super::keybinds_store::KeybindsStore;
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
    Save,
    Cancel,

    ScrollToBottom,
    ScrollToTop,

    ToggleHeadMark,

    GotoParent,

    CreateNew {
        describe: bool,
    },
    Duplicate,
    Rebase,
    Squash {
        ignore_immutable: bool,
    },
    EditChange {
        ignore_immutable: bool,
    },
    Abandon,
    Absorb,
    Describe,
    EditRevset,
    SetBookmark,
    OpenFiles,
    OpenEvolog,
    CopyChangeId,
    CopyRev,

    Push {
        all_bookmarks: bool,
        allow_new: bool,
    },
    Fetch {
        all_remotes: bool,
    },

    OpenContextMenu,

    Unbound,
}

impl Default for LogTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<LogTabEvent>::default();
        set_keybinds!(
            keys,
            LogTabEvent::Save => "ctrl+s",
            LogTabEvent::Cancel => "esc",
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
            event_push(false, false) => "p",
            event_push(false, true) => "ctrl+p",
            event_push(true, false) => "shift+p",
            event_push(true, true) => "ctrl+shift+p",
            LogTabEvent::Fetch { all_remotes: false } => "f",
            LogTabEvent::Fetch { all_remotes: true } => "shift+f",
            LogTabEvent::OpenContextMenu => "menu",
        );

        Self { keys }
    }
}

impl LogTabKeybinds {
    pub fn match_event(&self, event: KeyEvent) -> LogTabEvent {
        if let Some(action) = self.keys.match_event(event) {
            action
        } else {
            LogTabEvent::Unbound
        }
    }
    pub fn extend_from_config(&mut self, config: &KeybindsConfig) {
        if let Some(ref log_tab) = config.log_tab {
            self.extend_from_log_tab_config(log_tab);
        }
    }

    fn extend_from_log_tab_config(&mut self, config: &LogTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            LogTabEvent::Save => config.save,
            LogTabEvent::Cancel => config.cancel,
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
            event_push(false, false) => config.push,
            event_push(false, true) => config.push_new,
            event_push(true, false) => config.push_all,
            event_push(true, true) => config.push_all_new,
            LogTabEvent::Fetch { all_remotes: false } => config.fetch,
            LogTabEvent::Fetch { all_remotes: true } => config.fetch_all,
            LogTabEvent::OpenContextMenu => config.open_context_menu,
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
            event_push(false, false) => "git push",
            event_push(false, true) => "git push with new bookmarks",
            event_push(true, false) => "git push all bookmarks, except new",
            event_push(true, true) => "git push all bookmarks",
            LogTabEvent::OpenContextMenu => "open the context menu",
        )
    }
}

fn event_push(all_bookmarks: bool, allow_new: bool) -> LogTabEvent {
    LogTabEvent::Push {
        all_bookmarks,
        allow_new,
    }
}

#[test]
fn test_log_tab_keybinds_default() {
    let _ = LogTabKeybinds::default();
}
