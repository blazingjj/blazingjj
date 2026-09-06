/*! The colours the app draws in.

An element is drawn in a [Role], which gives a [Style] so that it carries
a background as well as a foreground. Styles are patched onto what is
already there rather than set: a role that says nothing about a channel
keeps whatever was underneath, jj's own colouring included.

A role falls back for a channel it says nothing about itself: to the role
it is a kind of, where it is one, and in the end to [Role::Default], so
that a background set there is set for the whole app. Left unset, the
terminal's own shows through.
*/

mod color;

use std::collections::HashMap;
use std::sync::LazyLock;

pub use color::Ansi;
pub use color::ThemeColor;
use ratatui::style::Color;
use ratatui::style::Style;
use serde::Deserialize;
use serde::Deserializer;
use serde::de;

use crate::env::JjConfig;
use crate::env::configured_theme;

/// What the app draws an element for. The colour of a role is the colour
/// of every element drawn in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// What everything else falls back to, and what is drawn in nothing
    /// more particular.
    Default,
    /// The row, tab or button the keys act on.
    Highlight,
    /// What is worth saying but not worth reading first: the keys a
    /// panel answers to, what an option falls back to, where a binding
    /// comes from, a row that cannot be picked.
    Hint,
    /// The rules a popup divides itself with.
    Separator,
    /// The border around a panel of the frame.
    Border,
    /// The workspace the app is running in, as the status bar names it.
    Workspace,
    /// The title a panel of the frame is drawn under.
    PanelTitle,
    /// The heading a list is divided under.
    Heading,
    /// The border around a popup.
    PopupBorder,
    /// The title of a popup.
    PopupTitle,
    /// What went wrong.
    Error,
    /// What is about to make a change rather than pick something that is
    /// already there.
    Warning,
    /// What just worked, for as long as it is worth showing.
    Success,
    /// A button that is not the one Enter presses.
    Button,
    /// The button Enter presses.
    ButtonActive,
    /// A change id.
    ChangeId,
    /// The name of a bookmark.
    Bookmark,
    /// A file the change adds.
    FileAdded,
    /// A file the change changes.
    FileModified,
    /// A file the change renames.
    FileRenamed,
    /// A file the change copies.
    FileCopied,
    /// A file the change deletes.
    FileDeleted,
    /// A file left conflicted.
    Conflict,
}

impl Role {
    /// Every role there is, in the order a list of them reads best in
    /// rather than the order they are declared in.
    pub const ALL: [Self; 23] = [
        Self::Default,
        Self::Highlight,
        Self::Hint,
        Self::Separator,
        Self::Border,
        Self::Workspace,
        Self::PanelTitle,
        Self::Heading,
        Self::PopupBorder,
        Self::PopupTitle,
        Self::Error,
        Self::Warning,
        Self::Success,
        Self::Button,
        Self::ButtonActive,
        Self::ChangeId,
        Self::Bookmark,
        Self::FileAdded,
        Self::FileModified,
        Self::FileRenamed,
        Self::FileCopied,
        Self::FileDeleted,
        Self::Conflict,
    ];

    /// Where the role's own colours sit, which is where it is declared
    /// rather than where [Role::ALL] lists it.
    fn index(self) -> usize {
        self as usize
    }

    /// What the role is called under `blazingjj.colors`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Highlight => "highlight",
            Self::Hint => "hint",
            Self::Separator => "separator",
            Self::Border => "border",
            Self::Workspace => "workspace",
            Self::PanelTitle => "panel-title",
            Self::Heading => "heading",
            Self::PopupBorder => "popup-border",
            Self::PopupTitle => "popup-title",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Success => "success",
            Self::Button => "button",
            Self::ButtonActive => "button-active",
            Self::ChangeId => "change-id",
            Self::Bookmark => "bookmark",
            Self::FileAdded => "file-added",
            Self::FileModified => "file-modified",
            Self::FileRenamed => "file-renamed",
            Self::FileCopied => "file-copied",
            Self::FileDeleted => "file-deleted",
            Self::Conflict => "conflict",
        }
    }

    /// What the role is drawn in while the configuration says nothing
    /// about it. A channel left out here is one the role has no colour
    /// of its own for, and so falls back to [Role::Default] for.
    fn builtin(self) -> RoleColors {
        let fg = |color| RoleColors {
            fg: Some(color),
            bg: None,
        };
        let ansi = |ansi| fg(ThemeColor::Ansi(ansi));

        match self {
            // Told apart by placement and boldness, or by what they fall
            // back to, rather than by a colour of their own.
            Self::Default
            | Self::Border
            | Self::PanelTitle
            | Self::Heading
            | Self::ButtonActive => RoleColors::default(),
            Self::Highlight => RoleColors {
                fg: None,
                bg: Some(ThemeColor::Rgb(50, 50, 150)),
            },
            Self::Separator => ansi(Ansi::BrightBlack),
            Self::Hint => ansi(Ansi::White),
            Self::PopupBorder | Self::Success | Self::FileAdded => ansi(Ansi::Green),
            Self::PopupTitle
            | Self::Workspace
            | Self::FileModified
            | Self::FileRenamed
            | Self::FileCopied => ansi(Ansi::Cyan),
            Self::Error | Self::FileDeleted | Self::Conflict => ansi(Ansi::Red),
            Self::Warning => ansi(Ansi::Yellow),
            Self::Button => ansi(Ansi::BrightWhite),
            Self::ChangeId | Self::Bookmark => ansi(Ansi::Magenta),
        }
    }

    /// The role this one falls back to for a colour it says nothing
    /// about itself. Almost every role falls back to [Role::Default],
    /// what is set for the app as a whole; one that is a kind of
    /// another falls back to that one first, so that colouring the one
    /// colours both.
    fn parent(self) -> Option<Self> {
        match self {
            Self::Default => None,
            Self::ButtonActive => Some(Self::Highlight),
            _ => Some(Self::Default),
        }
    }

    /// Whether the role takes `channel` from nothing but itself, so that
    /// what is set for the app as a whole leaves it alone: a default
    /// foreground on the highlight would flatten jj's colouring of the
    /// selected row.
    fn keeps_off(self, channel: Channel) -> bool {
        matches!((self, channel), (Self::Highlight, Channel::Fg))
    }
}

/// The foreground and background of a role, either of which may be left
/// for something else to say.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoleColors {
    pub fg: Option<ThemeColor>,
    pub bg: Option<ThemeColor>,
}

/// Which of the two colours of a role is being asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Fg,
    Bg,
}

impl Channel {
    fn of(self, colors: RoleColors) -> Option<ThemeColor> {
        match self {
            Self::Fg => colors.fg,
            Self::Bg => colors.bg,
        }
    }
}

/// A role is written either as the colour to draw it in, which is its
/// foreground, or as a table saying either of its colours.
impl<'de> Deserialize<'de> for RoleColors {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let read = |value: Option<toml::Value>| match value {
            None => Ok(None),
            Some(value) => ThemeColor::deserialize(value)
                .map(Some)
                .map_err(de::Error::custom),
        };

        match toml::Value::deserialize(deserializer)? {
            toml::Value::String(text) => Ok(Self {
                fg: Some(text.parse().map_err(de::Error::custom)?),
                bg: None,
            }),
            toml::Value::Table(mut table) => {
                if let Some(key) = table
                    .keys()
                    .find(|key| !matches!(key.as_str(), "fg" | "bg"))
                {
                    return Err(de::Error::custom(format!(
                        "a color says {key:?} about nothing; it says \"fg\" and \"bg\""
                    )));
                }

                Ok(Self {
                    fg: read(table.remove("fg"))?,
                    bg: read(table.remove("bg"))?,
                })
            }
            _ => Err(de::Error::custom(
                "a color is written as a name or a code, or as a table saying \"fg\" and \"bg\"",
            )),
        }
    }
}

/// What the configuration says to draw each role in. A role it says
/// nothing about is left out rather than held as saying nothing, so that
/// what a colour scheme says can be told apart from what the user does.
#[derive(Debug, Clone, Default)]
pub struct Colors(HashMap<Role, RoleColors>);

impl Colors {
    /// What the configuration says about `role`, which may be nothing.
    fn role(&self, role: Role) -> RoleColors {
        self.0.get(&role).copied().unwrap_or_default()
    }
}

/// The roles are named rather than numbered, and one that names no role
/// is refused with the ones there are: a misspelt role is a colour that
/// would otherwise go quietly unset.
impl<'de> Deserialize<'de> for Colors {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let table = toml::Table::deserialize(deserializer)?;
        let mut colors = HashMap::new();

        for (key, value) in table {
            let Some(role) = Role::ALL.into_iter().find(|role| role.key() == key) else {
                return Err(de::Error::custom(format!(
                    "{key:?} is no element to color; they are {}",
                    Role::ALL.map(Role::key).join(", ")
                )));
            };
            colors.insert(
                role,
                RoleColors::deserialize(value).map_err(de::Error::custom)?,
            );
        }

        Ok(Self(colors))
    }
}

/// The colours to draw the roles in.
#[derive(Debug, Clone)]
pub struct Theme {
    colors: Colors,
    /// What each role is drawn in, by [Role::index].
    styles: [Style; Role::ALL.len()],
}

impl Theme {
    fn new(colors: Colors) -> Self {
        let mut theme = Self {
            colors,
            styles: [Style::new(); Role::ALL.len()],
        };

        for role in Role::ALL {
            let style = theme.worked_out(role);

            theme.styles[role.index()] = style;
        }

        theme
    }

    /// What `role` is drawn in. Channels the role says nothing about are
    /// left for whatever is underneath to fill in.
    pub fn style(&self, role: Role) -> Style {
        self.styles[role.index()]
    }

    fn worked_out(&self, role: Role) -> Style {
        let mut style = Style::new();
        if let Some(fg) = self.color(role, Channel::Fg) {
            style = style.fg(fg);
        }
        if let Some(bg) = self.color(role, Channel::Bg) {
            style = style.bg(bg);
        }

        style
    }

    /// What the configuration draws `role` in, as far as it says: what it
    /// says about the role, else what the role is drawn in without being
    /// said anything about, else what it says about the app as a whole.
    pub fn color_of(&self, role: Role, channel: Channel) -> Option<ThemeColor> {
        let said = |this: Role| {
            channel
                .of(self.colors.role(this))
                .or_else(|| channel.of(this.builtin()))
        };

        if let Some(color) = said(role) {
            return Some(color);
        }
        // The cut-off is the asked-for role's own: a role that is only a
        // kind of one that keeps a channel off still takes what is set
        // for the app.
        if role.keeps_off(channel) {
            return None;
        }

        // Each role in turn, from the one it falls back to and from
        // there to what is set for the app.
        let mut at = role.parent();
        while let Some(this) = at {
            if let Some(color) = said(this) {
                return Some(color);
            }

            at = this.parent();
        }

        None
    }

    fn color(&self, role: Role, channel: Channel) -> Option<Color> {
        self.color_of(role, channel).map(ThemeColor::to_ratatui)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new(Colors::default())
    }
}

impl JjConfig {
    /// The colours the configuration draws the app in.
    pub fn theme(&self) -> Theme {
        Theme::new(self.colors().clone())
    }
}

/// The colours the app is drawing in.
///
/// Unlike [`crate::env::get_env()`], this works before the environment
/// is set, as in tests building components.
pub fn theme() -> &'static Theme {
    static WITHOUT_CONFIG: LazyLock<Theme> = LazyLock::new(Theme::default);

    configured_theme().unwrap_or(&WITHOUT_CONFIG)
}

impl Role {
    /// What the role is drawn in, for a caller with no configuration of
    /// its own to go by.
    pub fn style(self) -> Style {
        theme().style(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The theme the configuration `config` makes.
    fn theme_of(config: &str) -> Theme {
        toml::from_str::<JjConfig>(config)
            .expect("the configuration parses")
            .theme()
    }

    /// What a colour is refused for, for a configuration that does not
    /// read.
    fn refusal(config: &str) -> String {
        toml::from_str::<JjConfig>(config)
            .expect_err("the configuration does not parse")
            .message()
            .to_owned()
    }

    /// The list of the roles is what the configuration and the colours
    /// tab are built out of, so a role missing from it is one nothing
    /// can be said about. The list being what there is to iterate, only
    /// counting them catches one that was left out of it.
    #[test]
    fn the_list_of_roles_is_every_one_of_them_once() {
        // A role left out of the list leaves a gap in the places, and
        // one listed twice takes a place from another.
        let mut places: Vec<usize> = Role::ALL.into_iter().map(Role::index).collect();
        places.sort_unstable();

        assert_eq!(places, (0..Role::ALL.len()).collect::<Vec<_>>());
    }

    /// Every role is a key of the configuration, and one that is not is
    /// one nothing can be said about.
    #[test]
    fn every_role_is_a_key_of_the_configuration() {
        for role in Role::ALL {
            let theme = theme_of(&format!(
                "blazingjj.colors.{} = {{ fg = \"#010203\" }}\n",
                role.key()
            ));

            assert_eq!(
                theme.color_of(role, Channel::Fg),
                Some(ThemeColor::Rgb(1, 2, 3)),
                "{}",
                role.key()
            );
        }
    }

    /// The app looks as it always has while the configuration says
    /// nothing about the colours.
    #[test]
    fn without_a_word_the_roles_are_drawn_as_they_were() {
        let theme = Theme::default();

        assert_eq!(theme.style(Role::Hint), Style::new().fg(Color::Gray));
        assert_eq!(theme.style(Role::Error), Style::new().fg(Color::Red));
        assert_eq!(theme.style(Role::Button), Style::new().fg(Color::White));
        assert_eq!(theme.style(Role::Border), Style::new());
        assert_eq!(
            theme.style(Role::Highlight),
            Style::new().bg(Color::Rgb(50, 50, 150))
        );
        assert_eq!(theme.style(Role::Default), Style::new());
    }

    /// A role is written either as the colour to draw it in or as a
    /// table saying either of its colours.
    #[test]
    fn a_role_takes_a_colour_of_its_own_or_a_table_of_the_two() {
        let bare = theme_of("blazingjj.colors.hint = \"#010203\"\n");
        assert_eq!(
            bare.color_of(Role::Hint, Channel::Fg),
            Some(ThemeColor::Rgb(1, 2, 3))
        );

        let table = theme_of("blazingjj.colors.hint = { fg = \"red\", bg = \"blue\" }\n");
        assert_eq!(
            table.color_of(Role::Hint, Channel::Fg),
            Some(ThemeColor::Ansi(Ansi::Red))
        );
        assert_eq!(
            table.color_of(Role::Hint, Channel::Bg),
            Some(ThemeColor::Ansi(Ansi::Blue))
        );
    }

    /// What is set for the app as a whole is what a role that says
    /// nothing itself is drawn in, so that a background is set once
    /// rather than twenty-three times.
    #[test]
    fn a_role_falls_back_to_what_is_set_for_the_app() {
        let theme = theme_of("blazingjj.colors.default = { fg = \"#010203\", bg = \"#040506\" }\n");

        // The hint has a foreground of its own and no background, so it
        // takes only the background.
        assert_eq!(
            theme.style(Role::Hint),
            Style::new().fg(Color::Gray).bg(Color::Rgb(4, 5, 6))
        );
        assert_eq!(
            theme.style(Role::Default),
            Style::new().fg(Color::Rgb(1, 2, 3)).bg(Color::Rgb(4, 5, 6))
        );

        // A panel's border has no colour of its own either way, so it
        // takes both.
        assert_eq!(theme.style(Role::Border), theme.style(Role::Default));
    }

    /// The highlight is a background put over a row of jj's own output,
    /// so a foreground set for the app as a whole does not reach it:
    /// the change ids and bookmarks of the selected row keep the colours
    /// jj gave them rather than flattening to the one.
    #[test]
    fn the_highlight_takes_no_foreground_from_what_is_set_for_the_app() {
        let for_the_app = theme_of("blazingjj.colors.default = { fg = \"red\", bg = \"blue\" }\n");
        assert_eq!(for_the_app.style(Role::Highlight).fg, None);

        // What is asked for outright is still what it is drawn in.
        let asked = theme_of("blazingjj.colors.highlight.fg = \"red\"\n");
        assert_eq!(asked.style(Role::Highlight).fg, Some(Color::Red));

        // The button Enter presses is a kind of highlight, but it is a
        // label of ours rather than a row of jj's, so the foreground
        // reaches it as it reaches every other button.
        assert_eq!(for_the_app.style(Role::ButtonActive).fg, Some(Color::Red));
    }

    /// A role that is a kind of another takes what that one is drawn in,
    /// so that colouring the one colours both: the button Enter presses
    /// is marked the way the selected row is, without the highlight
    /// having to be named twice.
    #[test]
    fn a_role_falls_back_to_the_role_it_is_a_kind_of() {
        assert_eq!(
            Theme::default().style(Role::ButtonActive),
            Theme::default().style(Role::Highlight)
        );

        // Including where the highlight is what was set, rather than
        // only where it is what the app comes with.
        let highlighted = theme_of("blazingjj.colors.highlight.bg = \"#010203\"\n");
        assert_eq!(
            highlighted.style(Role::ButtonActive).bg,
            Some(Color::Rgb(1, 2, 3))
        );

        // What is said about the button itself still beats it.
        let both = theme_of(
            "blazingjj.colors.highlight.bg = \"#010203\"\n\
             blazingjj.colors.button-active.bg = \"#040506\"\n",
        );
        assert_eq!(both.style(Role::ButtonActive).bg, Some(Color::Rgb(4, 5, 6)));
    }

    /// The background of the app is the terminal's while nothing is said
    /// about it, so that a terminal's own background shows through.
    #[test]
    fn nothing_said_about_the_background_leaves_the_terminals_own() {
        let theme = Theme::default();

        assert_eq!(theme.style(Role::Hint).bg, None);
        assert_eq!(theme.style(Role::Default).bg, None);
    }

    /// A colour that names nothing is refused where it is written, with
    /// what one looks like, rather than left for the app to make what it
    /// can of.
    #[test]
    fn a_colour_that_names_nothing_is_refused() {
        assert!(refusal("blazingjj.colors.hint = \"chartreuse\"\n").contains("#rrggbb"));
        assert!(refusal("blazingjj.colors.hint.fg = \"chartreuse\"\n").contains("#rrggbb"));
    }

    /// A role says a foreground and a background and nothing else, so
    /// that a misspelt one is not taken for a colour that was set.
    #[test]
    fn a_role_is_refused_what_says_nothing_about_it() {
        let refusal = refusal("blazingjj.colors.hint = { foreground = \"red\" }\n");

        assert!(refusal.contains("foreground"), "{refusal}");
    }

    #[test]
    fn a_role_that_is_neither_a_colour_nor_a_table_is_refused() {
        assert!(refusal("blazingjj.colors.hint = 5\n").contains("name or a code"));
    }

    #[test]
    fn a_role_is_drawn_in_the_colour_it_is_given() {
        let theme = theme_of("");

        assert_eq!(theme.style(Role::Error).fg, Some(Color::Red));
        assert_eq!(theme.style(Role::Warning).fg, Some(Color::Yellow));
    }

    /// A role that says nothing about a channel leaves it for whatever
    /// is underneath, which is what lets jj's own colouring survive
    /// being highlighted.
    #[test]
    fn a_role_says_nothing_about_a_colour_it_does_not_draw_in() {
        let theme = theme_of("");

        assert_eq!(theme.style(Role::Highlight).fg, None);
        assert_eq!(theme.style(Role::Error).bg, None);
        assert_eq!(theme.style(Role::PanelTitle), Style::new());
    }

    #[test]
    fn the_highlight_is_drawn_on_what_the_configuration_says() {
        let theme = theme_of("blazingjj.colors.highlight.bg = \"green\"\n");

        assert_eq!(theme.style(Role::Highlight).bg, Some(Color::Green));
    }

    /// The button Enter presses is a kind of highlight rather than a
    /// colour of its own, so configuring the one colours both.
    #[test]
    fn the_button_enter_presses_is_drawn_as_the_highlight_is() {
        let theme = theme_of("blazingjj.colors.highlight.bg = \"green\"\n");

        assert_eq!(
            theme.style(Role::ButtonActive),
            theme.style(Role::Highlight)
        );
    }
}
