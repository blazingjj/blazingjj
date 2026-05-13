use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::keybinds_store::KeybindsStore;
use crate::make_keybinds_help;
use crate::set_keybinds;

#[derive(Debug)]
pub struct BookmarksTabKeybinds {
    keys: KeybindsStore<BookmarksTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BookmarksTabEvent {
    ToggleDiffFormat,
    ToggleShowAll,
    CreateBookmark,
    RenameBookmark,
    DeleteBookmark,
    ForgetBookmark,
    TrackBookmark,
    UntrackBookmark,
    NewChange { describe: bool },
    EditChange { ignore_immutable: bool },
    ViewInLog,

    Unbound,
}

impl Default for BookmarksTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<BookmarksTabEvent>::default();
        set_keybinds!(
            keys,
            BookmarksTabEvent::ToggleDiffFormat => "w",
            BookmarksTabEvent::ToggleShowAll => "a",
            BookmarksTabEvent::CreateBookmark => "c",
            BookmarksTabEvent::RenameBookmark => "r",
            BookmarksTabEvent::DeleteBookmark => "d",
            BookmarksTabEvent::ForgetBookmark => "f",
            BookmarksTabEvent::TrackBookmark => "t",
            BookmarksTabEvent::UntrackBookmark => "shift+t",
            BookmarksTabEvent::NewChange { describe: false } => "n",
            BookmarksTabEvent::NewChange { describe: true } => "shift+n",
            BookmarksTabEvent::EditChange { ignore_immutable: false } => "e",
            BookmarksTabEvent::EditChange { ignore_immutable: true } => "shift+e",
            BookmarksTabEvent::ViewInLog => "enter",
        );
        Self { keys }
    }
}

impl BookmarksTabKeybinds {
    pub fn match_event(&self, event: KeyEvent) -> BookmarksTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(BookmarksTabEvent::Unbound)
    }

    pub fn make_help(&self) -> Vec<(String, String)> {
        make_keybinds_help!(
            self.keys,
            BookmarksTabEvent::ToggleShowAll => "show all remotes",
            BookmarksTabEvent::CreateBookmark => "create bookmark",
            BookmarksTabEvent::RenameBookmark => "rename bookmark",
            BookmarksTabEvent::DeleteBookmark => "delete bookmark",
            BookmarksTabEvent::ForgetBookmark => "forget bookmark",
            BookmarksTabEvent::TrackBookmark => "track bookmark",
            BookmarksTabEvent::UntrackBookmark => "untrack bookmark",
            BookmarksTabEvent::ViewInLog => "view in log",
            BookmarksTabEvent::NewChange { describe: false } => "new from bookmark",
            BookmarksTabEvent::NewChange { describe: true } => "new and describe",
            BookmarksTabEvent::EditChange { ignore_immutable: false } => "edit bookmark",
            BookmarksTabEvent::EditChange { ignore_immutable: true } => "edit bookmark ignoring immutability",
        )
    }
}

#[test]
fn test_bookmarks_tab_keybinds_default() {
    let _ = BookmarksTabKeybinds::default();
}
