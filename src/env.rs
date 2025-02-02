/** The environment configures the application.

It is a combination of
- configuration files
- environment variables
- command line arguments
*/
use std::fmt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use ratatui::style::Color;
use serde::Deserialize;
use serde::Deserializer;
use serde::de;

use crate::commander::MIN_SETTABLE_WIDTH;
use crate::commander::RemoveEndLine;
use crate::commander::get_output_args;
use crate::keybinds::KeybindsConfig;

/// Singleton holding application environment
static ENV: OnceLock<Env> = OnceLock::new();

/// Set application environment. Panics if called twice
pub fn set_env(env: Env) {
    ENV.set(env).expect("set_env must only be called once");
}

/// Get application environment. Panics if not set first
pub fn get_env() -> &'static Env {
    ENV.get().unwrap()
}

/// The configured keybindings, if any. Unlike [`get_env()`], this works
/// before the environment is set, as in tests building components.
pub fn keybinds_config() -> Option<&'static KeybindsConfig> {
    ENV.get().and_then(|env| env.jj_config.keybinds())
}

/// A default environment for tests that build components reading it. The
/// tests share one process, so whichever gets there first sets it.
#[cfg(test)]
pub fn set_test_env() {
    let _ = ENV.set(Env {
        root: ".".to_owned(),
        jj_config: JjConfig::default(),
        default_revset: None,
        jj_bin: "jj".to_owned(),
    });
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
    diff_format: Option<DiffFormat>,
    diff_tool: Option<String>,
    bookmark_template: Option<String>,
    layout: JJLayout,
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
            layout_percent: 50,
            poll_interval: Some(Duration::from_secs(1)),
            // Standard defaults for the rest
            describe_mode: DescribeMode::default(),
            diff_format: None,
            diff_tool: None,
            bookmark_template: None,
            layout: JJLayout::default(),
            keybinds: None,
        }
    }
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
    format: Option<DiffFormat>,
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
            .clone()
            .or_else(|| self.ui.diff.format.clone())
            .or_else(|| self.diff_tool().map(DiffFormat::DiffTool))
            .unwrap_or(DiffFormat::ColorWords)
    }

    pub fn diff_tool(&self) -> Option<Option<String>> {
        match self.blazingjj.diff_tool.clone() {
            tool @ Some(_) => Some(tool),
            _ if self.ui.diff.tool.is_some() => Some(None),
            _ => None,
        }
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

        // Read/parse jj config
        let cfg = Command::new(&jj_bin)
            .arg("config")
            .arg("list")
            .args(get_output_args(false, true))
            .current_dir(&root)
            .output()
            .context("Failed to get jj config")?
            .stdout;
        let jj_config: JjConfig = toml::from_slice(&cfg).context("Failed to parse jj config")?;

        Ok(Env {
            root,
            jj_config,
            default_revset,
            jj_bin,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum DescribeMode {
    #[default]
    Popup,
    Jj,
}

#[derive(Clone, Debug, Deserialize, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum DiffFormat {
    #[default]
    ColorWords,
    Git,
    DiffTool(Option<String>),
    // Configuration only, [DiffFormat::get_next] does not cycle through these
    Summary,
    Stat,
}

impl DiffFormat {
    pub fn get_next(&self, diff_tool: Option<Option<String>>) -> DiffFormat {
        match self {
            DiffFormat::ColorWords => DiffFormat::Git,
            DiffFormat::Git => {
                if let Some(diff_tool) = diff_tool {
                    DiffFormat::DiffTool(diff_tool)
                } else {
                    DiffFormat::ColorWords
                }
            }
            _ => DiffFormat::ColorWords,
        }
    }

    /// How wide output in this format is rendered, given the width of the
    /// panel showing it. The width reaches an external diff tool through the
    /// COLUMNS environment variable, and `--stat` scales its histogram to
    /// it; the other formats produce the same output whatever the width,
    /// and the panel wraps or scrolls it.
    ///
    /// A width too narrow to be passed on makes no difference either, so it
    /// comes out as no width at all rather than as a value of its own.
    pub fn render_width(&self, panel_width: usize) -> usize {
        match self {
            DiffFormat::DiffTool(_) | DiffFormat::Stat if panel_width >= MIN_SETTABLE_WIDTH => {
                panel_width
            }
            _ => 0,
        }
    }
}

/// How the format is named in the UI, which for an external tool is the
/// tool itself, as that is what tells the formats it renders apart.
impl fmt::Display for DiffFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffFormat::ColorWords => write!(f, "color-words"),
            DiffFormat::Git => write!(f, "git"),
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

    #[test]
    fn only_formats_that_scale_are_told_the_panel_width() {
        for format in [
            DiffFormat::DiffTool(Some("difft".to_owned())),
            DiffFormat::DiffTool(None),
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
