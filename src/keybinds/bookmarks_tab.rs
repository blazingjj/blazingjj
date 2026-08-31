use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::BookmarksTabKeybindsConfig;
use super::config::KeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct BookmarksTabKeybinds {
    keys: KeybindsStore<BookmarksTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BookmarksTabEvent {
    ToggleShowAll,
    CreateBookmark,
    RenameBookmark,
    DeleteBookmark,
    ForgetBookmark,
    TrackBookmark,
    UntrackBookmark,
    /// Point the bookmark at the change the selected line stands for,
    /// which for one of several targets is what settles it on that one.
    SetBookmark,
    NewChange {
        describe: bool,
    },
    EditChange {
        ignore_immutable: bool,
    },
    ViewInLog,

    Unbound,
}

impl Default for BookmarksTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<BookmarksTabEvent>::default();
        set_keybinds!(
            keys,
            BookmarksTabEvent::ToggleShowAll => "a",
            BookmarksTabEvent::CreateBookmark => "c",
            BookmarksTabEvent::RenameBookmark => "r",
            BookmarksTabEvent::DeleteBookmark => "d",
            BookmarksTabEvent::ForgetBookmark => "f",
            BookmarksTabEvent::TrackBookmark => "t",
            BookmarksTabEvent::UntrackBookmark => "shift+t",
            BookmarksTabEvent::SetBookmark => "b",
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
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.bookmarks_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &BookmarksTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            BookmarksTabEvent::ToggleShowAll => config.toggle_show_all,
            BookmarksTabEvent::CreateBookmark => config.create_bookmark,
            BookmarksTabEvent::RenameBookmark => config.rename_bookmark,
            BookmarksTabEvent::DeleteBookmark => config.delete_bookmark,
            BookmarksTabEvent::ForgetBookmark => config.forget_bookmark,
            BookmarksTabEvent::TrackBookmark => config.track_bookmark,
            BookmarksTabEvent::UntrackBookmark => config.untrack_bookmark,
            BookmarksTabEvent::SetBookmark => config.set_bookmark,
            BookmarksTabEvent::ViewInLog => config.view_in_log,
            BookmarksTabEvent::NewChange { describe: false } => config.create_new,
            BookmarksTabEvent::NewChange { describe: true } => config.create_new_describe,
            BookmarksTabEvent::EditChange { ignore_immutable: false } => config.edit_change,
            BookmarksTabEvent::EditChange { ignore_immutable: true } => config.edit_change_ignore_immutable,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> BookmarksTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(BookmarksTabEvent::Unbound)
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::BookmarksTab,
            BookmarksTabEvent::ViewInLog => "view-in-log", Some(Section::Navigation), "view in log",
            BookmarksTabEvent::ToggleShowAll => "toggle-show-all", Some(Section::Navigation), "show all remotes",

            BookmarksTabEvent::NewChange { describe: false } => "create-new", Some(Section::Changes), "new from bookmark",
            BookmarksTabEvent::NewChange { describe: true } => "create-new-describe", Some(Section::Changes), "new and describe",
            BookmarksTabEvent::EditChange { ignore_immutable: false } => "edit-change", Some(Section::Changes), "edit bookmark",
            BookmarksTabEvent::EditChange { ignore_immutable: true } => "edit-change-ignore-immutable", Some(Section::Changes), "edit bookmark ignoring immutability",

            BookmarksTabEvent::CreateBookmark => "create-bookmark", Some(Section::BookmarksAndRemotes), "create bookmark",
            BookmarksTabEvent::RenameBookmark => "rename-bookmark", Some(Section::BookmarksAndRemotes), "rename bookmark",
            BookmarksTabEvent::DeleteBookmark => "delete-bookmark", Some(Section::BookmarksAndRemotes), "delete bookmark",
            BookmarksTabEvent::ForgetBookmark => "forget-bookmark", Some(Section::BookmarksAndRemotes), "forget bookmark",
            BookmarksTabEvent::TrackBookmark => "track-bookmark", Some(Section::BookmarksAndRemotes), "track bookmark",
            BookmarksTabEvent::UntrackBookmark => "untrack-bookmark", Some(Section::BookmarksAndRemotes), "untrack bookmark",
            BookmarksTabEvent::SetBookmark => "set-bookmark", Some(Section::BookmarksAndRemotes), "set bookmark to the selection",
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

    fn config() -> BookmarksTabKeybindsConfig {
        BookmarksTabKeybindsConfig {
            toggle_show_all: None,
            create_bookmark: None,
            rename_bookmark: None,
            delete_bookmark: None,
            forget_bookmark: None,
            track_bookmark: None,
            untrack_bookmark: None,
            set_bookmark: None,
            view_in_log: None,
            create_new: None,
            create_new_describe: None,
            edit_change: None,
            edit_change_ignore_immutable: None,
        }
    }

    #[test]
    fn test_bookmarks_tab_keybinds_default() {
        let _ = BookmarksTabKeybinds::default();
    }

    #[test]
    fn test_extend_from_config_replaces_bindings() {
        let config = BookmarksTabKeybindsConfig {
            delete_bookmark: Some(Keybind::Single(
                Shortcut::from_str("ctrl+x").expect("shortcut should parse"),
            )),
            create_new: Some(Keybind::Enable(false)),
            ..config()
        };

        let mut keybinds = BookmarksTabKeybinds::default();
        keybinds.extend_from_config(&config);

        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL)),
            BookmarksTabEvent::DeleteBookmark
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('d'))),
            BookmarksTabEvent::Unbound
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('n'))),
            BookmarksTabEvent::Unbound
        );

        // The two changes a describe tells apart stay apart.
        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT)),
            BookmarksTabEvent::NewChange { describe: true }
        );
    }
}
