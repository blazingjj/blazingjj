/*! Handing our colours to delta, so that the diff it renders into our
panel is drawn in the same palette as the frame around it.

A syntax theme is the whole of what delta draws a diff in, the tints
under the changed lines and words as much as the highlighting of the
text, and it is a palette we have no way of handing over colour by
colour. So where delta has a theme named after the scheme we draw in,
that theme is what it draws the diff by: it is the scheme, as far as
delta is concerned, and a colour of the scheme's handed over beside it
would be one from another set. What the user sets for a role themselves
still reaches delta, that being theirs to see whatever draws the rest.

Where delta has no theme of ours, it highlights nothing at all, a diff
drawn in a palette of delta's own choosing sitting in the panel worse
than one drawn the way jj's is. What draws it then is the roles a diff
is drawn in, written out as style strings: the colour each comes to,
whether it was said outright or fallen back to, with `auto` standing in
for a background none of them gives so that delta tints the lines as it
would anyway.

What delta is configured with under git is left out of the run
altogether, that being how the user has it on the command line, where it
has the whole terminal rather than a panel of ours.

An attribute a role turns down cannot be handed over: a style string
says which attributes to draw with and has no way to say which to leave
off, so a role that refuses one is a role that says nothing about it
here.
*/

use crate::theme::Attribute;
use crate::theme::Channel;
use crate::theme::Role;
use crate::theme::Scheme;
use crate::theme::Theme;
use crate::theme::ThemeColor;

/// What delta draws the text of a line in where nothing of ours says
/// otherwise, which is the syntax highlighting it is run for.
const SYNTAX: &str = "syntax";

/// What delta picks for a colour we say nothing about.
const AUTO: &str = "auto";

/// The syntax theme that highlights nothing, which is what delta is run
/// with while it has none that draws in the colours the app does: what
/// it would highlight with instead is a palette of its own choosing, and
/// a diff drawn in that sits in our panel worse than one drawn in
/// nothing at all.
const NONE: &str = "none";

/// What delta draws in where the terminal's own colour is what is asked
/// for.
const NORMAL: &str = "normal";

/// What delta is run with to draw in our colours, `syntax_themes` being
/// the ones it has to highlight with.
pub fn args(theme: &Theme, syntax_themes: &[String]) -> Vec<String> {
    let syntax_theme = syntax_theme(theme, syntax_themes);
    // Reading no git config is what leaves delta with nothing but the
    // diff and what we tell it about drawing it.
    let mut args = vec![
        "--no-gitconfig".to_owned(),
        format!("--syntax-theme={syntax_theme}"),
    ];

    // Which of the two the terminal is decides the tints delta fills in
    // for itself, and the scheme we draw in is not something it can see.
    if let Some(scheme) = theme.scheme() {
        args.push(match is_dark(scheme) {
            true => "--dark".to_owned(),
            false => "--light".to_owned(),
        });
    }

    // A theme drawing the diff has been given what the scheme says
    // about it already, so only what the user set over the scheme is
    // still ours to hand over. Without one, nothing of ours has reached
    // delta and every part of the diff is drawn in what its role comes
    // to.
    let by_a_theme = syntax_theme != NONE;
    let said = match by_a_theme {
        true => Said::ByTheUser,
        false => Said::Outright,
    };

    // A line and the words within it that changed. Their text is the
    // theme's to highlight where one draws the diff, and is drawn in
    // what the role comes to where none does.
    for (role, flag) in [
        (Role::DiffAdded, "--plus-style"),
        (Role::DiffAddedWord, "--plus-emph-style"),
        (Role::DiffRemoved, "--minus-style"),
        (Role::DiffRemovedWord, "--minus-emph-style"),
    ] {
        let unsaid = match by_a_theme {
            true => SYNTAX.to_owned(),
            false => drawn_in(theme, role),
        };
        let style = style_of(theme, role, said, &unsaid)
            .or_else(|| (!by_a_theme).then(|| format!("{unsaid} {AUTO}")));

        if let Some(style) = style {
            args.push(format!("{flag}={style}"));
        }
    }

    // The headers name a file and a place in it rather than holding any
    // of its text, and left alone they come out in delta's blue, which
    // is the terminal's rather than the scheme's.
    if let Some((style, fg)) = header_of(theme, Role::DiffFileHeader, said, by_a_theme) {
        args.push(format!("--file-style={style}"));
        // The rule delta draws under the path is part of the header.
        args.push(format!("--file-decoration-style={fg} ul"));
    }

    if let Some((style, fg)) = header_of(theme, Role::DiffHunkHeader, said, by_a_theme) {
        // `line-number` is what puts the line a hunk starts at in the
        // header at all, delta reading that from the style string rather
        // than from a flag of its own.
        args.push(format!("--hunk-header-style=line-number {style}"));
        // delta draws the path and the line number in the header in
        // styles of their own, and the box around it in a third.
        args.push(format!("--hunk-header-file-style={fg}"));
        args.push(format!("--hunk-header-line-number-style={fg}"));
        args.push(format!("--hunk-header-decoration-style={fg} box"));
    }

    args
}

/// The style delta draws a header in and the colour of its text on its
/// own, for the flags that take the one or the other. Nothing where the
/// header is delta's to draw as its theme has it.
fn header_of(theme: &Theme, role: Role, said: Said, by_a_theme: bool) -> Option<(String, String)> {
    let unsaid = match by_a_theme {
        true => AUTO.to_owned(),
        false => drawn_in(theme, role),
    };
    let style = match style_of(theme, role, said, &unsaid) {
        Some(style) => style,
        None if by_a_theme => return None,
        None => unsaid.clone(),
    };
    let fg = said.color(theme, role, Channel::Fg).map_or(unsaid, written);

    Some((style, fg))
}

/// What delta highlights the diff with: the theme it has for the scheme
/// we draw in, and nothing at all where it has none, the colours it
/// would pick instead being neither ours nor the terminal's.
fn syntax_theme(theme: &Theme, syntax_themes: &[String]) -> String {
    let named = |name: &str| {
        syntax_themes
            .iter()
            .find(|theme| as_written(theme) == as_written(name))
            .cloned()
    };

    theme
        .scheme()
        .and_then(|scheme| named(scheme.name))
        .unwrap_or_else(|| NONE.to_owned())
}

/// `name` as a theme of delta's is compared with a scheme of ours:
/// delta writes a theme out as it is spelled rather than as a scheme is
/// named, so `Solarized (dark)` and `solarized-dark` are the one thing.
fn as_written(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_lowercase()
}

/// Which of what the theme says about a role is delta's to be told.
#[derive(Clone, Copy)]
enum Said {
    /// Everything said outright, the scheme's as much as the user's,
    /// there being nothing else delta could draw the diff by.
    Outright,
    /// Only what the user set beyond the scheme, the scheme itself
    /// having reached delta as the theme it draws the diff by.
    ByTheUser,
}

impl Said {
    fn color(self, theme: &Theme, role: Role, channel: Channel) -> Option<ThemeColor> {
        match self {
            Self::Outright => theme.said_of(role, channel),
            Self::ByTheUser => theme.set_of(role, channel),
        }
    }

    fn attribute(self, theme: &Theme, role: Role, attribute: Attribute) -> Option<bool> {
        match self {
            Self::Outright => theme.attribute_said_of(role, attribute),
            Self::ByTheUser => theme.attribute_set_of(role, attribute),
        }
    }
}

/// The style string delta draws `role` in, with `unsaid_fg` standing in
/// for a text colour the theme leaves out. Nothing where it says
/// nothing about the role at all.
fn style_of(theme: &Theme, role: Role, said: Said, unsaid_fg: &str) -> Option<String> {
    let color = |channel| said.color(theme, role, channel);
    let attributes: Vec<&str> = Attribute::ALL
        .into_iter()
        .filter(|attribute| said.attribute(theme, role, *attribute) == Some(true))
        .map(attribute_word)
        .collect();

    if color(Channel::Fg).is_none() && color(Channel::Bg).is_none() && attributes.is_empty() {
        return None;
    }

    // The first colour of a style string is the text and the second
    // what it sits on, so neither can be left out while the other is
    // given.
    let fg = color(Channel::Fg).map_or_else(|| unsaid_fg.to_owned(), written);
    let bg = color(Channel::Bg).map_or_else(|| AUTO.to_owned(), written);

    Some(
        [fg, bg]
            .into_iter()
            .chain(attributes.into_iter().map(str::to_owned))
            .collect::<Vec<String>>()
            .join(" "),
    )
}

/// What the app draws `role`'s text in, said outright or fallen back
/// to, for the colour to hand delta where delta's own would be the
/// terminal's rather than the scheme's.
fn drawn_in(theme: &Theme, role: Role) -> String {
    theme
        .color_of(role, Channel::Fg)
        .map_or_else(|| NORMAL.to_owned(), written)
}

/// `color` as delta takes it. It reads a name as the terminal spells
/// one and we read it as jj does, `white` being the bright one to delta
/// and the dim one to us, so one of the sixteen goes by its number
/// rather than by its name.
fn written(color: ThemeColor) -> String {
    match color {
        ThemeColor::Terminal => NORMAL.to_owned(),
        ThemeColor::Ansi(ansi) => ansi.index().to_string(),
        ThemeColor::Indexed(index) => index.to_string(),
        ThemeColor::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
    }
}

/// What delta calls the attribute.
fn attribute_word(attribute: Attribute) -> &'static str {
    match attribute {
        Attribute::Bold => "bold",
        Attribute::Dim => "dim",
        Attribute::Italic => "italic",
        Attribute::Underline => "ul",
    }
}

/// Whether the scheme is a dark one, which is what it draws the app on
/// rather than anything it says of itself. A background that is not a
/// colour of its own is one we cannot weigh, and is taken for the dark
/// one delta assumes without being told.
fn is_dark(scheme: &Scheme) -> bool {
    let Some(ThemeColor::Rgb(red, green, blue)) = scheme.palette().default_colors().bg else {
        return true;
    };
    let luminance =
        (2126 * u32::from(red) + 7152 * u32::from(green) + 722 * u32::from(blue)) / 10_000;

    luminance < 128
}

#[cfg(test)]
mod tests {
    use crate::env::JjConfig;

    /// The theme delta has for the schemes it knows here, which is the
    /// one the tests that look at its highlighting draw in.
    const THEMES: [&str; 2] = ["Catppuccin Mocha", "Solarized (dark)"];

    /// What delta is run with, it having no theme for any scheme of
    /// ours and so nothing to highlight with.
    fn args_of(config: &str) -> Vec<String> {
        args_with(config, &[])
    }

    /// What delta is run with, `syntax_themes` being the ones it has.
    fn args_with(config: &str, syntax_themes: &[&str]) -> Vec<String> {
        let syntax_themes: Vec<String> =
            syntax_themes.iter().map(|&name| name.to_owned()).collect();

        toml::from_str::<JjConfig>(config)
            .expect("the configuration parses")
            .theme()
            .delta_args(&syntax_themes)
    }

    /// A scheme delta has a theme for is the whole of what it is told
    /// about drawing the diff: the theme's tints and highlighting are
    /// made to go together, and a colour of ours beside them would be
    /// one from another set.
    #[test]
    fn a_scheme_delta_has_a_theme_for_is_all_it_is_told() {
        let args = args_with("blazingjj.styles.scheme = \"catppuccin-mocha\"\n", &THEMES);

        assert_eq!(
            args,
            [
                "--no-gitconfig",
                "--syntax-theme=Catppuccin Mocha",
                "--dark",
            ]
        );
    }

    /// What the user sets for a role themselves is theirs to see
    /// whatever draws the rest, so it is handed over on top of the
    /// theme. What the scheme says is not: the theme is the scheme, as
    /// far as delta is concerned.
    #[test]
    fn what_the_user_sets_over_the_scheme_is_handed_over_beside_the_theme() {
        let args = args_with(
            "blazingjj.styles.scheme = \"catppuccin-mocha\"\nblazingjj.styles.diff-added = { bg = \"#112233\" }\n",
            &THEMES,
        );

        assert_eq!(
            args,
            [
                "--no-gitconfig",
                "--syntax-theme=Catppuccin Mocha",
                "--dark",
                "--plus-style=syntax #112233",
            ]
        );
    }

    /// A role of our own is drawn in what it says where delta has no
    /// theme to draw the diff by.
    #[test]
    fn what_is_set_for_a_role_is_what_delta_draws_it_in() {
        let args = args_of(
            "blazingjj.styles.scheme = \"catppuccin-mocha\"\nblazingjj.styles.diff-added = { fg = \"#112233\", bold = true }\n",
        );

        assert!(
            args.contains(&"--plus-style=#112233 #394545 bold".to_owned()),
            "{args:?}"
        );
    }

    /// A scheme is matched to a theme however the two are spelled:
    /// delta writes a theme out as it is named rather than as we name a
    /// scheme.
    #[test]
    fn a_scheme_is_matched_to_the_theme_of_the_same_name() {
        let highlighted_with = |config: &str| {
            args_with(config, &THEMES)
                .into_iter()
                .find_map(|arg| Some(arg.strip_prefix("--syntax-theme=")?.to_owned()))
                .expect("delta is always told what to highlight with")
        };

        assert_eq!(
            highlighted_with("blazingjj.styles.scheme = \"catppuccin-mocha\"\n"),
            "Catppuccin Mocha"
        );
        assert_eq!(
            highlighted_with("blazingjj.styles.scheme = \"solarized-dark\"\n"),
            "Solarized (dark)"
        );
    }

    /// Where delta has no theme for the scheme it highlights nothing,
    /// and what it draws the lines in is then ours to say outright: the
    /// colours they fall back to are handed over along with the ones
    /// the scheme names, so the diff comes out as jj's own would.
    #[test]
    fn a_diff_delta_cannot_highlight_in_our_colours_is_left_unhighlighted() {
        let args = args_of("blazingjj.styles.scheme = \"tokyo-night\"\n");

        assert!(args.contains(&"--syntax-theme=none".to_owned()), "{args:?}");
        // Tokyo Night says nothing about the diff, so the lines come
        // out in the green and the red the roles fall back to.
        assert!(
            args.contains(&"--plus-style=#9ece6a auto".to_owned()),
            "{args:?}"
        );
        assert!(
            args.contains(&"--minus-style=#f7768e auto".to_owned()),
            "{args:?}"
        );
    }

    /// delta picks the tints it fills in for itself by which of the two
    /// the terminal is, and the scheme we draw in is not something it
    /// can see.
    #[test]
    fn delta_is_told_whether_the_scheme_is_a_dark_one() {
        let dark = args_of("blazingjj.styles.scheme = \"catppuccin-mocha\"\n");
        let light = args_of("blazingjj.styles.scheme = \"catppuccin-latte\"\n");

        assert!(dark.contains(&"--dark".to_owned()), "{dark:?}");
        assert!(light.contains(&"--light".to_owned()), "{light:?}");
    }

    /// The header of a hunk reaches the parts delta draws it out of,
    /// the box around it included, so that none of it is left in
    /// delta's own blue, which is the terminal's rather than ours.
    #[test]
    fn a_hunk_header_is_handed_over_in_every_part_delta_draws_it_in() {
        let args = args_of("blazingjj.styles.diff-hunk-header = \"#fab387\"\n");

        assert!(
            args.contains(&"--hunk-header-style=line-number #fab387 auto".to_owned()),
            "{args:?}"
        );
        assert!(
            args.contains(&"--hunk-header-file-style=#fab387".to_owned()),
            "{args:?}"
        );
        assert!(
            args.contains(&"--hunk-header-line-number-style=#fab387".to_owned()),
            "{args:?}"
        );
        assert!(
            args.contains(&"--hunk-header-decoration-style=#fab387 box".to_owned()),
            "{args:?}"
        );
    }

    /// A colour of the sixteen is handed over by number: delta reads a
    /// name as the terminal spells one, where `white` is the bright one
    /// and not the dim one we mean by it.
    #[test]
    fn a_colour_of_the_palette_goes_by_its_number() {
        let args = args_of("blazingjj.styles.diff-added = \"white\"\n");

        assert!(args.contains(&"--plus-style=7 auto".to_owned()), "{args:?}");
    }

    /// Told not to, we leave delta to what it is configured with even
    /// where the app has something to say.
    #[test]
    fn delta_is_left_alone_when_it_is_told_to_be() {
        assert!(
            args_of(
                "blazingjj.styles.scheme = \"catppuccin-mocha\"\nblazingjj.styles.apply-to-delta = false\n"
            )
            .is_empty()
        );
    }
}
