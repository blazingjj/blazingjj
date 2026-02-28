use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use ratatui::style::Color;
use serde::Deserialize;

use crate::commander::RemoveEndLine;
use crate::commander::get_output_args;
use crate::keybinds::KeybindsConfig;

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
    diff_format: Option<DiffFormat>,
    diff_tool: Option<String>,
    bookmark_template: Option<String>,
    layout: JJLayout,
    layout_percent: u16,
    keybinds: Option<KeybindsConfig>,
}

impl Default for JjConfigBlazingjj {
    fn default() -> Self {
        Self {
            highlight_color: Color::Rgb(50, 50, 150),
            layout_percent: 50,
            // Standard defaults for the rest
            diff_format: None,
            diff_tool: None,
            bookmark_template: None,
            layout: JJLayout::default(),
            keybinds: None,
        }
    }
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

    pub fn layout(&self) -> JJLayout {
        self.blazingjj.layout
    }

    pub fn layout_percent(&self) -> u16 {
        self.blazingjj.layout_percent
    }

    pub fn keybinds(&self) -> Option<&KeybindsConfig> {
        self.blazingjj.keybinds.as_ref()
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

#[derive(Clone, Debug, Deserialize, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum DiffFormat {
    #[default]
    ColorWords,
    Git,
    DiffTool(Option<String>),
    // Unused
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
