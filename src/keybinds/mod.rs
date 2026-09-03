use std::fmt::Display;
use std::str::FromStr;

pub use bookmark_set_popup::BookmarkSetPopupEvent;
pub use bookmark_set_popup::BookmarkSetPopupKeybinds;
pub use bookmarks_tab::BookmarksTabEvent;
pub use bookmarks_tab::BookmarksTabKeybinds;
pub use commands_tab::CommandsTabEvent;
pub use commands_tab::CommandsTabKeybinds;
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
pub use keybindings_tab::KeybindingsTabEvent;
pub use keybindings_tab::KeybindingsTabKeybinds;
pub use log_tab::LogTabEvent;
pub use log_tab::LogTabKeybinds;
pub use log_tab::PushScope;
pub use log_tab::Relation;
pub use menus_tab::MenusTabEvent;
pub use menus_tab::MenusTabKeybinds;
pub use op_log_tab::OpLogTabEvent;
pub use op_log_tab::OpLogTabKeybinds;
pub use popup::PopupEvent;
pub use popup::PopupKeybinds;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEvent;
use ratatui::crossterm::event::KeyModifiers;
pub use settings_tab::SettingsTabEvent;
pub use settings_tab::SettingsTabKeybinds;

use crate::env::keybinds_config;

mod bookmark_set_popup;
mod bookmarks_tab;
mod commands_tab;
mod config;
mod confirm_popup;
mod details_panel;
mod evolog_tab;
mod files_tab;
mod global;
mod keybindings_tab;
mod keybinds_store;
mod log_tab;
mod menus_tab;
mod op_log_tab;
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

/// Where a binding takes effect, which is also where it is configured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Global,
    LogTab,
    FilesTab,
    BookmarksTab,
    EvologTab,
    OpLogTab,
    SettingsTab,
    KeybindingsTab,
    CommandsTab,
    MenusTab,
    DetailsPanel,
    Popup,
    TextPopup,
    ConfirmPopup,
    BookmarkSetPopup,
    RebasePopup,
}

impl Context {
    /// Every context, in the order the keybindings are listed in
    pub const ORDER: [Self; 16] = [
        Self::Global,
        Self::LogTab,
        Self::FilesTab,
        Self::BookmarksTab,
        Self::EvologTab,
        Self::OpLogTab,
        Self::SettingsTab,
        Self::KeybindingsTab,
        Self::CommandsTab,
        Self::MenusTab,
        Self::DetailsPanel,
        Self::Popup,
        Self::TextPopup,
        Self::ConfirmPopup,
        Self::BookmarkSetPopup,
        Self::RebasePopup,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Global => "Everywhere",
            Self::LogTab => "Log tab",
            Self::FilesTab => "Files tab",
            Self::BookmarksTab => "Bookmarks tab",
            Self::EvologTab => "Evolog tab",
            Self::OpLogTab => "Operation log tab",
            Self::SettingsTab => "Settings tab",
            Self::KeybindingsTab => "Keybindings tab",
            Self::CommandsTab => "Commands tab",
            Self::MenusTab => "Context menus tab",
            Self::DetailsPanel => "Details panel",
            Self::Popup => "Popups",
            Self::TextPopup => "Popups holding a text field",
            Self::ConfirmPopup => "Confirmation popup",
            Self::BookmarkSetPopup => "Set-bookmark popup",
            Self::RebasePopup => "Rebase popup",
        }
    }

    /// The table under `blazingjj.keybinds` its bindings are configured
    /// in, the ones that hold everywhere being configured in that table
    /// itself.
    pub fn table(self) -> Option<&'static str> {
        match self {
            Self::Global => None,
            Self::LogTab => Some("log-tab"),
            Self::FilesTab => Some("files-tab"),
            Self::BookmarksTab => Some("bookmarks-tab"),
            Self::EvologTab => Some("evolog-tab"),
            Self::OpLogTab => Some("op-log-tab"),
            Self::SettingsTab => Some("settings-tab"),
            Self::KeybindingsTab => Some("keybindings-tab"),
            Self::CommandsTab => Some("commands-tab"),
            Self::MenusTab => Some("menus-tab"),
            Self::DetailsPanel => Some("details-panel"),
            Self::Popup => Some("popup"),
            Self::TextPopup => Some("text-popup"),
            Self::ConfirmPopup => Some("confirm-popup"),
            Self::BookmarkSetPopup => Some("bookmark-set-popup"),
            Self::RebasePopup => Some("rebase-popup"),
        }
    }

    /// Whether keys bound here and keys bound in `other` are ever live
    /// at once, so that one of them would answer for both. A tab and
    /// its details panel answer to the keys that hold everywhere as
    /// well, and a popup of its own to the keys every popup takes; the
    /// tabs are never up together, and nothing under a popup is asked
    /// while it is up.
    pub fn shares_keys_with(self, other: Self) -> bool {
        self == other || self.beside().contains(&other)
    }

    /// The contexts whose keys are live alongside its own.
    fn beside(self) -> &'static [Self] {
        /// What is live with the details panel: the keys that hold
        /// everywhere and those of the tabs that have one.
        const BESIDE_DETAILS_PANEL: [Context; 6] = [
            Context::Global,
            Context::LogTab,
            Context::FilesTab,
            Context::BookmarksTab,
            Context::EvologTab,
            Context::OpLogTab,
        ];
        const IN_A_TAB: [Context; 10] = [
            Context::LogTab,
            Context::FilesTab,
            Context::BookmarksTab,
            Context::EvologTab,
            Context::OpLogTab,
            Context::SettingsTab,
            Context::KeybindingsTab,
            Context::CommandsTab,
            Context::MenusTab,
            Context::DetailsPanel,
        ];
        const IN_A_POPUP: [Context; 4] = [
            Context::Popup,
            Context::ConfirmPopup,
            Context::BookmarkSetPopup,
            Context::RebasePopup,
        ];

        match self {
            // The keys that hold everywhere are up against those of
            // whichever tab is showing and those of its details panel.
            Self::Global => &IN_A_TAB,
            Self::DetailsPanel => &BESIDE_DETAILS_PANEL,
            Self::LogTab
            | Self::FilesTab
            | Self::BookmarksTab
            | Self::EvologTab
            | Self::OpLogTab => &[Self::Global, Self::DetailsPanel],
            // The tabs about the app have no details panel beside them.
            Self::SettingsTab | Self::KeybindingsTab | Self::CommandsTab | Self::MenusTab => {
                &[Self::Global]
            }
            Self::Popup => &IN_A_POPUP,
            Self::ConfirmPopup | Self::BookmarkSetPopup | Self::RebasePopup => &[Self::Popup],
            Self::TextPopup => &[],
        }
    }

    /// What it binds, as the configuration leaves it.
    pub fn bindings(self) -> Vec<Binding> {
        self.bindings_of(keybinds_config())
    }

    /// What it binds, as `config` leaves it.
    fn bindings_of(self, config: Option<&KeybindsConfig>) -> Vec<Binding> {
        match self {
            Self::Global => GlobalKeybinds::from_config(config).bindings(),
            Self::LogTab => LogTabKeybinds::from_config(config).bindings(),
            Self::FilesTab => FilesTabKeybinds::from_config(config).bindings(),
            Self::BookmarksTab => BookmarksTabKeybinds::from_config(config).bindings(),
            Self::EvologTab => EvologTabKeybinds::from_config(config).bindings(),
            Self::OpLogTab => OpLogTabKeybinds::from_config(config).bindings(),
            Self::SettingsTab => SettingsTabKeybinds::from_config(config).bindings(),
            Self::KeybindingsTab => KeybindingsTabKeybinds::from_config(config).bindings(),
            Self::CommandsTab => CommandsTabKeybinds::from_config(config).bindings(),
            Self::MenusTab => MenusTabKeybinds::from_config(config).bindings(),
            Self::DetailsPanel => DetailsPanelKeybinds::from_config(config).bindings(),
            Self::Popup => PopupKeybinds::dialog_from_config(config).bindings(),
            Self::TextPopup => PopupKeybinds::text_from_config(config).text_bindings(),
            Self::ConfirmPopup => ConfirmPopupKeybinds::from_config(config).bindings(),
            Self::BookmarkSetPopup => BookmarkSetPopupKeybinds::from_config(config).bindings(),
            Self::RebasePopup => rebase_popup::Keybinds::from_config(config).bindings(),
        }
    }
}

/// One action a key can be bound to: what it does, the keys it answers
/// to, and how it is configured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub context: Context,
    /// The name the configuration gives it, or None where the keys are
    /// not the user's to change.
    pub name: Option<&'static str>,
    /// The section the help lists it under, or None for a binding the
    /// help says nothing about: a popup says for itself what it takes.
    pub section: Option<Section>,
    pub description: &'static str,
    /// The keys bound to it, of which there may be none.
    pub keys: Vec<Shortcut>,
    /// The keys it comes bound to.
    pub defaults: Vec<Shortcut>,
}

impl Binding {
    /// The config key it is configured under, for a binding that is the
    /// user's to change.
    pub fn key(&self) -> Option<String> {
        let name = self.name?;

        Some(match self.context.table() {
            Some(table) => format!("blazingjj.keybinds.{table}.{name}"),
            None => format!("blazingjj.keybinds.{name}"),
        })
    }

    /// The keys bound to it, as they read.
    pub fn keys_text(&self) -> String {
        Self::text_of(&self.keys)
    }

    /// The keys it comes bound to, as they read.
    pub fn defaults_text(&self) -> String {
        Self::text_of(&self.defaults)
    }

    /// `shortcuts` as they read, or `[disabled]` where there are none.
    fn text_of(shortcuts: &[Shortcut]) -> String {
        if shortcuts.is_empty() {
            return "[disabled]".to_owned();
        }

        shortcuts
            .iter()
            .map(Shortcut::to_string)
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// The keybindings of one section, as the help lists them
#[derive(Clone, Debug)]
pub struct HelpSection {
    pub section: Section,
    /// The keys and what each of them does
    pub items: Vec<(String, String)>,
}

impl HelpSection {
    /// The `bindings` gathered into the sections they belong to, in the
    /// order the help lists those; a section nothing belongs to is left
    /// out.
    pub fn gather(bindings: impl IntoIterator<Item = Binding>) -> Vec<Self> {
        let bindings: Vec<Binding> = bindings.into_iter().collect();

        Section::ORDER
            .into_iter()
            .filter_map(|section| {
                let items: Vec<(String, String)> = bindings
                    .iter()
                    .filter(|binding| binding.section == Some(section))
                    .map(|binding| (binding.keys_text(), binding.description.to_owned()))
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

/// Every action of one context: the name the configuration gives it, or
/// `_` for one that is not the user's to bind, the section the help
/// lists it under, and what it does. The keys come from `$keys` as they
/// are and from `$defaults` as the app ships them.
#[macro_export]
macro_rules! make_bindings {
    ($keys:expr, $defaults:expr, $context:expr, $($action:expr => $name:tt, $section:expr, $desc:literal),* $(,)?) => {
        #[allow(clippy::vec_init_then_push)]
        {
            let defaults = $defaults;
            let mut res = vec![];
            $(
                res.push($crate::keybinds::Binding {
                    context: $context,
                    name: $crate::binding_name!($name),
                    section: $section,
                    description: $desc,
                    keys: $keys.get_shortcuts($action),
                    defaults: defaults.get_shortcuts($action),
                });
            )*
            res
        }
    };
}

/// The name a binding is configured under, `_` standing for one that
/// cannot be.
#[macro_export]
macro_rules! binding_name {
    (_) => {
        None
    };
    ($name:literal) => {
        Some($name)
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
        if s.trim().is_empty() {
            return Err(ShortcutParseError::NoKey);
        }

        for s in s.to_lowercase().split('+').map(|s| s.trim()) {
            match s {
                // Written out is how a shortcut reads on screen, which
                // is what someone copying one out of the app writes.
                "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
                "alt" => modifiers |= KeyModifiers::ALT,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                // A plus is what the parts are told apart by, so it is
                // no part of its own; what is left where one stands on
                // its own is the key it is.
                "" => key = Some(KeyCode::Char('+')),
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
                "tab" => key = Some(KeyCode::Tab),
                // What a terminal reports for Shift+Tab, which it tells
                // apart from a Tab of its own.
                "backtab" => key = Some(KeyCode::BackTab),
                "backspace" => key = Some(KeyCode::Backspace),
                "delete" => key = Some(KeyCode::Delete),
                "insert" => key = Some(KeyCode::Insert),
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
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("Alt".to_string());
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
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "BackTab".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Insert => "Insert".to_string(),
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

    fn item(section: Section, key: &'static str) -> Binding {
        Binding {
            context: Context::Global,
            name: Some(key),
            section: Some(section),
            description: key,
            keys: vec![Shortcut::from_str(key).expect("shortcut should parse")],
            defaults: Vec::new(),
        }
    }

    fn listed(key: &str) -> (String, String) {
        (key.to_owned(), key.to_owned())
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
        let alt = KeyModifiers::ALT;
        let ctrl_shift = ctrl | shift;
        let ctrl_alt_shift = ctrl | alt | shift;

        let table = [
            ("q", Ok(Shortcut::new_char('q'))),
            ("+", Ok(Shortcut::new_char('+'))),
            ("ctrl++", Ok(Shortcut::new_mod_char(ctrl, '+'))),
            ("Q", Ok(Shortcut::new_char('q'))),
            ("f", Ok(Shortcut::new_char('f'))),
            ("@", Ok(Shortcut::new_char('@'))),
            ("super+q", Ok(Shortcut::new_char('q'))),
            ("ctrl+q", Ok(Shortcut::new_mod_char(ctrl, 'q'))),
            ("ctrl+a+q", Ok(Shortcut::new_mod_char(ctrl, 'q'))),
            ("ctrl+Q", Ok(Shortcut::new_mod_char(ctrl, 'q'))),
            ("ctrl+ctrl+q", Ok(Shortcut::new_mod_char(ctrl, 'q'))),
            ("ctrl+shift+q", Ok(Shortcut::new_mod_char(ctrl_shift, 'q'))),
            ("alt+q", Ok(Shortcut::new_mod_char(alt, 'q'))),
            (
                "ctrl+alt+shift+q",
                Ok(Shortcut::new_mod_char(ctrl_alt_shift, 'q')),
            ),
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
            ("tab", Ok(Shortcut::new_key(KeyCode::Tab))),
            ("backtab", Ok(Shortcut::new_key(KeyCode::BackTab))),
            ("backspace", Ok(Shortcut::new_key(KeyCode::Backspace))),
            ("delete", Ok(Shortcut::new_key(KeyCode::Delete))),
            ("insert", Ok(Shortcut::new_key(KeyCode::Insert))),
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
            ("alt+q", "Alt+q"),
            ("ctrl+alt+shift+q", "Control+Alt+Shift+q"),
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
            ("tab", "Tab"),
            ("backtab", "BackTab"),
            ("backspace", "Backspace"),
            ("delete", "Delete"),
            ("insert", "Insert"),
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

    /// An action is offered under the key the configuration is to bind
    /// it by, so binding it under that key has to be what reaches it.
    #[test]
    fn test_every_action_is_bound_by_the_key_it_is_offered_under() {
        let bound = Shortcut::from_str("f12").expect("shortcut should parse");

        for context in Context::ORDER {
            for binding in context.bindings_of(None) {
                let Some(key) = binding.key() else {
                    continue;
                };
                let named = key
                    .strip_prefix("blazingjj.keybinds.")
                    .expect("a binding is configured under the keybinds table");
                let config: KeybindsConfig = toml::from_str(&format!("{named} = \"f12\"\n"))
                    .expect("the configuration parses");

                let rebound = context
                    .bindings_of(Some(&config))
                    .into_iter()
                    .find(|rebound| rebound.name == binding.name)
                    .expect("the action is still listed");
                assert_eq!(rebound.keys, vec![bound], "{key}");
            }
        }
    }

    /// A shortcut is offered by the name it reads by, so that name has
    /// to be one the configuration takes back.
    #[test]
    fn test_a_shortcut_reads_back_as_the_one_it_was_written_from() {
        for s in [
            "q",
            "ctrl+q",
            "shift+q",
            "ctrl+shift+f5",
            "alt+q",
            "ctrl+alt+delete",
            "space",
            "enter",
            "esc",
            "ctrl+end",
            "menu",
            "tab",
            "shift+backtab",
            "backspace",
            "delete",
            "insert",
        ] {
            let shortcut = Shortcut::from_str(s).expect("shortcut should parse");

            assert_eq!(
                Shortcut::from_str(&shortcut.to_string()),
                Ok(shortcut),
                "{shortcut}"
            );
        }
    }
}
