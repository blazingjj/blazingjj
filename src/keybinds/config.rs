use super::Shortcut;

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct KeybindsConfig {
    pub scroll_down: Option<Keybind>,
    pub scroll_up: Option<Keybind>,
    pub scroll_down_half: Option<Keybind>,
    pub scroll_up_half: Option<Keybind>,

    pub focus_current: Option<Keybind>,
    pub refresh: Option<Keybind>,
    pub open_help: Option<Keybind>,

    pub next_tab: Option<Keybind>,
    pub prev_tab: Option<Keybind>,

    pub open_context_menu: Option<Keybind>,
    pub command_popup: Option<Keybind>,
    pub interactive_command_popup: Option<Keybind>,
    pub quit: Option<Keybind>,

    pub log_tab: Option<LogTabKeybindsConfig>,
    pub files_tab: Option<FilesTabKeybindsConfig>,
    pub bookmarks_tab: Option<BookmarksTabKeybindsConfig>,
    pub evolog_tab: Option<EvologTabKeybindsConfig>,
    pub details_panel: Option<DetailsPanelKeybindsConfig>,
    pub popup: Option<PopupKeybindsConfig>,
    pub text_popup: Option<TextPopupKeybindsConfig>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DetailsPanelKeybindsConfig {
    pub scroll_down: Option<Keybind>,
    pub scroll_up: Option<Keybind>,
    pub scroll_down_half: Option<Keybind>,
    pub scroll_up_half: Option<Keybind>,
    pub scroll_down_page: Option<Keybind>,
    pub scroll_up_page: Option<Keybind>,

    pub toggle_wrap: Option<Keybind>,
    pub toggle_diff_format: Option<Keybind>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PopupKeybindsConfig {
    pub accept: Option<Keybind>,
    pub cancel: Option<Keybind>,

    pub scroll_down_half: Option<Keybind>,
    pub scroll_up_half: Option<Keybind>,
    pub scroll_down_page: Option<Keybind>,
    pub scroll_up_page: Option<Keybind>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TextPopupKeybindsConfig {
    pub accept: Option<Keybind>,
    pub cancel: Option<Keybind>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum Keybind {
    Single(Shortcut),
    Multiple(Vec<Shortcut>),
    Enable(bool),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LogTabKeybindsConfig {
    pub goto_parent: Option<Keybind>,

    pub duplicate: Option<Keybind>,
    pub create_new: Option<Keybind>,
    pub create_new_describe: Option<Keybind>,
    pub squash: Option<Keybind>,
    pub squash_ignore_immutable: Option<Keybind>,
    pub edit_change: Option<Keybind>,
    pub edit_change_ignore_immutable: Option<Keybind>,
    pub abandon: Option<Keybind>,
    pub absorb: Option<Keybind>,
    pub describe: Option<Keybind>,
    pub edit_revset: Option<Keybind>,
    pub set_bookmark: Option<Keybind>,
    pub open_files: Option<Keybind>,
    pub open_evolog: Option<Keybind>,
    pub copy_change_id: Option<Keybind>,
    pub copy_rev: Option<Keybind>,
    pub rebase: Option<Keybind>,

    pub push: Option<Keybind>,
    pub push_new: Option<Keybind>,
    pub push_all: Option<Keybind>,
    pub push_all_new: Option<Keybind>,
    pub fetch: Option<Keybind>,
    pub fetch_all: Option<Keybind>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct FilesTabKeybindsConfig {
    pub untrack: Option<Keybind>,
    pub restore: Option<Keybind>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BookmarksTabKeybindsConfig {
    pub toggle_show_all: Option<Keybind>,

    pub create_bookmark: Option<Keybind>,
    pub rename_bookmark: Option<Keybind>,
    pub delete_bookmark: Option<Keybind>,
    pub forget_bookmark: Option<Keybind>,
    pub track_bookmark: Option<Keybind>,
    pub untrack_bookmark: Option<Keybind>,
    pub set_bookmark: Option<Keybind>,

    pub view_in_log: Option<Keybind>,
    pub create_new: Option<Keybind>,
    pub create_new_describe: Option<Keybind>,
    pub edit_change: Option<Keybind>,
    pub edit_change_ignore_immutable: Option<Keybind>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct EvologTabKeybindsConfig {
    pub open_files: Option<Keybind>,
    pub duplicate: Option<Keybind>,
    pub copy_rev: Option<Keybind>,
}
