/*! The dialog module contains all modal dialogs,
previously known as popups.

A Component can launch a dialog by sending
[`AppAction::SetPopup(<popup instance>)`](crate::ui::AppAction).
Once launched, a dialog will receive all input events from the App,
until it sends [`AppAction::ClosePopup`](crate::ui::AppAction).
*/

mod bookmark_name;
mod bookmark_set;
mod choice;
mod command;
mod confirm;
mod context_menu;
mod describe;
mod help;
mod loader;
mod message;
mod new_insert;
mod parent_select;
mod rebase;

pub use bookmark_name::BookmarkNameMode;
pub use bookmark_name::BookmarkNamePopup;
pub use bookmark_set::BookmarkSetPopup;
pub use choice::ChoicePopup;
pub use command::CommandPopup;
pub use confirm::ConfirmPopup;
pub use context_menu::files_context_menu;
pub use context_menu::log_context_menu;
pub use describe::DescribePopup;
pub use help::HelpPopup;
pub use loader::LoaderPopup;
pub use message::MessagePopup;
pub use new_insert::new_insert;
pub use parent_select::parent_select;
pub use rebase::RebasePopup;
