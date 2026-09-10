/*! Handing our colours to jj, so that what it writes into our panels is
drawn in the same palette as the frame around it.

jj settings name a colour of the palette rather than giving one, so what
we hand back is jj's own settings with every name replaced by what the
palette makes of it: which label is which colour stays jj's to say, and
only what the colour looks like is ours. Nothing jj says is left out,
whatever the user set it to.

Where a scheme says a role outright, that is it saying the palette draws
that role's colour poorly, and jj's labels naming the same colour are
drawn no better by it. So we take what the scheme says about a role as
what it says about the colour the role would otherwise have taken, and
hand jj that.
*/

use std::str::FromStr;

use crate::theme::Attribute;
use crate::theme::Channel;
use crate::theme::Role;
use crate::theme::Scheme;
use crate::theme::Theme;
use crate::theme::ThemeColor;

/// What the scheme draws a colour of the palette as, for the colours it
/// draws a role in rather than leaving to the palette. A role it says
/// nothing about, or one drawn in no colour of the palette to begin
/// with, says nothing about any colour here.
///
/// Several roles are drawn in the one colour, and a scheme that redraws
/// them differently says nothing about it that we could hand jj: which
/// of the two a label of jj's wanted is not ours to guess, so the colour
/// is left to the palette as it would have been.
///
/// A role naming a label of jj's is left out. It is handed to jj as the
/// label it names, so what a scheme says about it is about that label
/// and nothing else: the diff's header being drawn in something other
/// than the palette's yellow is no reason for the rest of jj's yellow to
/// follow it.
fn said_outright(scheme: &Scheme) -> [Option<ThemeColor>; 16] {
    /// What the roles drawn in one colour of the palette say it should
    /// be: the one thing they agree on, or more than one thing.
    enum Said {
        One(ThemeColor),
        Several,
    }

    let mut said: [Option<Said>; 16] = [const { None }; 16];

    for role in Role::ALL {
        if role.jj_labels().is_empty()
            && let Some(ThemeColor::Ansi(ansi)) = role.builtin().fg
            && let Some(color) = scheme.role(role).fg
        {
            let at = &mut said[ansi.index()];

            *at = Some(match at {
                None => Said::One(color),
                Some(Said::One(one)) if *one == color => Said::One(color),
                Some(_) => Said::Several,
            });
        }
    }

    said.map(|said| match said {
        Some(Said::One(color)) => Some(color),
        Some(Said::Several) | None => None,
    })
}

/// What jj is to be told, given `colors` as it reads them now: the same
/// settings with every colour of the palette replaced by what the theme
/// makes of it. Nothing where there is nothing to say.
pub fn config_text(theme: &Theme, colors: &toml::Table) -> Option<String> {
    // Without a scheme there is no palette to put jj's colours through,
    // so all we have to say is what the roles naming its output say.
    let mut told = match theme.scheme() {
        Some(scheme) => recolored(scheme, colors),
        None => toml::Table::new(),
    };
    for role in Role::ALL {
        said_about(theme, role, colors, &mut told);
    }
    if told.is_empty() {
        return None;
    }

    let mut config = toml::Table::new();
    config.insert("colors".to_owned(), toml::Value::Table(told));

    toml::to_string(&config).ok()
}

/// jj's own `colors` with every colour of the palette replaced by what
/// the scheme makes of it.
fn recolored(scheme: &Scheme, colors: &toml::Table) -> toml::Table {
    let said_outright = said_outright(scheme);
    let recolor = |color: &toml::Value| {
        let text = color.as_str()?;

        match ThemeColor::from_str(text) {
            Ok(ThemeColor::Ansi(ansi)) => {
                let color =
                    said_outright[ansi.index()].unwrap_or_else(|| scheme.palette().of(ansi));

                Some(toml::Value::String(color.to_string()))
            }
            // A colour that is already one of its own, and anything we
            // do not read as a colour, is jj's to keep saying.
            _ => None,
        }
    };

    colors
        .iter()
        .map(|(label, setting)| {
            let setting = match setting {
                // A label is either the colour to draw it in outright,
                // or a table of that colour and how it is drawn.
                color @ toml::Value::String(_) => recolor(color).unwrap_or_else(|| color.clone()),
                toml::Value::Table(style) => toml::Value::Table(
                    style
                        .iter()
                        .map(|(key, value)| {
                            let value = match key.as_str() {
                                "fg" | "bg" => recolor(value).unwrap_or_else(|| value.clone()),
                                _ => value.clone(),
                            };

                            (key.clone(), value)
                        })
                        .collect(),
                ),
                other => other.clone(),
            };

            (label.clone(), setting)
        })
        .collect()
}

/// Write what `role` is said to be drawn in and how onto the labels jj
/// draws it as, leaving what it says nothing about as `jj` has it. jj
/// reads the attributes under the same names we do.
fn said_about(theme: &Theme, role: Role, jj: &toml::Table, colors: &mut toml::Table) {
    let colors_said = Channel::ALL.into_iter().filter_map(|channel| {
        let color = theme.said_of(role, channel)?;

        Some((channel.key(), toml::Value::String(color.to_string())))
    });
    let attributes_said = Attribute::ALL.into_iter().filter_map(|attribute| {
        let asked = theme.attribute_said_of(role, attribute)?;

        Some((attribute.key(), toml::Value::Boolean(asked)))
    });

    let said: Vec<(&str, toml::Value)> = colors_said.chain(attributes_said).collect();
    if said.is_empty() {
        return;
    }

    for label in role.jj_labels() {
        // A label jj draws in a colour outright is written afresh as a
        // table, there being no other way to give it a background.
        let style = match colors.remove(*label).or_else(|| jj.get(*label).cloned()) {
            Some(toml::Value::Table(style)) => style,
            Some(color @ toml::Value::String(_)) => {
                toml::Table::from_iter([("fg".to_owned(), color)])
            }
            _ => toml::Table::new(),
        };

        let style = style.into_iter().chain(
            said.iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone())),
        );

        colors.insert((*label).to_owned(), toml::Value::Table(style.collect()));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;
    use crate::commander::get_output_args;
    use crate::env::JjConfig;
    use crate::theme::Ansi;
    use crate::theme::Scheme;

    /// jj's colours as it lists them, in the shapes it writes: a colour
    /// outright, a table of one and how it is drawn, and a label whose
    /// name has a space in it.
    fn colors() -> toml::Table {
        r#"
change_id = "magenta"
rest = "bright black"
"diff added".fg = "green"
"diff added".bold = true
error.fg = "default"
error.bold = true
"working_copy commit_id" = "bright blue"
prefix.bold = true
"#
        .parse()
        .expect("the listing parses")
    }

    /// A `jj` reading no configuration of the user's, whose colours are
    /// no business of whoever runs the tests. `directory` is where the
    /// nothing it reads instead is written.
    fn jj(directory: &Path) -> Command {
        let empty = directory.join("empty.toml");
        fs::write(&empty, "").expect("the file is written");

        let mut command = Command::new("jj");
        command.env("JJ_CONFIG", empty).arg("--ignore-working-copy");
        command
    }

    /// The colours jj lists, its own defaults among them.
    fn jj_colors(directory: &Path) -> toml::Table {
        let listed = jj(directory)
            .args(["config", "list", "--include-defaults", "colors"])
            .args(get_output_args(false, true))
            .output()
            .expect("jj runs");
        let listed: toml::Table =
            toml::from_slice(&listed.stdout).expect("what jj lists is what it takes");

        listed["colors"]
            .as_table()
            .expect("the colours are a table")
            .clone()
    }

    fn config(scheme: &str) -> Option<String> {
        let theme = toml::from_str::<JjConfig>(scheme)
            .expect("the configuration parses")
            .theme();

        config_text(&theme, &colors())
    }

    /// Every colour jj names is given as the palette draws it, so that
    /// what jj writes into a panel matches the frame around it.
    #[test]
    fn the_palette_is_what_jj_is_told_to_draw_in() {
        let config = config("blazingjj.styles.scheme = \"tokyo-night\"\n")
            .expect("the scheme is one to hand over");

        // Tokyo Night draws magenta as #bb9af7.
        assert!(config.contains("change_id = \"#bb9af7\""), "{config}");
    }

    /// A scheme that says what a role is drawn in is saying the palette
    /// draws that colour poorly, and jj's labels naming it are drawn no
    /// better by it: the part of a change id jj writes in `bright black`
    /// is the background itself in Solarized Dark.
    #[test]
    fn what_a_scheme_says_about_a_role_is_what_jj_is_told_about_its_colour() {
        for (scheme, hint) in [
            ("solarized-dark", "#586e75"),
            ("solarized-light", "#93a1a1"),
        ] {
            let config = config(&format!("blazingjj.styles.scheme = \"{scheme}\"\n"))
                .expect("the scheme is one to hand over");

            assert!(config.contains(&format!("rest = \"{hint}\"")), "{config}");
        }
    }

    /// A label is drawn as jj draws it: we say what its colour looks
    /// like and leave the rest of what it says alone.
    #[test]
    fn how_a_label_is_drawn_is_left_to_jj() {
        let config = config("blazingjj.styles.scheme = \"solarized-dark\"\n")
            .expect("the scheme is one to hand over");
        let config: toml::Table = config.parse().expect("what we write reads back");
        let colors = config["colors"]
            .as_table()
            .expect("the colours are a table");

        let added = colors["diff added"].as_table().expect("a table");
        assert_eq!(added["fg"].as_str(), Some("#859900"));
        assert_eq!(added["bold"].as_bool(), Some(true));

        // Nothing is said about a label that is only ever drawn bold.
        assert_eq!(colors["prefix"].as_table().expect("a table").len(), 1);
    }

    /// jj reads the attributes under the same names we do, so what a
    /// role naming its output is asked to be drawn with is handed over
    /// beside its colours.
    #[test]
    fn the_attributes_of_a_role_jj_draws_are_handed_over_too() {
        let config = config("blazingjj.styles.change-id = { italic = true, bold = false }\n")
            .expect("the attributes are something to hand over");
        let config: toml::Table = config.parse().expect("what we write reads back");
        let colors = config["colors"]
            .as_table()
            .expect("the colours are a table");

        let change_id = colors["change_id"].as_table().expect("a table");
        assert_eq!(change_id["italic"].as_bool(), Some(true));
        assert_eq!(change_id["bold"].as_bool(), Some(false));
        // The colour jj draws it in is left alone, nothing being said
        // about it here.
        assert_eq!(change_id["fg"].as_str(), Some("magenta"));
    }

    /// A label a role names that jj knows nothing about under that name
    /// is a colour that would go quietly undrawn, so the names are worth
    /// checking against the labels jj lists.
    #[test]
    fn every_role_names_labels_jj_knows() {
        let directory = TempDir::with_prefix("blazingjj").expect("a directory to write in");
        let listed = jj_colors(directory.path());

        for role in Role::ALL {
            for label in role.jj_labels() {
                assert!(
                    listed.contains_key(*label)
                        // A label jj sets nothing for of its own is one
                        // it draws all the same, as long as it says
                        // something about the label it is a kind of.
                        || label
                            .rsplit_once(' ')
                            .is_some_and(|(kind, _)| listed.contains_key(kind)),
                    "{} names {label:?}, which jj does not",
                    role.key()
                );
            }
        }
    }

    /// A role naming a label of jj's says nothing about the colour of
    /// the palette it would otherwise have taken: Catppuccin draws the
    /// diff's header in blue where jj draws it in the palette's yellow,
    /// which is no reason for the rest of jj's yellow to turn blue.
    #[test]
    fn what_a_role_of_jjs_own_is_redrawn_in_stays_its_own() {
        let colors = "\"diff header\" = \"yellow\"\nconflict_description = \"yellow\"\n"
            .parse::<toml::Table>()
            .expect("the listing parses");
        let theme = toml::from_str::<JjConfig>("blazingjj.styles.scheme = \"catppuccin-mocha\"\n")
            .expect("the configuration parses")
            .theme();
        let config: toml::Table = config_text(&theme, &colors)
            .expect("the scheme is one to hand over")
            .parse()
            .expect("what we write reads back");
        let colors = config["colors"]
            .as_table()
            .expect("the colours are a table");

        let header = colors["diff header"].as_table().expect("a table");
        assert_eq!(header["fg"].as_str(), Some("#89b4fa"), "{colors:?}");
        // Mocha's yellow, rather than the blue the header took.
        assert_eq!(
            colors["conflict_description"].as_str(),
            Some("#f9e2af"),
            "{colors:?}"
        );
    }

    /// A scheme tinting the diff's added lines is giving them a
    /// background jj gives them none of, and the green jj writes them in
    /// and the boldness stay jj's.
    #[test]
    fn a_background_a_scheme_gives_a_label_is_added_to_what_jj_says() {
        let config = config("blazingjj.styles.scheme = \"catppuccin-mocha\"\n")
            .expect("the scheme is one to hand over");
        let config: toml::Table = config.parse().expect("what we write reads back");
        let colors = config["colors"]
            .as_table()
            .expect("the colours are a table");

        let added = colors["diff added"].as_table().expect("a table");
        assert_eq!(added["bg"].as_str(), Some("#394545"));
        assert_eq!(added["fg"].as_str(), Some("#a6e3a1"));
        assert_eq!(added["bold"].as_bool(), Some(true));
    }

    /// The terminal's own is not one of the sixteen, so it stays what it
    /// was rather than being given a colour it never had.
    #[test]
    fn the_terminals_own_colour_stays_the_terminals() {
        let config = config("blazingjj.styles.scheme = \"tokyo-night\"\n")
            .expect("the scheme is one to hand over");
        let config: toml::Table = config.parse().expect("what we write reads back");
        let colors = config["colors"]
            .as_table()
            .expect("the colours are a table");

        assert_eq!(
            colors["error"].as_table().expect("a table")["fg"].as_str(),
            Some("default")
        );
    }

    /// A popup's border and what just worked are both drawn in green,
    /// so a scheme that redraws them differently says nothing about
    /// that colour that jj could be handed: a label of jj's naming it
    /// wanted neither in particular, and it is left to the palette.
    #[test]
    fn a_colour_two_roles_disagree_about_is_left_to_the_palette() {
        let scheme = |colors: &str| {
            let palette: String = Ansi::ALL
                .iter()
                .map(|color| format!("{:?} = \"#010203\"\n", color.name()))
                .collect();

            toml::from_str::<Scheme>(&format!(
                "[palette]\nfg = \"#000000\"\nbg = \"#ffffff\"\n{palette}[colors]\n{colors}"
            ))
            .expect("the scheme reads")
        };

        let agreed = scheme("popup-border = \"#040506\"\nsuccess = \"#040506\"\n");
        assert_eq!(
            said_outright(&agreed)[Ansi::Green.index()],
            Some(ThemeColor::Rgb(4, 5, 6))
        );

        let disagreed = scheme("popup-border = \"#040506\"\nsuccess = \"#070809\"\n");
        assert_eq!(said_outright(&disagreed)[Ansi::Green.index()], None);
    }

    /// A label whose name has a space in it is written so that it reads
    /// back as the label it was, rather than as a table.
    #[test]
    fn a_label_of_several_words_reads_back_as_one_label() {
        let config = config("blazingjj.styles.scheme = \"tokyo-night\"\n")
            .expect("the scheme is one to hand over");
        let config: toml::Table = config.parse().expect("what we write reads back");
        let colors = config["colors"]
            .as_table()
            .expect("the colours are a table");

        assert_eq!(
            colors["working_copy commit_id"].as_str(),
            Some("#8db0ff"),
            "{config:?}"
        );
    }

    /// Without a scheme the app draws in the terminal's own colours, and
    /// so does jj: there is nothing to hand over.
    #[test]
    fn without_a_scheme_jj_is_told_nothing() {
        assert!(config("").is_none());
    }

    /// A change id and a bookmark are jj's output rather than ours, so
    /// what is said about the role reaches the screen only by being said
    /// to jj. It is worth saying without a scheme too, that being the
    /// only way the colour is drawn at all.
    #[test]
    fn what_is_said_about_jjs_own_output_is_handed_to_jj() {
        let config = config("blazingjj.styles.change-id = { fg = \"cyan\", bg = \"#202030\" }\n")
            .expect("there is something to hand over");
        let config: toml::Table = config.parse().expect("what we write reads back");
        let colors = config["colors"]
            .as_table()
            .expect("the colours are a table");

        for label in ["change_id", "working_copy change_id"] {
            let style = colors[label].as_table().expect("a table");

            assert_eq!(style["fg"].as_str(), Some("cyan"), "{label}");
            assert_eq!(style["bg"].as_str(), Some("#202030"), "{label}");
        }
        // Nothing else is said, jj's own colours being its to keep while
        // no scheme replaces them.
        assert_eq!(colors.len(), 2, "{colors:?}");
    }

    /// A channel the role says nothing about is jj's to keep: it draws
    /// the working copy's own change id brighter to tell it apart, which
    /// a background of ours is no reason to lose.
    #[test]
    fn a_channel_the_role_leaves_alone_stays_as_jj_draws_it() {
        let config = config("blazingjj.styles.change-id.bg = \"#202030\"\n")
            .expect("there is something to hand over");
        let config: toml::Table = config.parse().expect("what we write reads back");
        let colors = config["colors"]
            .as_table()
            .expect("the colours are a table");

        let style = colors["change_id"].as_table().expect("a table");
        assert_eq!(style["fg"].as_str(), Some("magenta"));
        assert_eq!(style["bg"].as_str(), Some("#202030"));
    }

    /// What we hand jj is what jj takes: every label it knows, every
    /// colour written the way it writes one. jj refuses a config file it
    /// cannot read outright, so getting this wrong would leave the app
    /// unable to run a single command.
    #[test]
    fn what_we_hand_jj_is_what_jj_takes() {
        let directory = TempDir::with_prefix("blazingjj").expect("a directory to write in");
        let colors = &jj_colors(directory.path());

        for scheme in Scheme::NAMES.map(|name| Scheme::named(name).expect("the scheme reads")) {
            let theme = toml::from_str::<JjConfig>(&format!(
                "blazingjj.styles.scheme = \"{}\"\nblazingjj.styles.apply-to-jj = true\n",
                scheme.name
            ))
            .expect("the configuration parses")
            .theme();

            let path = directory.path().join(format!("{}.toml", scheme.name));
            fs::write(
                &path,
                theme.jj_config(colors).expect("the scheme is handed over"),
            )
            .expect("the file is written");

            let read = jj(directory.path())
                .arg("--config-file")
                .arg(&path)
                .args(["config", "list", "colors"])
                .args(get_output_args(false, true))
                .output()
                .expect("jj runs");

            assert!(
                read.status.success(),
                "{} is not what jj takes: {}",
                scheme.name,
                String::from_utf8_lossy(&read.stderr)
            );
        }
    }
}
