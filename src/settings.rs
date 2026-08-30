/*! The options the app can be configured with, as the settings tab
offers them.

Each option is a jj config key, so what the tab reads and writes is the
configuration jj already keeps. A value is passed around as the TOML
expression `jj config set` takes, which is also what the configuration
is checked against before anything is written.
*/

use anyhow::Result;

use crate::env::check_config_value;

/// What kind of value an option takes, which decides both how it is
/// asked for and how what is typed becomes a TOML expression.
pub enum SettingKind {
    /// One of a fixed set of values.
    Choice(&'static [&'static str]),
    /// Free text.
    Text,
    /// A number, which is written as one rather than as the text of one.
    Number,
    /// A command line, which is written as the list of the program and
    /// its arguments. Arguments are taken apart at whitespace, so one
    /// that holds any has to be written in the config file itself.
    CommandLine,
}

/// One option the settings tab shows.
pub struct Setting {
    /// The config key, as jj names it.
    pub key: &'static str,
    /// The heading the tab lists it under.
    pub section: &'static str,
    /// What the option does.
    pub doc: &'static str,
    /// What the app goes by while the option is not set.
    pub fallback: &'static str,
    pub kind: SettingKind,
}

impl Setting {
    /// The TOML expression `input` stands for, refused when the option
    /// cannot be read back from it.
    pub fn value_of(&self, input: &str) -> Result<String> {
        let value = match self.kind {
            SettingKind::Number => input.trim().to_owned(),
            SettingKind::Choice(_) | SettingKind::Text => {
                toml::Value::String(input.to_owned()).to_string()
            }
            SettingKind::CommandLine => toml::Value::Array(
                input
                    .split_whitespace()
                    .map(|word| toml::Value::String(word.to_owned()))
                    .collect(),
            )
            .to_string(),
        };
        check_config_value(self.key, &value)?;

        Ok(value)
    }

    /// How `value` reads on screen, and so also how it is offered for
    /// changing: a string as it says rather than as it is quoted, a
    /// command line as the command, anything else as TOML writes it.
    pub fn text_of(&self, value: &toml::Value) -> String {
        match (&self.kind, value) {
            (SettingKind::CommandLine, toml::Value::Array(words)) => words
                .iter()
                .map(|word| {
                    word.as_str()
                        .map_or_else(|| word.to_string(), str::to_owned)
                })
                .collect::<Vec<_>>()
                .join(" "),
            (_, toml::Value::String(text)) => text.clone(),
            _ => value.to_string(),
        }
    }

    /// The values the option can take, for an option that only takes one
    /// of a fixed set.
    pub fn choices(&self) -> Option<&'static [&'static str]> {
        match self.kind {
            SettingKind::Choice(choices) => Some(choices),
            _ => None,
        }
    }
}

/// Every option the settings tab shows, in the order it lists them. The
/// options of a section come one after another, that being how the tab
/// gathers them under it.
pub const SETTINGS: &[Setting] = &[
    Setting {
        key: "blazingjj.highlight-color",
        section: "Appearance",
        doc: "Background colour of the selected row, as a colour name or a #rrggbb code.",
        fallback: "#323296",
        kind: SettingKind::Text,
    },
    Setting {
        key: "blazingjj.layout",
        section: "Appearance",
        doc: "How a tab divides itself between its main and its details panel.",
        fallback: "horizontal",
        kind: SettingKind::Choice(&["horizontal", "vertical"]),
    },
    Setting {
        key: "blazingjj.layout-percent",
        section: "Appearance",
        doc: "The share of a tab the main panel takes, from 0 to 100.",
        fallback: "50",
        kind: SettingKind::Number,
    },
    Setting {
        key: "blazingjj.diff-format",
        section: "Diffs",
        doc: "How a diff is rendered. Without one, a configured diff pager or diff tool is used.",
        fallback: "ui.diff.format, else color-words",
        kind: SettingKind::Choice(&["color-words", "git", "pager", "summary", "stat"]),
    },
    Setting {
        key: "blazingjj.diff-pager",
        section: "Diffs",
        doc: "The command a diff in the pager format is piped through, like `delta --width=$width`. It reads a Git format diff and must not page itself; $width stands for the columns it renders into.",
        fallback: "no pager",
        kind: SettingKind::CommandLine,
    },
    Setting {
        key: "blazingjj.diff-tool",
        section: "Diffs",
        doc: "The program that renders a diff when the diff format is the diff tool.",
        fallback: "ui.diff.tool",
        kind: SettingKind::Text,
    },
    Setting {
        key: "blazingjj.describe-mode",
        section: "Changes",
        doc: "What describing a change puts up: the built-in editor, or `jj describe` with the terminal and your own editor.",
        fallback: "popup",
        kind: SettingKind::Choice(&["popup", "jj"]),
    },
    Setting {
        key: "blazingjj.bookmark-template",
        section: "Changes",
        doc: "Template for the name a generated bookmark is proposed under. Without one, 'push-' ++ change_id.short().",
        fallback: "templates.git_push_bookmark",
        kind: SettingKind::Text,
    },
    Setting {
        key: "blazingjj.poll-interval",
        section: "Watching the repo",
        doc: "Seconds between checks for work done outside the app, or 0 to only check when asked.",
        fallback: "1",
        kind: SettingKind::Number,
    },
];

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui::style::Color;

    use super::*;
    use crate::env::DescribeMode;
    use crate::env::DiffFormat;
    use crate::env::JJLayout;
    use crate::env::JjConfig;

    fn setting(key: &str) -> &'static Setting {
        SETTINGS
            .iter()
            .find(|setting| setting.key == key)
            .expect("the setting is one the app has")
    }

    /// The configuration as it reads with the option set to what the
    /// tab says the app goes by while it is not.
    fn as_fallback(key: &str) -> JjConfig {
        let setting = setting(key);
        let value = setting
            .value_of(setting.fallback)
            .expect("the fallback is a value the option takes");

        toml::from_str(&format!("{key} = {value}\n")).expect("the configuration parses")
    }

    /// The configuration as it reads with `key` set to `value`.
    fn set(key: &str, value: &str) -> JjConfig {
        toml::from_str(&format!("{key} = {value}\n")).expect("the configuration parses")
    }

    /// Every option is a key the app reads, and a key it does not read
    /// is one the tab would write to the user's config for nothing:
    /// what names no option is taken as saying nothing rather than
    /// refused.
    #[test]
    fn every_option_is_a_key_the_app_reads() {
        assert_eq!(
            set("blazingjj.highlight-color", "\"#010203\"").highlight_color(),
            Color::Rgb(1, 2, 3)
        );
        assert_eq!(
            set("blazingjj.diff-format", "\"git\"").diff_format(),
            DiffFormat::Git
        );
        assert_eq!(
            set("blazingjj.diff-tool", "\"difft\"").diff_tool(),
            Some(Some("difft".to_owned()))
        );
        assert!(
            set("blazingjj.diff-pager", "\"delta\"")
                .diff_pager()
                .is_some()
        );
        assert_eq!(
            set("blazingjj.bookmark-template", "\"name\"").bookmark_template(),
            "name"
        );
        assert_eq!(
            set("blazingjj.describe-mode", "\"jj\"").describe_mode(),
            DescribeMode::Jj
        );
        assert_eq!(
            set("blazingjj.layout", "\"vertical\"").layout(),
            JJLayout::Vertical
        );
        assert_eq!(set("blazingjj.layout-percent", "40").layout_percent(), 40);
        assert_eq!(
            set("blazingjj.poll-interval", "5").poll_interval(),
            Some(Duration::from_secs(5))
        );
    }

    /// What the tab says the app goes by without an option has to be
    /// what it does go by, for an option whose fallback is a value of
    /// its own rather than another key of the configuration.
    #[test]
    fn a_fallback_says_what_the_app_goes_by_without_the_option() {
        let default = JjConfig::default();

        assert_eq!(
            as_fallback("blazingjj.highlight-color").highlight_color(),
            default.highlight_color()
        );
        assert_eq!(
            as_fallback("blazingjj.describe-mode").describe_mode(),
            default.describe_mode()
        );
        assert_eq!(as_fallback("blazingjj.layout").layout(), default.layout());
        assert_eq!(
            as_fallback("blazingjj.layout-percent").layout_percent(),
            default.layout_percent()
        );
        assert_eq!(
            as_fallback("blazingjj.poll-interval").poll_interval(),
            default.poll_interval()
        );
    }

    #[test]
    fn text_is_quoted_into_a_value_of_its_own() {
        let color = setting("blazingjj.highlight-color");

        assert_eq!(color.value_of("#123456").unwrap(), "\"#123456\"");
        assert!(color.value_of("chartreuse").is_err());
    }

    #[test]
    fn a_number_is_written_as_one_rather_than_as_its_text() {
        let percent = setting("blazingjj.layout-percent");

        assert_eq!(percent.value_of(" 40 ").unwrap(), "40");
        assert!(percent.value_of("half").is_err());
    }

    /// A number the option cannot take is refused where it is typed,
    /// rather than written for the app to make what it can of.
    #[test]
    fn a_number_outside_what_the_option_takes_is_refused() {
        let percent = setting("blazingjj.layout-percent");

        assert!(percent.value_of("100").is_ok());
        assert!(percent.value_of("101").is_err());
    }

    #[test]
    fn a_command_line_is_written_as_the_program_and_its_arguments() {
        let pager = setting("blazingjj.diff-pager");

        assert_eq!(
            pager.value_of("delta --width=$width").unwrap(),
            r#"["delta", "--width=$width"]"#
        );
        assert!(pager.value_of("  ").is_err());
    }

    #[test]
    fn a_command_line_reads_back_as_the_command_it_was_typed_as() {
        let pager = setting("blazingjj.diff-pager");
        let value = toml::Value::Array(
            ["delta", "--width=$width"]
                .map(|word| toml::Value::String(word.to_owned()))
                .to_vec(),
        );

        assert_eq!(pager.text_of(&value), "delta --width=$width");
    }

    #[test]
    fn every_choice_an_option_offers_is_one_it_can_take() {
        for setting in SETTINGS {
            for choice in setting.choices().unwrap_or_default() {
                assert!(setting.value_of(choice).is_ok(), "{} {choice}", setting.key);
            }
        }
    }
}
