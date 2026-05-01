use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::KeybindsConfig;
use super::config::LogTabKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct LogTabKeybinds {
    // todo: probably split keys for different contexts, e.g when describe_textarea is opened
    keys: KeybindsStore<LogTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LogTabEvent {
    ToggleHeadMark,
    UseMarks,

    Goto(Relation),

    CreateNew { describe: bool },
    Duplicate,
    Parallelize,
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

    PushMenu,
    Push(PushScope),
    Fetch { all_remotes: bool },

    Unbound,
}

/// Which way along the graph a move of the selection goes.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Relation {
    Parent,
    Child,
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
    /// A new bookmark for the selected change, named after it.
    Change,
    /// A new bookmark for the selected change, under a name to be given.
    Named,
}

impl Default for LogTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<LogTabEvent>::default();
        set_keybinds!(
            keys,
            LogTabEvent::ToggleHeadMark => "space",
            LogTabEvent::UseMarks => ";",
            LogTabEvent::Goto(Relation::Parent) => "-",
            LogTabEvent::Goto(Relation::Child) => "+",
            LogTabEvent::Duplicate => "shift+d",
            LogTabEvent::Parallelize => "|",
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
            LogTabEvent::PushMenu => "p",
            LogTabEvent::Fetch { all_remotes: false } => "f",
            LogTabEvent::Fetch { all_remotes: true } => "shift+f",
        );

        Self { keys }
    }
}

impl LogTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.log_tab.as_ref()) {
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
            LogTabEvent::ToggleHeadMark => config.mark_head,
            LogTabEvent::UseMarks => config.use_marks,
            LogTabEvent::Goto(Relation::Parent) => config.goto_parent,
            LogTabEvent::Goto(Relation::Child) => config.goto_child,
            LogTabEvent::Duplicate => config.duplicate,
            LogTabEvent::Parallelize => config.parallelize,
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
            LogTabEvent::PushMenu => config.push_menu,
            LogTabEvent::Push(PushScope::Selected) => config.push,
            LogTabEvent::Push(PushScope::SelectedWithNew) => config.push_new,
            LogTabEvent::Push(PushScope::Tracked) => config.push_all,
            LogTabEvent::Push(PushScope::All) => config.push_all_new,
            LogTabEvent::Push(PushScope::Change) => config.push_change,
            LogTabEvent::Push(PushScope::Named) => config.push_named,
            LogTabEvent::Fetch { all_remotes: false } => config.fetch,
            LogTabEvent::Fetch { all_remotes: true } => config.fetch_all,
        );
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::LogTab,
            LogTabEvent::Goto(Relation::Parent) => "goto-parent", Some(Section::Navigation), "go to parent commit",
            LogTabEvent::Goto(Relation::Child) => "goto-child", Some(Section::Navigation), "go to child commit",
            LogTabEvent::OpenFiles => "open-files", Some(Section::Navigation), "see files",
            LogTabEvent::OpenEvolog => "open-evolog", Some(Section::Navigation), "see how the change evolved",
            LogTabEvent::EditRevset => "edit-revset", Some(Section::Navigation), "set revset",

            LogTabEvent::ToggleHeadMark => "mark-head", Some(Section::Changes), "mark change to act on",
            LogTabEvent::UseMarks => "use-marks", Some(Section::Changes), "make the next operation act on the marked changes",
            LogTabEvent::CreateNew { describe: false } => "create-new", Some(Section::Changes), "new change",
            LogTabEvent::CreateNew { describe: true } => "create-new-describe", Some(Section::Changes), "new with message",
            LogTabEvent::Describe => "describe", Some(Section::Changes), "describe change",
            LogTabEvent::EditChange { ignore_immutable: false } => "edit-change", Some(Section::Changes), "edit change",
            LogTabEvent::EditChange { ignore_immutable: true } => "edit-change-ignore-immutable", Some(Section::Changes), "edit change ignoring immutability",
            LogTabEvent::Duplicate => "duplicate", Some(Section::Changes), "duplicate change",
            LogTabEvent::Parallelize => "parallelize", Some(Section::Changes), "parallelize changes",
            LogTabEvent::Abandon => "abandon", Some(Section::Changes), "abandon change",
            LogTabEvent::Rebase => "rebase", Some(Section::Changes), "rebase @ onto the selection",
            LogTabEvent::Squash { ignore_immutable: false } => "squash", Some(Section::Changes), "squash @ into the selection",
            LogTabEvent::Squash { ignore_immutable: true } => "squash-ignore-immutable", Some(Section::Changes), "squash @ into the selection, ignoring immutability",
            LogTabEvent::Absorb => "absorb", Some(Section::Changes), "absorb the selection into its mutable ancestors",

            LogTabEvent::SetBookmark => "set-bookmark", Some(Section::BookmarksAndRemotes), "set bookmark",
            LogTabEvent::Fetch { all_remotes: false } => "fetch", Some(Section::BookmarksAndRemotes), "git fetch",
            LogTabEvent::Fetch { all_remotes: true } => "fetch-all", Some(Section::BookmarksAndRemotes), "git fetch all remotes",
            LogTabEvent::PushMenu => "push-menu", Some(Section::BookmarksAndRemotes), "open the push menu",
            LogTabEvent::Push(PushScope::Selected) => "push", Some(Section::BookmarksAndRemotes), "git push",
            LogTabEvent::Push(PushScope::SelectedWithNew) => "push-new", Some(Section::BookmarksAndRemotes), "git push, tracking new bookmarks",
            LogTabEvent::Push(PushScope::Tracked) => "push-all", Some(Section::BookmarksAndRemotes), "git push all tracked bookmarks",
            LogTabEvent::Push(PushScope::All) => "push-all-new", Some(Section::BookmarksAndRemotes), "git push all bookmarks, new ones included",
            LogTabEvent::Push(PushScope::Change) => "push-change", Some(Section::BookmarksAndRemotes), "git push a new bookmark named after the change",
            LogTabEvent::Push(PushScope::Named) => "push-named", Some(Section::BookmarksAndRemotes), "git push a new bookmark under a name of your own",

            LogTabEvent::CopyChangeId => "copy-change-id", Some(Section::Clipboard), "yank change id",
            LogTabEvent::CopyRev => "copy-rev", Some(Section::Clipboard), "yank revision",
        )
    }
}

#[test]
fn test_log_tab_keybinds_default() {
    let _ = LogTabKeybinds::default();
}
