use std::fmt::Display;
use std::str::FromStr;

pub use bookmark_set_popup::BookmarkSetPopupEvent;
pub use bookmark_set_popup::BookmarkSetPopupKeybinds;
pub use bookmarks_tab::BookmarksTabEvent;
pub use bookmarks_tab::BookmarksTabKeybinds;
pub use config::Keybind;
pub use config::KeybindsConfig;
pub use confirm_popup::ConfirmPopupEvent;
pub use confirm_popup::ConfirmPopupKeybinds;
pub use details_panel::DetailsPanelEvent;
pub use details_panel::DetailsPanelKeybinds;
pub use evolog_tab::EvologTabEvent;
pub use evolog_tab::EvologTabKeybinds;
pub use files_tab::FilesTabEvent;
pub use files_tab::FilesTabKeybinds;
pub use global::GlobalEvent;
pub use global::GlobalKeybinds;
pub use log_tab::LogTabEvent;
pub use log_tab::LogTabKeybinds;
pub use log_tab::PushScope;
pub use popup::PopupEvent;
pub use popup::PopupKeybinds;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEvent;
use ratatui::crossterm::event::KeyModifiers;
pub use settings_tab::SettingsTabEvent;
pub use settings_tab::SettingsTabKeybinds;

mod bookmark_set_popup;
mod bookmarks_tab;
mod config;
mod confirm_popup;
mod details_panel;
mod evolog_tab;
mod files_tab;
mod global;
mod keybinds_store;
mod log_tab;
mod popup;
pub mod rebase_popup;
mod settings_tab;

/*#[derive(Debug)]
pub struct Keybinds {
    log_tab: LogTabKeybinds,
}*/

/// The kind of thing a keybinding does, under which the help lists it.
/// Which one a binding belongs to follows from what it does, not from
/// who binds it, so the keys for moving around in the main panel are one
/// section whether a tab or the app as a whole binds them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Navigation,
    Changes,
    Files,
    BookmarksAndRemotes,
    Clipboard,
    Settings,
    DetailsPanel,
    /// What the app does, rather than what it does to the repo
    App,
}

impl Section {
    /// Every section, in the order the help lists them
    const ORDER: [Self; 8] = [
        Self::Navigation,
        Self::Changes,
        Self::Files,
        Self::BookmarksAndRemotes,
        Self::Clipboard,
        Self::Settings,
        Self::DetailsPanel,
        Self::App,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::Changes => "Changes",
            Self::Files => "Files",
            Self::BookmarksAndRemotes => "Bookmarks and remotes",
            Self::Clipboard => "Clipboard",
            Self::Settings => "Settings",
            Self::DetailsPanel => "Details panel",
            Self::App => "Global",
        }
    }

    /// Whether the help lists the section beside what the main panel
    /// answers to rather than among it.
    pub fn beside_main_panel(self) -> bool {
        matches!(self, Self::DetailsPanel | Self::App)
    }
}

/// A keybinding as the help lists it
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpItem {
    pub section: Section,
    /// The keys bound to it, or `[disabled]` where there are none
    pub keys: String,
    pub description: String,
}

/// The keybindings of one section, as the help lists them
#[derive(Clone, Debug)]
pub struct HelpSection {
    pub section: Section,
    /// The keys and what each of them does
    pub items: Vec<(String, String)>,
}

impl HelpSection {
    /// The `items` gathered into the sections they belong to, in the
    /// order the help lists those; a section nothing belongs to is left
    /// out.
    pub fn gather(items: impl IntoIterator<Item = HelpItem>) -> Vec<Self> {
        let items: Vec<HelpItem> = items.into_iter().collect();

        Section::ORDER
            .into_iter()
            .filter_map(|section| {
                let items: Vec<(String, String)> = items
                    .iter()
                    .filter(|item| item.section == section)
                    .map(|item| (item.keys.clone(), item.description.clone()))
                    .collect();

                (!items.is_empty()).then_some(Self { section, items })
            })
            .collect()
    }
}

/// Add keybindings to [`keybinds_store::KeybindsStore`]. Checks that shortcuts not duplicated
#[macro_export]
macro_rules! set_keybinds {
    () => {};
    ($keys:ident, $($action:expr => $shortcut:literal),* $(,)?) => {
        let mut __shortcuts_count = 0;
        $(
            $keys.add_action(Shortcut::from_str($shortcut).unwrap(), $action);
            __shortcuts_count += 1;
        )*
        debug_assert_eq!(__shortcuts_count, $keys.len(), "shortcuts should not duplicate");
    };
}

/// Replace keybindings in [`keybinds_store::KeybindsStore`] from config
#[macro_export]
macro_rules! update_keybinds {
    () => {};
    ($keys:expr, $($action:expr => $config:expr),* $(,)?) => {
        $(
            if let Some(ref k) = $config {
                $keys.replace_action_from_config($action, k);
            }
        )*
    };
}

/// The help entry of every listed action: what it does, the section it
/// belongs to, and the keys it is bound to.
#[macro_export]
macro_rules! make_keybinds_help {
    () => {};
    ($keys:expr, $($action:expr => $section:expr, $desc:literal),* $(,)?) => {
        #[allow(clippy::vec_init_then_push)]
        {
            let mut res = vec![];
            $(
                let shortcuts = $keys.get_shortcuts($action)
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join("/");
                res.push($crate::keybinds::HelpItem {
                    section: $section,
                    keys: if shortcuts.is_empty() {
                        "[disabled]".to_string()
                    } else {
                        shortcuts
                    },
                    description: $desc.to_string(),
                });
            )*
            res
        }
    };
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, serde_with::DeserializeFromStr)]
pub struct Shortcut {
    key: KeyCode,
    modifiers: KeyModifiers,
}

impl Shortcut {
    pub fn new_mod_key(modifiers: KeyModifiers, key: KeyCode) -> Self {
        Self { key, modifiers }
    }
    /// The character the shortcut is, where it is a plain one that no
    /// modifier goes with.
    pub fn as_char(&self) -> Option<char> {
        match self.key {
            KeyCode::Char(c) if self.modifiers.is_empty() => Some(c),
            _ => None,
        }
    }

    pub fn from_event(event: KeyEvent) -> Self {
        Self {
            key: match event.code {
                // when shift is pressed character is in upper case, so normalize it here
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                c => c,
            },
            modifiers: event.modifiers,
        }
    }
}

impl FromStr for Shortcut {
    type Err = ShortcutParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut modifiers = KeyModifiers::empty();
        let mut key = None;
        for s in s.to_lowercase().split('+').map(|s| s.trim()) {
            match s {
                "ctrl" => modifiers |= KeyModifiers::CONTROL,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                "space" => key = Some(KeyCode::Char(' ')),
                "enter" => key = Some(KeyCode::Enter),
                "esc" => key = Some(KeyCode::Esc),
                "left" => key = Some(KeyCode::Left),
                "right" => key = Some(KeyCode::Right),
                "up" => key = Some(KeyCode::Up),
                "down" => key = Some(KeyCode::Down),
                "home" => key = Some(KeyCode::Home),
                "end" => key = Some(KeyCode::End),
                "pagedown" => key = Some(KeyCode::PageDown),
                "pageup" => key = Some(KeyCode::PageUp),
                "menu" => key = Some(KeyCode::Menu),
                s if s.starts_with('f') && s.chars().count() > 1 => {
                    let s = s.trim_start_matches('f');
                    match s.parse::<u8>() {
                        Ok(k) => key = Some(KeyCode::F(k)),
                        Err(_) => return Err(ShortcutParseError::InvalidF),
                    }
                }
                s if s.chars().count() == 1 => {
                    let s = s.chars().next().unwrap();
                    key = Some(KeyCode::Char(s));
                }
                _ => (),
            }
        }

        if let Some(key) = key {
            Ok(Self::new_mod_key(modifiers, key))
        } else {
            Err(ShortcutParseError::NoKey)
        }
    }
}

impl Display for Shortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::with_capacity(3);
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("Control".to_string());
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("Shift".to_string());
        }
        let k = match self.key {
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::F(n) => format!("F{n}"),
            // A space of its own would be read as no key at all.
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Menu => "Menu".to_string(),
            _ => "Unknown".to_string(),
        };
        parts.push(k);

        parts.join("+").fmt(f)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ShortcutParseError {
    #[error("invalid number after f")]
    InvalidF,
    #[error("no key specified")]
    NoKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Shortcut {
        pub fn new_mod_char(modifiers: KeyModifiers, key: char) -> Self {
            Self::new_mod_key(modifiers, KeyCode::Char(key))
        }
        pub fn new_char(key: char) -> Self {
            Self::new_mod_key(KeyModifiers::empty(), KeyCode::Char(key))
        }
        pub fn new_key(key: KeyCode) -> Self {
            Self::new_mod_key(KeyModifiers::empty(), key)
        }
    }

    fn item(section: Section, key: &str) -> HelpItem {
        HelpItem {
            section,
            keys: key.to_owned(),
            description: format!("do {key}"),
        }
    }

    fn listed(key: &str) -> (String, String) {
        (key.to_owned(), format!("do {key}"))
    }

    #[test]
    fn test_gathering_puts_the_sections_in_the_order_the_help_lists_them() {
        let sections = HelpSection::gather([
            item(Section::App, "q"),
            item(Section::Changes, "n"),
            item(Section::Navigation, "j"),
        ]);

        let titles: Vec<Section> = sections.iter().map(|section| section.section).collect();
        assert_eq!(
            titles,
            vec![Section::Navigation, Section::Changes, Section::App]
        );
    }

    /// The keys of a section come from whoever binds them, so those the
    /// app binds and those a tab binds end up under the one heading.
    #[test]
    fn test_gathering_collects_a_section_from_every_source() {
        let sections = HelpSection::gather([
            item(Section::Navigation, "j"),
            item(Section::Changes, "n"),
            item(Section::Navigation, "-"),
        ]);

        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].items, vec![listed("j"), listed("-")]);
    }

    #[test]
    fn test_shortcut_from_str() {
        let ctrl = KeyModifiers::CONTROL;
        let shift = KeyModifiers::SHIFT;
        let ctrl_shift = ctrl | shift;

        let table = [
            ("q", Ok(Shortcut::new_char('q'))),
            ("Q", Ok(Shortcut::new_char('q'))),
            ("f", Ok(Shortcut::new_char('f'))),
            ("@", Ok(Shortcut::new_char('@'))),
            ("super+q", Ok(Shortcut::new_char('q'))),
            ("ctrl+q", Ok(Shortcut::new_mod_char(ctrl, 'q'))),
            ("ctrl+a+q", Ok(Shortcut::new_mod_char(ctrl, 'q'))),
            ("ctrl+Q", Ok(Shortcut::new_mod_char(ctrl, 'q'))),
            ("ctrl+ctrl+q", Ok(Shortcut::new_mod_char(ctrl, 'q'))),
            ("ctrl+shift+q", Ok(Shortcut::new_mod_char(ctrl_shift, 'q'))),
            (
                "ctrl+shift+f5",
                Ok(Shortcut::new_mod_key(ctrl_shift, KeyCode::F(5))),
            ),
            (
                "ctrl+shift+f25",
                Ok(Shortcut::new_mod_key(ctrl_shift, KeyCode::F(25))),
            ),
            ("enter", Ok(Shortcut::new_key(KeyCode::Enter))),
            (
                "ctrl+enter",
                Ok(Shortcut::new_mod_key(ctrl, KeyCode::Enter)),
            ),
            ("esc", Ok(Shortcut::new_key(KeyCode::Esc))),
            ("left", Ok(Shortcut::new_key(KeyCode::Left))),
            ("right", Ok(Shortcut::new_key(KeyCode::Right))),
            ("up", Ok(Shortcut::new_key(KeyCode::Up))),
            ("down", Ok(Shortcut::new_key(KeyCode::Down))),
            ("pagedown", Ok(Shortcut::new_key(KeyCode::PageDown))),
            ("pageup", Ok(Shortcut::new_key(KeyCode::PageUp))),
            ("ctrl+ff", Err(ShortcutParseError::InvalidF)),
            ("qq", Err(ShortcutParseError::NoKey)),
            ("", Err(ShortcutParseError::NoKey)),
        ];

        for (s, expected) in table {
            assert_eq!(
                Shortcut::from_str(s),
                expected,
                "Shortcut::from_str(\"{s}\")"
            );
        }
    }

    /// Every key a binding can be given has to have a name to show it
    /// by, or the help offers a key the user cannot make out.
    #[test]
    fn test_shortcut_display() {
        let table = [
            ("q", "q"),
            ("ctrl+shift+q", "Control+Shift+q"),
            ("space", "Space"),
            ("enter", "Enter"),
            ("esc", "Esc"),
            ("left", "Left"),
            ("right", "Right"),
            ("up", "Up"),
            ("down", "Down"),
            ("home", "Home"),
            ("end", "End"),
            ("ctrl+end", "Control+End"),
            ("pagedown", "PageDown"),
            ("pageup", "PageUp"),
            ("menu", "Menu"),
            ("f5", "F5"),
        ];

        for (s, expected) in table {
            let shortcut = Shortcut::from_str(s).expect("shortcut should parse");
            assert_eq!(
                shortcut.to_string(),
                expected,
                "Shortcut::from_str(\"{s}\")"
            );
        }
    }
}
