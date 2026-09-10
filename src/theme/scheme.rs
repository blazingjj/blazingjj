/*! A colour scheme: what the sixteen colours of the palette look like,
and what of the app is not drawn well in them.

The app draws almost everything in one of the sixteen, which is what
makes a palette enough to recolour the whole of it. jj does the same --
every one of its own colour settings names one of the sixteen -- so the
one palette is what we draw in and what we hand jj.

A scheme also says what a role is drawn in outright, for the few where
the palette's own answer is a poor one. The dim text is why: every
scheme here puts `bright black` close enough to its own background that
text drawn in it cannot be read, so each names its comment colour
instead.
*/

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;
use serde::Deserializer;
use serde::de;

use crate::theme::Ansi;
use crate::theme::Role;
use crate::theme::RoleStyle;
use crate::theme::ThemeColor;
use crate::theme::roles_from_table;

/// The schemes the app comes with, by name and by the file each is read
/// from, in the order they are offered.
const SOURCES: [(&str, &str); 10] = [
    (
        "catppuccin-mocha",
        include_str!("schemes/catppuccin-mocha.toml"),
    ),
    (
        "catppuccin-macchiato",
        include_str!("schemes/catppuccin-macchiato.toml"),
    ),
    (
        "catppuccin-frappe",
        include_str!("schemes/catppuccin-frappe.toml"),
    ),
    (
        "catppuccin-latte",
        include_str!("schemes/catppuccin-latte.toml"),
    ),
    (
        "solarized-dark",
        include_str!("schemes/solarized-dark.toml"),
    ),
    (
        "solarized-light",
        include_str!("schemes/solarized-light.toml"),
    ),
    ("tokyo-night", include_str!("schemes/tokyo-night.toml")),
    (
        "tokyo-night-storm",
        include_str!("schemes/tokyo-night-storm.toml"),
    ),
    (
        "tokyo-night-moon",
        include_str!("schemes/tokyo-night-moon.toml"),
    ),
    (
        "tokyo-night-day",
        include_str!("schemes/tokyo-night-day.toml"),
    ),
];

/// [SOURCES], read.
static BUILT_IN: LazyLock<Vec<Scheme>> = LazyLock::new(|| {
    SOURCES
        .into_iter()
        .map(|(name, source)| Scheme {
            name,
            ..toml::from_str(source).unwrap_or_else(|err| panic!("{name} reads: {err}"))
        })
        .collect()
});

/// What the sixteen colours of a terminal look like, and what the app is
/// drawn on and in where it says nothing more particular.
#[derive(Debug, Clone)]
pub struct Palette {
    fg: ThemeColor,
    bg: ThemeColor,
    ansi: [ThemeColor; 16],
}

impl Palette {
    /// What `ansi` looks like.
    pub fn of(&self, ansi: Ansi) -> ThemeColor {
        self.ansi[ansi.index()]
    }

    /// What the app is drawn in and on where nothing more particular is
    /// said.
    pub fn default_colors(&self) -> RoleStyle {
        RoleStyle {
            fg: Some(self.fg),
            bg: Some(self.bg),
            ..RoleStyle::default()
        }
    }
}

/// A set of colours to draw the app in, picked by name.
#[derive(Debug, Clone, Deserialize)]
pub struct Scheme {
    /// What the scheme is picked by. Taken from the file it is read
    /// from rather than from the file itself.
    #[serde(skip)]
    pub name: &'static str,
    palette: Palette,
    /// What the scheme says outright, for the roles the palette answers
    /// poorly for.
    #[serde(default, deserialize_with = "deserialize_roles")]
    colors: HashMap<Role, RoleStyle>,
}

impl Scheme {
    /// The names the schemes are picked by, in the order they are offered.
    pub const NAMES: [&'static str; SOURCES.len()] = {
        let mut names = [""; SOURCES.len()];
        let mut at = 0;
        while at < names.len() {
            names[at] = SOURCES[at].0;
            at += 1;
        }
        names
    };

    /// The scheme called `name`, of the ones the app comes with.
    pub fn named(name: &str) -> Option<&'static Self> {
        BUILT_IN.iter().find(|scheme| scheme.name == name)
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// What the scheme says about `role` outright, which for most roles
    /// is nothing: the palette is what answers for them.
    pub fn role(&self, role: Role) -> RoleStyle {
        self.colors.get(&role).copied().unwrap_or_default()
    }
}

/// The roles a scheme says something about outright.
fn deserialize_roles<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<Role, RoleStyle>, D::Error> {
    roles_from_table(toml::Table::deserialize(deserializer)?).map_err(de::Error::custom)
}

/// A palette says all sixteen and what the app is drawn in and on. One
/// that leaves any of them out is one the app could not be drawn in.
impl<'de> Deserialize<'de> for Palette {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let table = toml::Table::deserialize(deserializer)?;
        let mut said: [Option<ThemeColor>; 16] = [None; 16];
        let (mut fg, mut bg) = (None, None);

        // The sixteen are spelt as a palette spells them, so that
        // `bright-black` is the colour of that name rather than a key
        // saying nothing.
        for (key, value) in table {
            let color = ThemeColor::deserialize(value).map_err(de::Error::custom)?;

            match Ansi::named(&key) {
                Some(ansi) => {
                    // A colour goes by more than one name, so a palette
                    // can say the same one twice without repeating a
                    // key, which TOML would refuse. Which of the two it
                    // meant is not ours to pick.
                    if said[ansi.index()].replace(color).is_some() {
                        return Err(de::Error::custom(format!(
                            "the palette says {:?} more than once",
                            ansi.name()
                        )));
                    }
                }
                None if key == "fg" => fg = Some(color),
                None if key == "bg" => bg = Some(color),
                None => {
                    return Err(de::Error::custom(format!(
                        "the palette says {key:?} about nothing"
                    )));
                }
            }
        }

        let missing = |what: &str| de::Error::custom(format!("the palette leaves out {what:?}"));
        let mut ansi = [ThemeColor::Terminal; 16];
        for color in Ansi::ALL {
            ansi[color.index()] = said[color.index()].ok_or_else(|| missing(color.name()))?;
        }

        Ok(Self {
            fg: fg.ok_or_else(|| missing("fg"))?,
            bg: bg.ok_or_else(|| missing("bg"))?,
            ansi,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every scheme the app comes with reads, saying all sixteen of the
    /// palette and naming only roles there are: reading them is what
    /// refuses one that does not, and it panics where it is picked
    /// rather than where it is written. What is wrong with one is worth
    /// finding out here.
    #[test]
    fn every_built_in_scheme_reads() {
        for name in Scheme::NAMES {
            assert!(Scheme::named(name).is_some(), "{name} reads");
        }
    }

    /// A palette naming one of the sixteen under two of its spellings
    /// is turned down rather than drawn in whichever came last.
    #[test]
    fn a_palette_saying_a_colour_twice_is_refused() {
        let said =
            |palette: &str| toml::from_str::<Palette>(palette).map_err(|err| err.to_string());

        assert!(
            said("bright-black = \"#010203\"\ndarkgray = \"#040506\"\n")
                .is_err_and(|err| err.contains("more than once")),
        );
    }

    /// A scheme is picked by its name, and one that names no scheme is
    /// no scheme.
    #[test]
    fn a_scheme_is_found_by_the_name_it_is_picked_by() {
        for name in Scheme::NAMES {
            assert_eq!(Scheme::named(name).map(|scheme| scheme.name), Some(name));
        }

        assert!(Scheme::named("dracula").is_none());
    }

    /// A scheme draws in colours of its own rather than the terminal's,
    /// that being what picking one is for.
    #[test]
    fn a_palette_is_colours_of_its_own() {
        for scheme in Scheme::NAMES.map(|name| Scheme::named(name).expect("the scheme reads")) {
            let palette = scheme.palette();

            for color in Ansi::ALL {
                assert!(
                    matches!(palette.of(color), ThemeColor::Rgb(..)),
                    "{} draws {} in no colour of its own",
                    scheme.name,
                    color.name()
                );
            }
            assert!(matches!(
                palette.default_colors().fg,
                Some(ThemeColor::Rgb(..))
            ));
            assert!(matches!(
                palette.default_colors().bg,
                Some(ThemeColor::Rgb(..))
            ));
        }
    }

    #[test]
    fn a_palette_that_leaves_a_colour_out_is_refused() {
        let refusal = toml::from_str::<Scheme>("[palette]\nfg = \"#ffffff\"\n")
            .expect_err("the palette is not one to draw in")
            .to_string();

        assert!(refusal.contains("leaves out"), "{refusal}");
    }
}
