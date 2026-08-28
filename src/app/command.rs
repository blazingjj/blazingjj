/*! The operations against the repo, each naming both what to do and
what to do it to, so that whatever asks for one does not have to be the
component that holds the selection it acts on.

Only the app runs them. What it is to show once one is done comes back
as an [AppAction].
*/

use anyhow::Result;
use ratatui::crossterm::clipboard::CopyToClipboard;
use ratatui::crossterm::execute;

use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskOutput;
use crate::background_tasks::TaskSlot;
use crate::commander::bookmarks::Bookmark;
use crate::commander::files::File;
use crate::commander::ids::CommitId;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::commander::revset::Revset;
use crate::ui::AppAction;
use crate::ui::dialog::LoaderPopup;
use crate::ui::dialog::MessagePopup;

pub enum Command {
    /// Put text on the system clipboard.
    Copy(String),
    Duplicate(Revset),
    Absorb(Head),
    /// Push the bookmarks pointing at this change, or all of them.
    Push {
        commit_id: CommitId,
        all_bookmarks: bool,
        allow_new: bool,
    },
    Fetch {
        all_remotes: bool,
    },
    RestoreFile(File),
    UntrackFile(File),
    TrackBookmark(Bookmark),
    UntrackBookmark(Bookmark),
    /// Move the log onto the change a bookmark points at.
    ShowBookmarkInLog(Bookmark),
}

impl Command {
    /// Run the operation, returning what the app is to show for it.
    pub fn run(self, background_tasks: &BackgroundTasks) -> Result<Option<AppAction>> {
        match self {
            Command::Copy(text) => {
                let _ = execute!(std::io::stdout(), CopyToClipboard::to_clipboard_from(text));
                Ok(None)
            }
            Command::Duplicate(revset) => {
                let _ = new_commander().run_duplicate(revset.as_str());
                Ok(Some(AppAction::RefreshTab))
            }
            Command::Absorb(head) => {
                new_commander().run_absorb(&head.commit_id)?;
                Ok(Some(show_change(new_commander().get_head_latest(&head)?)))
            }
            Command::Push {
                commit_id,
                all_bookmarks,
                allow_new,
            } => Ok(Some(with_loader(
                background_tasks,
                "Pushing",
                TaskSlot::GitPush,
                move || Ok(new_commander().git_push(all_bookmarks, allow_new, &commit_id)?),
            ))),
            Command::Fetch { all_remotes } => Ok(Some(with_loader(
                background_tasks,
                "Fetching",
                TaskSlot::GitFetch,
                move || Ok(new_commander().git_fetch(all_remotes)?),
            ))),
            Command::RestoreFile(file) => {
                if let Err(err) = new_commander().restore_file(&file) {
                    return Ok(Some(message("Can't restore file", err.to_string())));
                }
                Ok(Some(show_working_copy_files()?))
            }
            // This works even for deleted files, as jj does not fail on
            // those.
            Command::UntrackFile(file) => {
                if new_commander().untrack_file(&file).is_err() {
                    return Ok(Some(message(
                        "Can't untrack file",
                        "Make sure that file is ignored",
                    )));
                }
                Ok(Some(show_working_copy_files()?))
            }
            Command::TrackBookmark(bookmark) => {
                new_commander().track_bookmark(&bookmark)?;
                Ok(Some(AppAction::RefreshTab))
            }
            Command::UntrackBookmark(bookmark) => {
                new_commander().untrack_bookmark(&bookmark)?;
                Ok(Some(AppAction::RefreshTab))
            }
            Command::ShowBookmarkInLog(bookmark) => Ok(Some(AppAction::ViewLog(
                new_commander().get_bookmark_head(&bookmark)?,
            ))),
        }
    }
}

/// Put `change` up wherever a change shows, the repo having moved under
/// whatever else is on screen.
fn show_change(change: Head) -> AppAction {
    AppAction::Multiple(vec![
        AppAction::ViewLog(change.clone()),
        AppAction::ChangeHead(change),
        AppAction::RefreshTab,
    ])
}

/// Show the files of the working copy commit, the operation having
/// changed what is in it.
fn show_working_copy_files() -> Result<AppAction> {
    Ok(AppAction::Multiple(vec![
        AppAction::ViewFiles(new_commander().get_current_head()?),
        AppAction::RefreshTab,
    ]))
}

fn message(title: &'static str, text: impl Into<String>) -> AppAction {
    AppAction::SetPopup(Box::new(MessagePopup::new(title, text)))
}

/// Run `operation` in `slot` and put up a loader popup for it, which
/// stays until that slot's result arrives. The popup swallows all
/// input, so the slot it waits for has to be the one submitted here.
fn with_loader<F>(
    background_tasks: &BackgroundTasks,
    operation_name: &str,
    slot: TaskSlot,
    operation: F,
) -> AppAction
where
    F: FnOnce() -> TaskOutput + Send + 'static,
{
    background_tasks.submit_uninterruptible(slot.clone(), operation);

    AppAction::SetPopup(Box::new(LoaderPopup::new(operation_name.to_owned(), slot)))
}
