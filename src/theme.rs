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
mod jj;
mod scheme;

use std::collections::HashMap;
use std::iter;
use std::sync::LazyLock;

pub use color::Ansi;
pub use color::ThemeColor;
use ratatui::style::Color;
use ratatui::style::Style;
pub use scheme::Scheme;
use serde::Deserialize;
use serde::Deserializer;
use serde::de;

use crate::env::JjConfig;
use crate::env::configured_theme;

/// What the app draws an element for. The colour of a role is the colour
/// of every element drawn in it, and [Role::doc] is what each is drawn
/// for, said once for the colours tab to show and to read here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Default,
    Highlight,
    Hint,
    Separator,
    Border,
    Workspace,
    PanelTitle,
    Heading,
    PopupBorder,
    PopupTitle,
    Error,
    Warning,
    Success,
    Button,
    ButtonActive,
    ChangeId,
    Bookmark,
    FileAdded,
    FileModified,
    FileRenamed,
    FileCopied,
    FileDeleted,
    Conflict,
    Value,
}

impl Role {
    /// Every role there is, in the order the colours tab lists them.
    pub const ALL: [Self; 24] = [
        Self::Default,
        Self::Highlight,
        Self::Hint,
        Self::Value,
        Self::Separator,
        Self::Border,
        Self::Workspace,
        Self::PanelTitle,
        Self::Heading,
        Self::PopupBorder,
        Self::PopupTitle,
        Self::Button,
        Self::ButtonActive,
        Self::Error,
        Self::Warning,
        Self::Success,
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
            Self::Value => "value",
        }
    }

    /// What the role is drawn for, as the colours tab says it.
    pub fn doc(self) -> &'static str {
        match self {
            Self::Default => {
                "What everything else falls back to, and what is drawn in nothing more particular."
            }
            Self::Highlight => "The row, tab or button the keys act on.",
            Self::Hint => {
                "What is worth saying but not worth reading first: the keys a panel answers to, what an option falls back to, a row that cannot be picked."
            }
            Self::Separator => "The rules a popup divides itself with.",
            Self::Border => "The border around a panel of the frame.",
            Self::Workspace => "The workspace the app is running in, as the status bar names it.",
            Self::PanelTitle => "The title a panel of the frame is drawn under.",
            Self::Heading => "The heading a list is divided under.",
            Self::PopupBorder => "The border around a popup.",
            Self::PopupTitle => "The title of a popup.",
            Self::Error => "What went wrong.",
            Self::Warning => {
                "What is about to make a change rather than pick something already there."
            }
            Self::Success => "What just worked, for as long as it is worth showing.",
            Self::Button => "A button that is not the one Enter presses.",
            Self::ButtonActive => {
                "The button Enter presses. Takes the highlight's colors unless given its own."
            }
            Self::ChangeId => "A change id.",
            Self::Bookmark => "The name of a bookmark.",
            Self::FileAdded => "A file the change adds.",
            Self::FileModified => "A file the change changes.",
            Self::FileRenamed => "A file the change renames.",
            Self::FileCopied => "A file the change copies.",
            Self::FileDeleted => "A file the change deletes.",
            Self::Conflict => "A file left conflicted.",
            Self::Value => {
                "What an option or a binding is set to, where the settings and keybindings tabs list them."
            }
        }
    }

    /// The heading the colours tab lists the role under.
    pub fn section(self) -> &'static str {
        match self {
            Self::Default
            | Self::Highlight
            | Self::Hint
            | Self::Separator
            | Self::Border
            | Self::Workspace
            | Self::PanelTitle
            | Self::Heading
            | Self::Value => "The frame",
            Self::PopupBorder | Self::PopupTitle | Self::Button | Self::ButtonActive => "Popups",
            Self::Error | Self::Warning | Self::Success => "What the app has to say",
            Self::ChangeId | Self::Bookmark => "The repo",
            Self::FileAdded
            | Self::FileModified
            | Self::FileRenamed
            | Self::FileCopied
            | Self::FileDeleted
            | Self::Conflict => "Files",
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
            Self::Value => ansi(Ansi::Blue),
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

    /// The labels jj writes what the role names, for the roles that name
    /// something jj draws rather than something we do. The `working_copy`
    /// forms are the same thing on the working copy's row. A change id
    /// jj marks as divergent is left out: it is drawn in red to say so.
    pub fn jj_labels(self) -> &'static [&'static str] {
        match self {
            Self::ChangeId => &["change_id", "working_copy change_id"],
            Self::Bookmark => &[
                "bookmark",
                "bookmarks",
                "local_bookmarks",
                "remote_bookmarks",
                "working_copy bookmark",
                "working_copy bookmarks",
                "working_copy local_bookmarks",
                "working_copy remote_bookmarks",
            ],
            _ => &[],
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
    /// Both of them, in the order the colours tab asks for them.
    pub const ALL: [Self; 2] = [Self::Fg, Self::Bg];

    /// What the channel is called under a role's table.
    pub fn key(self) -> &'static str {
        match self {
            Self::Fg => "fg",
            Self::Bg => "bg",
        }
    }

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

/// What the configuration says about the colours: which scheme to draw
/// in, and what of it to draw differently. A role it says nothing about
/// is left out rather than held as saying nothing, so that what the
/// scheme says can be told apart from what the user does.
#[derive(Debug, Clone, Default)]
pub struct Colors {
    scheme: Option<&'static Scheme>,
    /// Whether jj is to be told to write in the scheme's colours too,
    /// for as long as anything is said about it either way.
    apply_to_jj: Option<bool>,
    roles: HashMap<Role, RoleColors>,
}

impl Colors {
    /// What the configuration says about `role`, which may be nothing.
    fn role(&self, role: Role) -> RoleColors {
        self.roles.get(&role).copied().unwrap_or_default()
    }
}

/// The scheme and whether it reaches jj are named alongside the roles
/// rather than under a table of their own, so they are taken out before
/// what is left is read as roles.
impl<'de> Deserialize<'de> for Colors {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut table = toml::Table::deserialize(deserializer)?;

        let scheme = match table.remove("scheme") {
            None => None,
            Some(value) => {
                let name = String::deserialize(value).map_err(de::Error::custom)?;

                Some(Scheme::named(&name).ok_or_else(|| {
                    de::Error::custom(format!(
                        "{name:?} is no color scheme; they are {}",
                        Scheme::NAMES.join(", ")
                    ))
                })?)
            }
        };

        let apply_to_jj = table
            .remove("apply-to-jj")
            .map(bool::deserialize)
            .transpose()
            .map_err(de::Error::custom)?;

        Ok(Self {
            scheme,
            apply_to_jj,
            roles: roles_from_table(table).map_err(de::Error::custom)?,
        })
    }
}

/// The roles `table` says something about, refusing a key that names no
/// role with the roles there are: a misspelt one is a colour that would
/// otherwise go quietly unset.
fn roles_from_table(table: toml::Table) -> Result<HashMap<Role, RoleColors>, String> {
    table
        .into_iter()
        .map(|(key, value)| {
            let role = Role::ALL
                .into_iter()
                .find(|role| role.key() == key)
                .ok_or_else(|| {
                    format!(
                        "{key:?} is no element to color; they are {}",
                        Role::ALL.map(Role::key).join(", ")
                    )
                })?;

            Ok((
                role,
                RoleColors::deserialize(value).map_err(|err| err.to_string())?,
            ))
        })
        .collect()
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

    /// What `role` is drawn in, as far as anything says: what the user
    /// says about the role, else what the scheme says about it, else
    /// what the role is drawn in without being said anything about,
    /// else what is set for the app as a whole.
    ///
    /// A colour naming one of the sixteen is what the scheme's palette
    /// makes of it, which is how a scheme recolours the roles neither it
    /// nor the user says anything about.
    pub fn color_of(&self, role: Role, channel: Channel) -> Option<ThemeColor> {
        let scheme = self.colors.scheme;
        let said = |this: Role| {
            channel
                .of(self.colors.role(this))
                .or_else(|| channel.of(scheme?.role(this)))
                .or_else(|| channel.of(this.builtin()))
        };

        // The role itself first, then what it falls back to and from
        // there to what is set for the app. A role that keeps the
        // channel off is left alone when it says nothing itself; one
        // that is only a kind of such a role still falls back.
        let color = match said(role) {
            Some(color) => color,
            None if role.keeps_off(channel) => return None,
            None => iter::successors(role.parent(), |this| this.parent())
                .find_map(said)
                .or_else(|| channel.of(scheme?.palette().default_colors()))?,
        };

        Some(self.through_palette(color))
    }

    /// What the configuration or the scheme draws `role`'s `channel` in
    /// where either says so outright, as the palette makes of it.
    ///
    /// What a role only falls back to is not something to hand jj: it is
    /// what the app draws in for want of anything said, and jj has its
    /// own answer for that already.
    pub fn said_of(&self, role: Role, channel: Channel) -> Option<ThemeColor> {
        let scheme = self.colors.scheme;
        let color = channel
            .of(self.colors.role(role))
            .or_else(|| channel.of(scheme?.role(role)))?;

        Some(self.through_palette(color))
    }

    /// `color` as the scheme's palette draws it, where it names one of
    /// the sixteen and a scheme is picked.
    fn through_palette(&self, color: ThemeColor) -> ThemeColor {
        match (color, self.colors.scheme) {
            (ThemeColor::Ansi(ansi), Some(scheme)) => scheme.palette().of(ansi),
            _ => color,
        }
    }

    fn color(&self, role: Role, channel: Channel) -> Option<Color> {
        self.color_of(role, channel).map(ThemeColor::to_ratatui)
    }

    /// The scheme the app is drawn in, if one is picked.
    pub fn scheme(&self) -> Option<&'static Scheme> {
        self.colors.scheme
    }

    /// Whether jj is to be told to write in the colours the app draws
    /// in: while a scheme is picked or a role naming jj's output is
    /// given a colour, unless told not to.
    pub fn applies_to_jj(&self) -> bool {
        let says_what_jj_draws = || {
            Role::ALL.into_iter().any(|role| {
                !role.jj_labels().is_empty()
                    && Channel::ALL
                        .into_iter()
                        .any(|channel| self.said_of(role, channel).is_some())
            })
        };

        self.asked_to_apply_to_jj() && (self.colors.scheme.is_some() || says_what_jj_draws())
    }

    /// Whether telling jj is turned on, whether or not there is anything
    /// to tell it as things stand.
    pub fn asked_to_apply_to_jj(&self) -> bool {
        self.colors.apply_to_jj.unwrap_or(true)
    }

    /// What jj is to be told to write in, given `colors` as jj reads
    /// them now.
    pub fn jj_config(&self, colors: &toml::Table) -> Option<String> {
        jj::config_text(self, colors)
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

    /// Picking a scheme recolours the roles it says nothing about,
    /// because what they name is one of the sixteen and the scheme is
    /// what says what those look like. This is the whole of what a
    /// scheme is for: the app is recoloured without a role being named.
    #[test]
    fn a_scheme_recolours_the_roles_it_says_nothing_about() {
        let theme = theme_of("blazingjj.colors.scheme = \"tokyo-night\"\n");

        // The error is `red`, which Tokyo Night draws as #f7768e, on the
        // background the scheme draws the app on.
        assert_eq!(
            theme.style(Role::Error),
            Style::new()
                .fg(Color::Rgb(0xf7, 0x76, 0x8e))
                .bg(Color::Rgb(0x1a, 0x1b, 0x26))
        );
    }

    /// A scheme says what the app is drawn in, which the highlight is
    /// not: it goes over a row of jj's own output, and a foreground of
    /// ours would flatten the change ids and bookmarks on that row to
    /// the one colour. Every scheme the app comes with says so.
    #[test]
    fn no_scheme_gives_the_highlight_a_foreground() {
        for scheme in Scheme::NAMES.map(|name| Scheme::named(name).expect("the scheme reads")) {
            let theme = theme_of(&format!("blazingjj.colors.scheme = \"{}\"\n", scheme.name));

            assert_eq!(theme.style(Role::Highlight).fg, None, "{}", scheme.name);
            assert!(theme.style(Role::Highlight).bg.is_some(), "{}", scheme.name);
        }
    }

    /// A scheme says what the app is drawn on, so that picking one is
    /// picking a background as well as the colours on it.
    #[test]
    fn a_scheme_says_what_the_app_is_drawn_on() {
        let theme = theme_of("blazingjj.colors.scheme = \"tokyo-night-storm\"\n");

        assert_eq!(
            theme.style(Role::Default),
            Style::new()
                .fg(Color::Rgb(0xc0, 0xca, 0xf5))
                .bg(Color::Rgb(0x24, 0x28, 0x3b))
        );
    }

    /// Solarized puts its dark background on `bright black`, which is
    /// what the separator would otherwise be drawn in. The scheme saying
    /// so outright is what keeps it visible.
    #[test]
    fn a_scheme_beats_what_a_role_is_drawn_in_without_one() {
        let theme = theme_of("blazingjj.colors.scheme = \"solarized-dark\"\n");

        assert_eq!(
            theme.style(Role::Separator),
            Style::new()
                .fg(Color::Rgb(0x58, 0x6e, 0x75))
                .bg(Color::Rgb(0x00, 0x2b, 0x36))
        );
    }

    /// What the user says beats the scheme, so that picking one is a
    /// place to start rather than the last word.
    #[test]
    fn what_is_set_beats_the_scheme() {
        let theme = theme_of(
            "blazingjj.colors.scheme = \"tokyo-night\"\nblazingjj.colors.error = \"#010203\"\n",
        );

        assert_eq!(theme.style(Role::Error).fg, Some(Color::Rgb(1, 2, 3)));
    }

    /// The log and the diffs are jj's output in our panels, so a scheme
    /// that stopped at the frame would be half applied. Picking one
    /// hands it to jj as well, without a second thing to find and set.
    #[test]
    fn a_scheme_is_handed_to_jj_without_being_asked_twice() {
        assert!(
            theme_of("blazingjj.colors.scheme = \"tokyo-night\"\n").applies_to_jj(),
            "a scheme is handed over"
        );
    }

    /// Nothing is said to jj without a scheme, so a configuration that
    /// never asked to be recoloured keeps the colours it set for itself.
    #[test]
    fn without_a_scheme_jj_is_left_as_it_was() {
        assert!(!Theme::default().applies_to_jj());
        assert!(!theme_of("blazingjj.colors.apply-to-jj = true\n").applies_to_jj());
    }

    /// Handing the colours over is still something to turn down, for
    /// whoever wants the app drawn in a scheme and jj left alone.
    #[test]
    fn handing_the_colours_to_jj_can_be_turned_down() {
        let theme = theme_of(
            "blazingjj.colors.scheme = \"tokyo-night\"\n\
             blazingjj.colors.apply-to-jj = false\n",
        );

        assert!(!theme.applies_to_jj());
    }

    /// A scheme that is not one of the app's is refused with the ones
    /// that are, rather than leaving the app looking unchanged for no
    /// stated reason.
    #[test]
    fn a_scheme_the_app_does_not_come_with_is_refused() {
        let refusal = refusal("blazingjj.colors.scheme = \"dracula\"\n");

        assert!(refusal.contains("dracula"), "{refusal}");
        assert!(refusal.contains("tokyo-night"), "{refusal}");
    }
}
