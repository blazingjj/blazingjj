/*! Keys the set-bookmark popup has of its own, for the options it lists
by the letter they are picked by. They are matched after the keys every
popup answers to.
*/

use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Shortcut;
use super::config::BookmarkSetPopupKeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BookmarkSetPopupEvent {
    UseGeneratedName,
    CreateBookmark,

    Unbound,
}

#[derive(Debug)]
pub struct BookmarkSetPopupKeybinds {
    keys: KeybindsStore<BookmarkSetPopupEvent>,
}

impl Default for BookmarkSetPopupKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<BookmarkSetPopupEvent>::default();
        set_keybinds!(
            keys,
            BookmarkSetPopupEvent::UseGeneratedName => "g",
            BookmarkSetPopupEvent::CreateBookmark => "c",
        );
        Self { keys }
    }
}

impl BookmarkSetPopupKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        let mut keybinds = Self::default();
        if let Some(config) =
            keybinds_config().and_then(|config| config.bookmark_set_popup.as_ref())
        {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    pub fn match_event(&self, event: KeyEvent) -> BookmarkSetPopupEvent {
        self.keys
            .match_event(event)
            .unwrap_or(BookmarkSetPopupEvent::Unbound)
    }

    /// The shortcut to name `event` by, of those bound to it.
    pub fn shortcut(&self, event: BookmarkSetPopupEvent) -> Option<Shortcut> {
        self.keys.get_shortcuts(event).into_iter().next()
    }

    fn extend_from_config(&mut self, config: &BookmarkSetPopupKeybindsConfig) {
        update_keybinds!(
            self.keys,
            BookmarkSetPopupEvent::UseGeneratedName => config.use_generated_name,
            BookmarkSetPopupEvent::CreateBookmark => config.create_bookmark,
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
        let config = BookmarkSetPopupKeybindsConfig {
            create_bookmark: Some(Keybind::Single(
                Shortcut::from_str("ctrl+n").expect("shortcut should parse"),
            )),
            use_generated_name: None,
        };

        let mut keybinds = BookmarkSetPopupKeybinds::default();
        keybinds.extend_from_config(&config);

        assert_eq!(
            keybinds.match_event(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL)),
            BookmarkSetPopupEvent::CreateBookmark
        );
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('c'))),
            BookmarkSetPopupEvent::Unbound
        );

        // Anything the config leaves out keeps its default.
        assert_eq!(
            keybinds.match_event(key(KeyCode::Char('g'))),
            BookmarkSetPopupEvent::UseGeneratedName
        );
    }
}
