/*! The dialog module contains all modal dialogs,
previously known as popups.

A Component can launch a dialog by sending
[`AppAction::SetPopup(<popup instance>)`](crate::ui::AppAction).
Once launched, a dialog will receive all input events from the App,
until it sends [`AppAction::PopupDone`](crate::ui::AppAction) or
[`AppAction::PopupCanceled`](crate::ui::AppAction).
*/

mod bookmark_set;
mod command;
mod help;
mod loader;
mod message;
mod rebase;

pub use bookmark_set::BookmarkSetPopup;
pub use command::CommandPopup;
pub use help::HelpPopup;
pub use loader::LoaderPopup;
pub use message::MessagePopup;
pub use rebase::RebasePopup;
