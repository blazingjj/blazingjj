/** The environment configures the application.

It is a combination of
- configuration files
- environment variables
- command line arguments
*/
use std::fmt;
use std::path::PathBuf;
use std::process::Command;
use std::ptr;
#[cfg(test)]
use std::sync::Once;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use ratatui::style::Color;
use serde::Deserialize;
use serde::Deserializer;
use serde::de;

use crate::commander::MIN_SETTABLE_WIDTH;
use crate::commander::RemoveEndLine;
use crate::commander::get_output_args;
use crate::keybinds::KeybindsConfig;

/// Singleton holding application environment.
///
/// The environment is read through a `&'static`, so putting another one
/// in place leaks the one that was there: whoever is reading it may
/// still be looking at it.
static ENV: AtomicPtr<Env> = AtomicPtr::new(ptr::null_mut());

/// Set application environment, in place of whatever was set before.
pub fn set_env(env: Env) {
    ENV.store(Box::into_raw(Box::new(env)), Ordering::Release);
}

/// Get application environment. Panics if not set first
pub fn get_env() -> &'static Env {
    env().expect("the environment is set before anything reads it")
}

/// The configured keybindings, if any. Unlike [`get_env()`], this works
/// before the environment is set, as in tests building components.
pub fn keybinds_config() -> Option<&'static KeybindsConfig> {
    env().and_then(|env| env.jj_config.keybinds())
}

/// Read the configuration again and put the environment it makes up in
/// place of the one the app has been running on.
pub fn reload_env() -> Result<()> {
    let env = get_env();
    let (config, jj_config) = read_jj_config(&env.root, &env.jj_bin)?;

    set_env(Env {
        config,
        jj_config,
        ..env.clone()
    });

    Ok(())
}

/// The environment, if one has been set.
fn env() -> Option<&'static Env> {
    // SAFETY: the pointer is either null or one leaked in `set_env`, and
    // nothing ever frees what it points at.
    unsafe { ENV.load(Ordering::Acquire).as_ref() }
}

/// A default environment for tests that build components reading it. The
/// tests share one process, so whichever gets there first sets it.
#[cfg(test)]
pub fn set_test_env() {
    static ONCE: Once = Once::new();

    ONCE.call_once(|| {
        set_env(Env {
            root: ".".to_owned(),
            config: toml::Table::new(),
            jj_config: JjConfig::default(),
            default_revset: None,
            jj_bin: "jj".to_owned(),
        })
    });
}

/// Whether the configuration would still be read the same way with `key`
/// set to `value`, which is a TOML expression as `jj config set` takes
/// one.
pub fn check_config_value(key: &str, value: &str) -> Result<()> {
    // Only what the value was refused for is worth reading; the line
    // and column of a document we wrote ourselves are not.
    toml::from_str::<JjConfig>(&format!("{key} = {value}\n"))
        .map_err(|err| anyhow!("{}", err.message()))
        .context("The setting cannot take that value")?;

    Ok(())
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct JjConfig {
    pub blazingjj: JjConfigBlazingjj,
    pub ui: JjConfigUi,
    pub templates: JjConfigTemplates,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case", default)]
pub struct JjConfigBlazingjj {
    highlight_color: Color,
    describe_mode: DescribeMode,
    diff_format: Option<ConfiguredDiffFormat>,
    diff_tool: Option<String>,
    diff_pager: Option<DiffPager>,
    bookmark_template: Option<String>,
    confirm_push: bool,
    layout: JJLayout,
    /// The share of a tab the main panel takes, of the whole of it at
    /// the most.
    #[serde(deserialize_with = "deserialize_layout_percent")]
    layout_percent: u16,
    keybinds: Option<KeybindsConfig>,
    /// How long to wait between checks for work done outside the app, or
    /// None to only check when one is asked for.
    #[serde(deserialize_with = "deserialize_poll_interval")]
    poll_interval: Option<Duration>,
}

impl Default for JjConfigBlazingjj {
    fn default() -> Self {
        Self {
            highlight_color: Color::Rgb(50, 50, 150),
            confirm_push: true,
            layout_percent: 50,
            poll_interval: Some(Duration::from_secs(1)),
            // Standard defaults for the rest
            describe_mode: DescribeMode::default(),
            diff_format: None,
            diff_tool: None,
            diff_pager: None,
            bookmark_template: None,
            layout: JJLayout::default(),
            keybinds: None,
        }
    }
}

/// Reads a share of a tab, which is refused above the whole of it: a
/// share the panels cannot be divided in is one to say something about
/// where it is set rather than one to make what we can of.
fn deserialize_layout_percent<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
    let percent = u16::deserialize(deserializer)?;
    if percent > 100 {
        return Err(de::Error::custom("a share of a tab is 100 percent at most"));
    }

    Ok(percent)
}

/// Reads a number of seconds, of which zero means never checking.
fn deserialize_poll_interval<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Duration>, D::Error> {
    let seconds = f64::deserialize(deserializer)?;
    let interval = Duration::try_from_secs_f64(seconds).map_err(de::Error::custom)?;

    Ok(Some(interval).filter(|interval| !interval.is_zero()))
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct JjConfigUi {
    diff: JjConfigUiDiff,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct JjConfigUiDiff {
    format: Option<ConfiguredDiffFormat>,
    tool: Option<toml::Value>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct JjConfigTemplates {
    git_push_bookmark: Option<String>,
}

impl JjConfig {
    pub fn diff_format(&self) -> DiffFormat {
        self.blazingjj
            .diff_format
            .or(self.ui.diff.format)
            .and_then(|configured| self.resolve_diff_format(configured))
            .or_else(|| self.diff_pager().map(DiffFormat::Pager))
            .or_else(|| self.diff_tool().map(DiffFormat::DiffTool))
            .unwrap_or(DiffFormat::ColorWords)
    }

    /// The format a configured one names, which for the pager format is
    /// nothing unless a pager is configured to render it.
    fn resolve_diff_format(&self, configured: ConfiguredDiffFormat) -> Option<DiffFormat> {
        Some(match configured {
            ConfiguredDiffFormat::ColorWords => DiffFormat::ColorWords,
            ConfiguredDiffFormat::Git => DiffFormat::Git,
            ConfiguredDiffFormat::Summary => DiffFormat::Summary,
            ConfiguredDiffFormat::Stat => DiffFormat::Stat,
            ConfiguredDiffFormat::Pager => DiffFormat::Pager(self.diff_pager()?),
        })
    }

    pub fn diff_pager(&self) -> Option<DiffPager> {
        self.blazingjj.diff_pager.clone()
    }

    pub fn diff_tool(&self) -> Option<Option<String>> {
        match self.blazingjj.diff_tool.clone() {
            tool @ Some(_) => Some(tool),
            _ if self.ui.diff.tool.is_some() => Some(None),
            _ => None,
        }
    }

    /// Whether a push is to be shown and asked about before it is sent.
    pub fn confirm_push(&self) -> bool {
        self.blazingjj.confirm_push
    }

    pub fn highlight_color(&self) -> Color {
        self.blazingjj.highlight_color
    }

    pub fn bookmark_template(&self) -> String {
        self.blazingjj
            .bookmark_template
            .clone()
            .or(self.templates.git_push_bookmark.clone())
            .unwrap_or("'push-' ++ change_id.short()".to_string())
    }

    pub fn describe_mode(&self) -> DescribeMode {
        self.blazingjj.describe_mode
    }

    pub fn layout(&self) -> JJLayout {
        self.blazingjj.layout
    }

    pub fn layout_percent(&self) -> u16 {
        self.blazingjj.layout_percent
    }

    pub fn keybinds(&self) -> Option<&KeybindsConfig> {
        self.blazingjj.keybinds.as_ref()
    }

    pub fn poll_interval(&self) -> Option<Duration> {
        self.blazingjj.poll_interval
    }
}

#[derive(Debug, Clone)]
pub struct Env {
    pub jj_config: JjConfig,
    /// The configuration as jj lists it, which is what an option is
    /// read out of by the key it is named by rather than by what the
    /// app makes of it.
    pub config: toml::Table,
    pub root: String,
    pub default_revset: Option<String>,
    pub jj_bin: String,
}

impl Env {
    pub fn new(path: PathBuf, default_revset: Option<String>, jj_bin: String) -> Result<Env> {
        // Get jj repository root
        let root_output = Command::new(&jj_bin)
            .arg("root")
            .args(get_output_args(false, true))
            .current_dir(&path)
            .output()?;
        if !root_output.status.success() {
            bail!("No jj repository found in {}", path.to_str().unwrap_or(""))
        }
        let root = String::from_utf8(root_output.stdout)?.remove_end_line();
        let (config, jj_config) = read_jj_config(&root, &jj_bin)?;

        Ok(Env {
            root,
            config,
            jj_config,
            default_revset,
            jj_bin,
        })
    }
}

/// What the configuration of the repo at `root` says, across all the
/// layers jj reads it from, as it is listed and as the app reads it.
fn read_jj_config(root: &str, jj_bin: &str) -> Result<(toml::Table, JjConfig)> {
    let cfg = Command::new(jj_bin)
        .arg("config")
        .arg("list")
        .args(get_output_args(false, true))
        .current_dir(root)
        .output()
        .context("Failed to get jj config")?
        .stdout;

    let config: toml::Table = toml::from_slice(&cfg).context("Failed to parse jj config")?;
    let jj_config = config
        .clone()
        .try_into()
        .context("Failed to read the jj config")?;

    Ok((config, jj_config))
}

#[derive(Clone, Copy, Debug, Deserialize, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum DescribeMode {
    #[default]
    Popup,
    Jj,
}

/// A diff format as `blazingjj.diff-format` and `ui.diff.format` name it.
/// What a name stands for depends on the rest of the configuration, so it
/// is resolved into a [DiffFormat] by
/// [diff_format](JjConfig::diff_format).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ConfiguredDiffFormat {
    ColorWords,
    Git,
    Pager,
    Summary,
    Stat,
}

/// What a pager argument says to have the render width substituted into it
const WIDTH_PLACEHOLDER: &str = "$width";

/// The command a diff in the [pager](DiffFormat::Pager) format is piped
/// through, like `delta`. It reads a Git format diff on standard input and
/// writes the rendering to standard output; `$width` in an argument stands
/// for the number of columns it has to render into.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(try_from = "ConfiguredDiffPager")]
pub struct DiffPager {
    program: String,
    args: Vec<String>,
}

/// A pager as it is configured: the program on its own, or a command line
/// of the program and its arguments.
#[derive(Deserialize)]
#[serde(untagged)]
enum ConfiguredDiffPager {
    Program(String),
    CommandLine(Vec<String>),
}

impl TryFrom<ConfiguredDiffPager> for DiffPager {
    type Error = &'static str;

    fn try_from(configured: ConfiguredDiffPager) -> Result<Self, Self::Error> {
        let (program, args) = match configured {
            ConfiguredDiffPager::Program(program) => (program, Vec::new()),
            ConfiguredDiffPager::CommandLine(mut command_line) => {
                if command_line.is_empty() {
                    return Err("a diff pager needs a program to run");
                }
                let args = command_line.split_off(1);
                (command_line.remove(0), args)
            }
        };

        Ok(Self { program, args })
    }
}

impl DiffPager {
    pub fn program(&self) -> &str {
        &self.program
    }

    /// The arguments to run the pager with, rendering into `width`
    /// columns. An argument asking for a width is left out when there is
    /// none to give, so the pager falls back to whatever it uses by
    /// default.
    pub fn args(&self, width: usize) -> Vec<String> {
        self.args
            .iter()
            .filter(|arg| width > 0 || !arg.contains(WIDTH_PLACEHOLDER))
            .map(|arg| arg.replace(WIDTH_PLACEHOLDER, &width.to_string()))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DiffFormat {
    ColorWords,
    Git,
    /// The Git format, rendered by an external pager
    Pager(DiffPager),
    DiffTool(Option<String>),
    // Configuration only, [DiffFormat::get_next] does not cycle through these
    Summary,
    Stat,
}

impl DiffFormat {
    /// The format the user gets by toggling this one, which skips whatever
    /// the configuration has no program for.
    pub fn get_next(&self, config: &JjConfig) -> DiffFormat {
        let pager = || config.diff_pager().map(DiffFormat::Pager);
        let diff_tool = || config.diff_tool().map(DiffFormat::DiffTool);

        match self {
            DiffFormat::ColorWords => Some(DiffFormat::Git),
            DiffFormat::Git => pager().or_else(diff_tool),
            DiffFormat::Pager(_) => diff_tool(),
            _ => None,
        }
        .unwrap_or(DiffFormat::ColorWords)
    }

    /// The pager the output of this format is piped through, if any.
    pub fn pager(&self) -> Option<&DiffPager> {
        match self {
            DiffFormat::Pager(pager) => Some(pager),
            _ => None,
        }
    }

    /// How wide output in this format is rendered, given the width of the
    /// panel showing it. The width reaches an external program through the
    /// COLUMNS environment variable, a pager through
    /// [its arguments](DiffPager::args) as well, and `--stat` scales its
    /// histogram to it; the other formats produce the same output whatever
    /// the width, and the panel wraps or scrolls it.
    ///
    /// A width too narrow to be passed on makes no difference either, so it
    /// comes out as no width at all rather than as a value of its own.
    pub fn render_width(&self, panel_width: usize) -> usize {
        match self {
            DiffFormat::DiffTool(_) | DiffFormat::Pager(_) | DiffFormat::Stat
                if panel_width >= MIN_SETTABLE_WIDTH =>
            {
                panel_width
            }
            _ => 0,
        }
    }
}

/// How the format is named in the UI, which for an external program is
/// that program, as it is what tells the formats it renders apart.
impl fmt::Display for DiffFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffFormat::ColorWords => write!(f, "color-words"),
            DiffFormat::Git => write!(f, "git"),
            DiffFormat::Pager(pager) => write!(f, "{}", pager.program()),
            DiffFormat::DiffTool(Some(tool)) => write!(f, "{tool}"),
            DiffFormat::DiffTool(None) => write!(f, "diff tool"),
            DiffFormat::Summary => write!(f, "summary"),
            DiffFormat::Stat => write!(f, "stat"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Default, Copy, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum JJLayout {
    #[default]
    Horizontal,
    Vertical,
}

impl JJLayout {
    pub fn toggle(self) -> Self {
        match self {
            JJLayout::Horizontal => JJLayout::Vertical,
            JJLayout::Vertical => JJLayout::Horizontal,
        }
    }
}

// Impl into for JJLayout to ratatui's Direction
impl From<JJLayout> for ratatui::layout::Direction {
    fn from(layout: JJLayout) -> Self {
        match layout {
            JJLayout::Horizontal => ratatui::layout::Direction::Horizontal,
            JJLayout::Vertical => ratatui::layout::Direction::Vertical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANEL_WIDTH: usize = 80;

    /// The configuration `jj config list` would print for the given
    /// settings
    fn config(settings: &str) -> JjConfig {
        toml::from_str(settings).expect("the settings are a valid configuration")
    }

    /// The pager the given `blazingjj.diff-pager` value configures
    fn pager(setting: &str) -> DiffPager {
        config(&format!("blazingjj.diff-pager = {setting}\n"))
            .diff_pager()
            .expect("the setting configures a pager")
    }

    #[test]
    fn only_formats_that_scale_are_told_the_panel_width() {
        for format in [
            DiffFormat::DiffTool(Some("difft".to_owned())),
            DiffFormat::DiffTool(None),
            DiffFormat::Pager(pager(r#""delta""#)),
            DiffFormat::Stat,
        ] {
            assert_eq!(format.render_width(PANEL_WIDTH), PANEL_WIDTH, "{format:?}");
        }

        for format in [DiffFormat::ColorWords, DiffFormat::Git, DiffFormat::Summary] {
            assert_eq!(format.render_width(PANEL_WIDTH), 0, "{format:?}");
        }
    }

    #[test]
    fn a_width_that_cannot_be_passed_on_is_no_width() {
        let diff_tool = DiffFormat::DiffTool(None);

        assert_eq!(
            diff_tool.render_width(MIN_SETTABLE_WIDTH),
            MIN_SETTABLE_WIDTH
        );
        assert_eq!(diff_tool.render_width(MIN_SETTABLE_WIDTH - 1), 0);
        assert_eq!(diff_tool.render_width(0), 0);
    }

    #[test]
    fn a_pager_is_configured_as_a_program_or_as_a_command_line() {
        let program = pager(r#""delta""#);
        assert_eq!(program.program(), "delta");
        assert!(program.args(PANEL_WIDTH).is_empty());

        let command_line = pager(r#"["delta", "--line-numbers"]"#);
        assert_eq!(command_line.program(), "delta");
        assert_eq!(command_line.args(PANEL_WIDTH), ["--line-numbers"]);

        // A command line without a program to run is a config error, as a
        // bad value is for every other setting.
        let error = toml::from_str::<JjConfig>("blazingjj.diff-pager = []\n")
            .expect_err("a pager without a program is an error");
        assert!(
            error.to_string().contains("a diff pager needs a program"),
            "the error does not say what is wrong: {error}"
        );
    }

    #[test]
    fn an_argument_asking_for_the_render_width_is_given_it() {
        let pager = pager(r#"["delta", "--width=$width", "--line-numbers"]"#);

        assert_eq!(
            pager.args(PANEL_WIDTH),
            ["--width=80", "--line-numbers"],
            "the width goes where it is asked for"
        );
        assert_eq!(
            pager.args(0),
            ["--line-numbers"],
            "an argument asking for a width there is none of is left out"
        );
    }

    #[test]
    fn a_configured_pager_is_the_format_until_another_one_is_configured() {
        assert_eq!(
            config(r#"blazingjj.diff-pager = "delta""#).diff_format(),
            DiffFormat::Pager(pager(r#""delta""#))
        );
        assert_eq!(
            config("blazingjj.diff-pager = \"delta\"\nblazingjj.diff-format = \"git\"\n")
                .diff_format(),
            DiffFormat::Git
        );
    }

    #[test]
    fn the_pager_format_falls_back_to_the_default_without_a_pager_to_run() {
        assert_eq!(
            config("blazingjj.diff-format = \"pager\"\nblazingjj.diff-pager = \"delta\"\n")
                .diff_format(),
            DiffFormat::Pager(pager(r#""delta""#))
        );
        assert_eq!(
            config("blazingjj.diff-format = \"pager\"\n").diff_format(),
            DiffFormat::ColorWords
        );
    }

    #[test]
    fn toggling_the_format_leaves_out_what_is_not_configured() {
        let delta = DiffFormat::Pager(pager(r#""delta""#));
        let difft = DiffFormat::DiffTool(Some("difft".to_owned()));

        let plain = config("");
        assert_eq!(DiffFormat::ColorWords.get_next(&plain), DiffFormat::Git);
        assert_eq!(DiffFormat::Git.get_next(&plain), DiffFormat::ColorWords);

        let with_pager = config(r#"blazingjj.diff-pager = "delta""#);
        assert_eq!(DiffFormat::Git.get_next(&with_pager), delta);
        assert_eq!(delta.get_next(&with_pager), DiffFormat::ColorWords);

        let with_both =
            config("blazingjj.diff-pager = \"delta\"\nblazingjj.diff-tool = \"difft\"\n");
        assert_eq!(DiffFormat::Git.get_next(&with_both), delta);
        assert_eq!(delta.get_next(&with_both), difft);
        assert_eq!(difft.get_next(&with_both), DiffFormat::ColorWords);

        let with_tool = config(r#"blazingjj.diff-tool = "difft""#);
        assert_eq!(DiffFormat::Git.get_next(&with_tool), difft);
    }

    #[test]
    fn a_value_is_checked_against_the_setting_it_is_for() {
        assert!(check_config_value("blazingjj.layout", "\"vertical\"").is_ok());
        assert!(check_config_value("blazingjj.layout-percent", "40").is_ok());

        // A number written as text is not a number, and neither is text
        // that names nothing the setting can be.
        assert!(check_config_value("blazingjj.layout-percent", "\"40\"").is_err());
        assert!(check_config_value("blazingjj.layout", "\"sideways\"").is_err());
    }

    #[test]
    fn poll_interval() {
        assert_eq!(
            JjConfig::default().poll_interval(),
            Some(Duration::from_secs(1))
        );

        // As `jj config list` prints it: a dotted key.
        let config: JjConfig = toml::from_str("blazingjj.poll-interval = 0.5\n").unwrap();
        assert_eq!(config.poll_interval(), Some(Duration::from_millis(500)));

        let config: JjConfig = toml::from_str("blazingjj.poll-interval = 0\n").unwrap();
        assert_eq!(config.poll_interval(), None);

        // Anything that is not a length of time is a config error, as a
        // bad value is for every other setting.
        for interval in ["-1", "nan", "inf", "1e30", "\"1s\"", "true"] {
            let config =
                toml::from_str::<JjConfig>(&format!("blazingjj.poll-interval = {interval}\n"));
            assert!(config.is_err(), "{interval}");
        }
    }

    #[test]
    fn describe_mode() {
        assert_eq!(JjConfig::default().describe_mode(), DescribeMode::Popup);

        let config: JjConfig = toml::from_str("blazingjj.describe-mode = \"jj\"\n").unwrap();
        assert_eq!(config.describe_mode(), DescribeMode::Jj);

        let config: JjConfig = toml::from_str("blazingjj.describe-mode = \"popup\"\n").unwrap();
        assert_eq!(config.describe_mode(), DescribeMode::Popup);

        assert!(toml::from_str::<JjConfig>("blazingjj.describe-mode = \"editor\"\n").is_err());
    }
}
