/*! A colour, as the configuration writes one.

Neither of the two that have to read it takes what the other does.
ratatui takes `reset`, a bare `0`-`255` and `#rrggbb`, and reads `white`
as the bright one; jj takes `default`, `ansi-color-<0-255>` and
`#rrggbb`, and reads `white` as the dim one. So we read a colour
ourselves and hand each of them what it takes.

Where the two disagree on a name we follow jj: the configuration lives in
jj's config file beside jj's own `colors`, and it is jj that the app is
made to look like.
*/

use std::fmt;
use std::str::FromStr;

use ratatui::style::Color;
use serde::Deserialize;
use serde::Deserializer;
use serde::de;

/// One of the sixteen colours a terminal has a palette of. Kept as which
/// one rather than as what it looks like, so that a colour scheme can
/// say what it looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ansi {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl Ansi {
    /// The name the colour is written under, which is the name jj knows
    /// it by.
    pub fn name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::Red => "red",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Blue => "blue",
            Self::Magenta => "magenta",
            Self::Cyan => "cyan",
            Self::White => "white",
            Self::BrightBlack => "bright black",
            Self::BrightRed => "bright red",
            Self::BrightGreen => "bright green",
            Self::BrightYellow => "bright yellow",
            Self::BrightBlue => "bright blue",
            Self::BrightMagenta => "bright magenta",
            Self::BrightCyan => "bright cyan",
            Self::BrightWhite => "bright white",
        }
    }

    /// What ratatui calls the colour. Its `White` is the bright one and
    /// its `Gray` the dim one, which is the other way round from the
    /// names.
    fn to_ratatui(self) -> Color {
        match self {
            Self::Black => Color::Black,
            Self::Red => Color::Red,
            Self::Green => Color::Green,
            Self::Yellow => Color::Yellow,
            Self::Blue => Color::Blue,
            Self::Magenta => Color::Magenta,
            Self::Cyan => Color::Cyan,
            Self::White => Color::Gray,
            Self::BrightBlack => Color::DarkGray,
            Self::BrightRed => Color::LightRed,
            Self::BrightGreen => Color::LightGreen,
            Self::BrightYellow => Color::LightYellow,
            Self::BrightBlue => Color::LightBlue,
            Self::BrightMagenta => Color::LightMagenta,
            Self::BrightCyan => Color::LightCyan,
            Self::BrightWhite => Color::White,
        }
    }

    /// The colour `name` stands for, taking the spellings ratatui and
    /// the terminals have for them besides the ones jj has.
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "black" => Self::Black,
            "red" => Self::Red,
            "green" => Self::Green,
            "yellow" => Self::Yellow,
            "blue" => Self::Blue,
            "magenta" | "purple" => Self::Magenta,
            "cyan" => Self::Cyan,
            "white" | "gray" | "grey" | "silver" => Self::White,
            "brightblack" | "lightblack" | "darkgray" | "darkgrey" => Self::BrightBlack,
            "brightred" | "lightred" => Self::BrightRed,
            "brightgreen" | "lightgreen" => Self::BrightGreen,
            "brightyellow" | "lightyellow" => Self::BrightYellow,
            "brightblue" | "lightblue" => Self::BrightBlue,
            "brightmagenta" | "lightmagenta" | "brightpurple" => Self::BrightMagenta,
            "brightcyan" | "lightcyan" => Self::BrightCyan,
            "brightwhite" | "lightwhite" | "brightgray" | "brightgrey" | "lightgray"
            | "lightgrey" => Self::BrightWhite,
            _ => return None,
        })
    }
}

/// A colour an element is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeColor {
    /// Whatever the terminal draws in of its own accord.
    Terminal,
    /// One of the sixteen the terminal has a palette of.
    Ansi(Ansi),
    /// One of the 256 a terminal that has that many has.
    Indexed(u8),
    /// A colour of its own, for a terminal that can draw one.
    Rgb(u8, u8, u8),
}

impl ThemeColor {
    /// What ratatui draws the colour as.
    pub fn to_ratatui(self) -> Color {
        match self {
            Self::Terminal => Color::Reset,
            Self::Ansi(ansi) => ansi.to_ratatui(),
            Self::Indexed(index) => Color::Indexed(index),
            Self::Rgb(r, g, b) => Color::Rgb(r, g, b),
        }
    }
}

/// How the colour is written, which is how jj takes it and how we write
/// it back to the configuration.
impl fmt::Display for ThemeColor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Terminal => write!(f, "default"),
            Self::Ansi(ansi) => write!(f, "{}", ansi.name()),
            Self::Indexed(index) => write!(f, "ansi-color-{index}"),
            Self::Rgb(r, g, b) => write!(f, "#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

impl FromStr for ThemeColor {
    type Err = ParseColorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if let Some(rgb) = parse_hex(text) {
            return Ok(rgb);
        }

        let lower = text.to_lowercase();

        // Both the bare number ratatui takes and the `ansi-color-N` jj
        // takes name a colour of the 256. The digits are read as they
        // are written, so that the spacing a name is taken with does not
        // make a number out of `1_2_8`.
        let digits = lower.strip_prefix("ansi-color-").unwrap_or(&lower);
        if digits.bytes().all(|byte| byte.is_ascii_digit())
            && let Ok(index) = digits.parse::<u8>()
        {
            return Ok(Self::Indexed(index));
        }

        // A name is taken however it is spaced and capitalised, so that
        // `bright black`, `bright-black` and `BrightBlack` are the one
        // colour.
        let name = lower.replace([' ', '-', '_'], "");

        if let "default" | "none" | "terminal" | "reset" = name.as_str() {
            return Ok(Self::Terminal);
        }
        if let Some(ansi) = Ansi::parse(&name) {
            return Ok(Self::Ansi(ansi));
        }

        Err(ParseColorError(text.to_owned()))
    }
}

/// The colour `#rrggbb` stands for, and nothing for anything else.
fn parse_hex(text: &str) -> Option<ThemeColor> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();

    Some(ThemeColor::Rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// What was written where a colour was wanted.
#[derive(Debug)]
pub struct ParseColorError(String);

impl fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:?} is none of a color name, a #rrggbb code, ansi-color-0 to ansi-color-255, or \"default\"",
            self.0
        )
    }
}

impl std::error::Error for ParseColorError {}

impl<'de> Deserialize<'de> for ThemeColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;

        text.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sixteen, in the order the palette has them.
    const ANSI: [Ansi; 16] = [
        Ansi::Black,
        Ansi::Red,
        Ansi::Green,
        Ansi::Yellow,
        Ansi::Blue,
        Ansi::Magenta,
        Ansi::Cyan,
        Ansi::White,
        Ansi::BrightBlack,
        Ansi::BrightRed,
        Ansi::BrightGreen,
        Ansi::BrightYellow,
        Ansi::BrightBlue,
        Ansi::BrightMagenta,
        Ansi::BrightCyan,
        Ansi::BrightWhite,
    ];

    fn parse(text: &str) -> ThemeColor {
        text.parse().expect("the colour is one that can be written")
    }

    #[test]
    fn a_colour_is_taken_however_it_is_spaced_and_capitalised() {
        let bright_black = ThemeColor::Ansi(Ansi::BrightBlack);

        assert_eq!(parse("bright black"), bright_black);
        assert_eq!(parse("bright-black"), bright_black);
        assert_eq!(parse("BrightBlack"), bright_black);
        assert_eq!(parse("bright_black"), bright_black);
    }

    /// ratatui and the terminals spell some of the sixteen differently
    /// from jj, and what is written is whichever the user reaches for.
    #[test]
    fn the_spellings_the_other_two_have_name_the_same_colours() {
        assert_eq!(parse("dark gray"), parse("bright black"));
        assert_eq!(parse("light red"), parse("bright red"));
        assert_eq!(parse("grey"), parse("white"));
    }

    /// ratatui reads `white` as the bright one and `gray` as the dim
    /// one. We follow jj, so `white` is the dim one and the bright one
    /// has to be asked for by name.
    #[test]
    fn white_is_the_dim_one_as_jj_has_it() {
        assert_eq!(parse("white").to_ratatui(), Color::Gray);
        assert_eq!(parse("bright white").to_ratatui(), Color::White);
    }

    /// A colour of the 256 is written as a bare number by ratatui and as
    /// `ansi-color-N` by jj, and either is taken.
    #[test]
    fn a_colour_of_the_256_is_taken_in_both_spellings() {
        assert_eq!(parse("208"), ThemeColor::Indexed(208));
        assert_eq!(parse("ansi-color-208"), ThemeColor::Indexed(208));
        assert_eq!(parse("0"), ThemeColor::Indexed(0));
    }

    #[test]
    fn the_terminals_own_is_taken_in_both_spellings() {
        assert_eq!(parse("default"), ThemeColor::Terminal);
        assert_eq!(parse("reset"), ThemeColor::Terminal);
        assert_eq!(parse("none"), ThemeColor::Terminal);
    }

    #[test]
    fn a_code_is_read_as_the_colour_it_stands_for() {
        assert_eq!(parse("#0a141e"), ThemeColor::Rgb(10, 20, 30));
        assert_eq!(parse("#FFFFFF"), ThemeColor::Rgb(255, 255, 255));
    }

    /// What a colour is written as is what jj takes, so that handing it
    /// our colours is writing them out.
    #[test]
    fn a_colour_is_written_as_jj_takes_it() {
        assert_eq!(parse("BrightBlack").to_string(), "bright black");
        assert_eq!(parse("208").to_string(), "ansi-color-208");
        assert_eq!(parse("#0A141E").to_string(), "#0a141e");
        assert_eq!(parse("none").to_string(), "default");
    }

    /// However a colour was written, writing it out and reading it back
    /// is the same colour, which is what lets the tab offer what the
    /// configuration says for changing.
    #[test]
    fn what_a_colour_is_written_as_reads_back_as_the_same_colour() {
        for color in ANSI.map(ThemeColor::Ansi).into_iter().chain([
            ThemeColor::Terminal,
            ThemeColor::Indexed(208),
            ThemeColor::Rgb(1, 2, 3),
        ]) {
            assert_eq!(parse(&color.to_string()), color);
        }
    }

    #[test]
    fn what_names_no_colour_says_what_one_looks_like() {
        let err = "chartreuse"
            .parse::<ThemeColor>()
            .expect_err("the colour is not one that can be written");

        assert!(err.to_string().contains("#rrggbb"), "{err}");
        assert!(err.to_string().contains("chartreuse"), "{err}");
    }

    /// A code that is not six digits is no colour, rather than one made
    /// of what can be read of it.
    #[test]
    fn a_code_of_the_wrong_length_is_no_colour() {
        assert!("#fff".parse::<ThemeColor>().is_err());
        assert!("#0a141e0".parse::<ThemeColor>().is_err());
        assert!("#gggggg".parse::<ThemeColor>().is_err());
    }

    /// A number past the 256 is no colour, rather than one folded back
    /// into them.
    #[test]
    fn a_number_past_the_palette_is_no_colour() {
        assert!("256".parse::<ThemeColor>().is_err());
        assert!("ansi-color-256".parse::<ThemeColor>().is_err());
    }
}
