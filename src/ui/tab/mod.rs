/*! The tab module contains all tab components seen in the main menu.
 */

pub mod bookmarks;
mod commands;
mod evolog;
pub mod files;
mod keybindings;
pub mod log;
mod menus;
mod op_log;
mod settings;
mod styles;
mod workspaces;

pub use bookmarks::BookmarksTab;
pub use commands::CommandsTab;
pub use evolog::EvologTab;
pub use files::FilesTab;
pub use keybindings::KeybindingsTab;
pub use log::LogTab;
pub use menus::MenusTab;
pub use op_log::OpLogTab;
pub use settings::SettingsTab;
pub use styles::StylesTab;
pub use workspaces::WorkspacesTab;
