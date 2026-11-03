/*! The tab module contains all tab components seen in the main menu.
 */

pub mod bookmarks;
mod evolog;
pub mod files;
mod keybindings;
pub mod log;
mod op_log;
mod settings;
mod styles;

pub use bookmarks::BookmarksTab;
pub use evolog::EvologTab;
pub use files::FilesTab;
pub use keybindings::KeybindingsTab;
pub use log::LogTab;
pub use op_log::OpLogTab;
pub use settings::SettingsTab;
pub use styles::StylesTab;
