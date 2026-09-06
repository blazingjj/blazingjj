/*! The colours the app draws in.

An element is drawn in a [Role], which gives a [Style] so that it carries
a background as well as a foreground. Styles are patched onto what is
already there rather than set: a role that says nothing about a channel
keeps whatever was underneath, jj's own colouring included.
*/

use std::sync::LazyLock;

use ratatui::style::Color;
use ratatui::style::Style;

use crate::env::JjConfig;
use crate::env::jj_config;

/// What the app draws an element for. The colour of a role is the colour
/// of every element drawn in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// What is drawn in nothing more particular.
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

/// The colours to draw the roles in.
#[derive(Debug, Clone)]
pub struct Theme {
    highlight: Color,
}

impl Theme {
    /// What `role` is drawn in. Channels the role says nothing about are
    /// left for whatever is underneath to fill in.
    pub fn style(&self, role: Role) -> Style {
        match role {
            // A panel's border and title and a list's heading are drawn
            // in what they sit in, told apart by where they are or by
            // being bold rather than by a colour.
            Role::Default | Role::Border | Role::PanelTitle | Role::Heading => Style::new(),
            Role::Highlight | Role::ButtonActive => Style::new().bg(self.highlight),
            Role::Hint | Role::Separator => Style::new().fg(Color::DarkGray),
            Role::PopupBorder | Role::Success | Role::FileAdded => Style::new().fg(Color::Green),
            Role::PopupTitle
            | Role::Workspace
            | Role::FileModified
            | Role::FileRenamed
            | Role::FileCopied => Style::new().fg(Color::Cyan),
            Role::Error | Role::FileDeleted | Role::Conflict => Style::new().fg(Color::Red),
            Role::Warning => Style::new().fg(Color::Yellow),
            Role::Button => Style::new().fg(Color::White),
            Role::ChangeId | Role::Bookmark => Style::new().fg(Color::Magenta),
        }
    }
}

impl JjConfig {
    /// The colours the configuration draws the app in.
    pub fn theme(&self) -> Theme {
        Theme {
            highlight: self.highlight_color(),
        }
    }
}

/// The colours the app is drawing in, as the configuration has them.
///
/// Unlike [`crate::env::get_env()`], this works before the environment
/// is set, as in tests building components.
pub fn theme() -> Theme {
    static WITHOUT_CONFIG: LazyLock<Theme> = LazyLock::new(|| JjConfig::default().theme());

    jj_config().map_or_else(|| WITHOUT_CONFIG.clone(), JjConfig::theme)
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
        let theme = theme_of("[blazingjj]\nhighlight-color = \"green\"\n");

        assert_eq!(theme.style(Role::Highlight).bg, Some(Color::Green));
    }

    /// The button Enter presses is a kind of highlight rather than a
    /// colour of its own, so configuring the one colours both.
    #[test]
    fn the_button_enter_presses_is_drawn_as_the_highlight_is() {
        let theme = theme_of("[blazingjj]\nhighlight-color = \"green\"\n");

        assert_eq!(
            theme.style(Role::ButtonActive),
            theme.style(Role::Highlight)
        );
    }
}
