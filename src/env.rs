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

// TODO: After 0.18, remove Config and replace with JjConfig
#[derive(Deserialize, Debug, Clone, Default)]
pub struct Config {
    #[serde(rename = "blazingjj.highlight-color")]
    blazingjj_highlight_color: Option<Color>,
    #[serde(rename = "blazingjj.diff-format")]
    blazingjj_diff_format: Option<DiffFormat>,
    #[serde(rename = "blazingjj.diff-tool")]
    blazingjj_diff_tool: Option<String>,
    #[serde(rename = "blazingjj.bookmark-template")]
    blazingjj_bookmark_template: Option<String>,
    #[serde(rename = "blazingjj.layout")]
    blazingjj_layout: Option<JJLayout>,
    #[serde(rename = "blazingjj.layout-percent")]
    blazingjj_layout_percent: Option<u16>,
    #[serde(rename = "blazingjj.keybinds")]
    blazingjj_keybinds: Option<KeybindsConfig>,
    #[serde(rename = "ui.diff.format")]
    ui_diff_format: Option<DiffFormat>,
    #[serde(rename = "ui.diff.tool")]
    ui_diff_tool: Option<()>,
    #[serde(rename = "templates.git_push_bookmark")]
    git_push_bookmark_template: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct JjConfig {
    blazingjj: Option<JjConfigBlazingjj>,
    ui: Option<JjConfigUi>,
    templates: Option<JjConfigTemplates>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct JjConfigBlazingjj {
    highlight_color: Option<Color>,
    diff_format: Option<DiffFormat>,
    diff_tool: Option<String>,
    bookmark_prefix: Option<String>,
    layout: Option<JJLayout>,
    layout_percent: Option<u16>,
    keybinds: Option<KeybindsConfig>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct JjConfigUi {
    diff: Option<JjConfigUiDiff>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct JjConfigUiDiff {
    format: Option<DiffFormat>,
    tool: Option<toml::Value>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct JjConfigTemplates {
    git_push_bookmark: Option<String>,
}

impl Config {
    pub fn diff_format(&self) -> DiffFormat {
        let default = if let Some(diff_tool) = self.diff_tool() {
            DiffFormat::DiffTool(diff_tool)
        } else {
            DiffFormat::ColorWords
        };
        self.blazingjj_diff_format
            .clone()
            .unwrap_or(self.ui_diff_format.clone().unwrap_or(default))
    }

    pub fn diff_tool(&self) -> Option<Option<String>> {
        if let Some(diff_tool) = self.blazingjj_diff_tool.as_ref() {
            return Some(Some(diff_tool.to_owned()));
        }

        if self.ui_diff_tool.is_some() {
            return Some(None);
        }

        None
    }

    pub fn highlight_color(&self) -> Color {
        self.blazingjj_highlight_color
            .unwrap_or(Color::Rgb(50, 50, 150))
    }

    pub fn bookmark_template(&self) -> String {
        self.blazingjj_bookmark_template
            .clone()
            .or(self.git_push_bookmark_template.clone())
            .unwrap_or("'push-' ++ change_id.short()".to_string())
    }

    pub fn layout(&self) -> JJLayout {
        self.blazingjj_layout.unwrap_or(JJLayout::Horizontal)
    }

    pub fn layout_percent(&self) -> u16 {
        self.blazingjj_layout_percent.unwrap_or(50)
    }

    pub fn keybinds(&self) -> Option<&KeybindsConfig> {
        self.blazingjj_keybinds.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct Env {
    pub config: Config,
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
        let config_toml = String::from_utf8(
            Command::new(&jj_bin)
                .arg("config")
                .arg("list")
                .arg("--template")
                .arg("'\"' ++ name ++ '\"' ++ '=' ++ value ++ '\n'")
                .args(get_output_args(false, true))
                .current_dir(&root)
                .output()
                .context("Failed to get jj config")?
                .stdout,
        )?;
        // Prior to https://github.com/martinvonz/jj/pull/3728, keys were not TOML-escaped.
        let config = match toml::from_str::<Config>(&config_toml) {
            Ok(config) => config,
            Err(_) => {
                let config_toml = String::from_utf8(
                    Command::new(&jj_bin)
                        .arg("config")
                        .arg("list")
                        .args(get_output_args(false, true))
                        .current_dir(&root)
                        .output()
                        .context("Failed to get jj config")?
                        .stdout,
                )?;
                toml::from_str::<JjConfig>(&config_toml)
                    .context("Failed to parse jj config")
                    .map(|config| Config {
                        blazingjj_highlight_color: config
                            .blazingjj
                            .as_ref()
                            .and_then(|blazingjj| blazingjj.highlight_color),
                        blazingjj_diff_format: config
                            .blazingjj
                            .as_ref()
                            .and_then(|blazingjj| blazingjj.diff_format.clone()),
                        blazingjj_diff_tool: config
                            .blazingjj
                            .as_ref()
                            .and_then(|blazingjj| blazingjj.diff_tool.clone()),
                        blazingjj_bookmark_template: config
                            .blazingjj
                            .as_ref()
                            .and_then(|blazingjj| blazingjj.bookmark_prefix.clone()),
                        blazingjj_layout: config
                            .blazingjj
                            .as_ref()
                            .and_then(|blazingjj| blazingjj.layout),
                        blazingjj_layout_percent: config
                            .blazingjj
                            .as_ref()
                            .and_then(|blazingjj| blazingjj.layout_percent),
                        blazingjj_keybinds: config
                            .blazingjj
                            .as_ref()
                            .and_then(|blazingjj| blazingjj.keybinds.clone()),
                        ui_diff_format: config
                            .ui
                            .as_ref()
                            .and_then(|ui| ui.diff.as_ref().and_then(|diff| diff.format.clone())),
                        ui_diff_tool: config.ui.as_ref().and_then(|ui| {
                            ui.diff
                                .as_ref()
                                .and_then(|diff| diff.tool.as_ref().map(|_| ()))
                        }),
                        git_push_bookmark_template: config
                            .templates
                            .and_then(|templates| templates.git_push_bookmark),
                    })?
            }
        };

        Ok(Env {
            root,
            config,
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
