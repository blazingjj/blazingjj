/*! The operations against the repo, each naming both what to do and
what to do it to, so that whatever asks for one does not have to be the
component that holds the selection it acts on.

Only the app runs them. What it is to show once one is done comes back
as an [AppAction].
*/

use anyhow::Result;
use ratatui::crossterm::clipboard::CopyToClipboard;
use ratatui::crossterm::execute;
use ratatui::layout::Alignment;
use ratatui::text::Line;
use ratatui::text::Text;

use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskOutput;
use crate::background_tasks::TaskSlot;
use crate::commander::bookmarks::Bookmark;
use crate::commander::files::File;
use crate::commander::ids::CommitId;
use crate::commander::jj::NewInsertMode;
use crate::commander::jj::RebaseSource;
use crate::commander::jj::RebaseTarget;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::commander::revset::Revset;
use crate::env::JjConfig;
use crate::ui::AppAction;
use crate::ui::dialog::BookmarkNameMode;
use crate::ui::dialog::BookmarkNamePopup;
use crate::ui::dialog::ConfirmPopup;
use crate::ui::dialog::DescribePopup;
use crate::ui::dialog::LoaderPopup;
use crate::ui::dialog::MessagePopup;
use crate::ui::dialog::new_insert;

/// What a new change is created from, which decides whether the log is
/// done marking it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NewSource {
    /// The changes the log has marked, which the new change now stands
    /// on.
    Marks,
    /// A single change, named by the selection or by a bookmark.
    Change,
}

pub enum Command {
    /// Put text on the system clipboard.
    Copy(String),
    Duplicate(Revset),
    Absorb(Head),
    /// Create a change from `revset`, put where `insert` says.
    New {
        revset: Revset,
        source: NewSource,
        insert: NewInsertMode,
        describe: bool,
    },
    Squash {
        target: Head,
        ignore_immutable: bool,
    },
    Edit {
        revset: Revset,
        ignore_immutable: bool,
    },
    /// Abandon the marked changes, or the selected one when none are
    /// marked, moving the selection out of them.
    Abandon {
        marked: Vec<CommitId>,
        selected: Head,
    },
    Describe {
        head: Head,
        description: String,
    },
    Rebase {
        source: Head,
        source_mode: RebaseSource,
        target: Head,
        target_mode: RebaseTarget,
    },
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
    CreateBookmark(String),
    RenameBookmark {
        old_name: String,
        new_name: String,
    },
    /// Put the bookmark of this name on the commit, creating it if there
    /// is no bookmark by that name yet.
    SetBookmark {
        name: String,
        commit_id: CommitId,
    },
    DeleteBookmark(String),
    ForgetBookmark(String),
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
                Ok(Some(AppAction::MarkTabsStale))
            }
            Command::Absorb(head) => {
                new_commander().run_absorb(&head.commit_id)?;
                Ok(Some(show_change(new_commander().get_head_latest(&head)?)))
            }
            Command::New {
                revset,
                source,
                insert,
                describe,
            } => {
                // Inserting can hit immutable changes, so report the
                // refusal and leave the marks for another attempt.
                if let Err(err) = new_commander().run_new_with_insert(revset, insert) {
                    return Ok(Some(AppAction::SetPopup(Box::new(
                        MessagePopup::new("New", format!("{err:#}")).text_align(Alignment::Left),
                    ))));
                }

                let head = new_commander().get_current_head()?;
                let mut actions = vec![show_change(head.clone())];
                if source == NewSource::Marks {
                    actions.push(AppAction::ClearLogMarks);
                }
                if describe {
                    actions.push(AppAction::SetPopup(Box::new(DescribePopup::new(
                        head,
                        vec![],
                    ))));
                }

                Ok(Some(AppAction::Multiple(actions)))
            }
            Command::Squash {
                target,
                ignore_immutable,
            } => {
                new_commander().run_squash(target.commit_id.as_str(), ignore_immutable)?;
                Ok(Some(show_change(new_commander().get_current_head()?)))
            }
            Command::Edit {
                revset,
                ignore_immutable,
            } => {
                new_commander().run_edit(revset, ignore_immutable)?;
                Ok(Some(show_change(new_commander().get_current_head()?)))
            }
            Command::Abandon { marked, selected } => {
                let (revset, abandoned) = match Revset::union(&marked) {
                    Some(revset) => (revset, marked.as_slice()),
                    None => (
                        Revset::from(&selected.commit_id),
                        std::slice::from_ref(&selected.commit_id),
                    ),
                };

                // The selection has to come out of what is being
                // abandoned, or it names a change that is no longer there.
                let mut moved_to = selected.clone();
                while abandoned.contains(&moved_to.commit_id) {
                    moved_to = new_commander().get_commit_parent(&moved_to.commit_id)?;
                }

                new_commander().run_abandon(revset)?;

                // Abandoning may have rebased what the selection landed
                // on.
                let moved_to = new_commander().get_head_latest(&moved_to)?;
                let mut actions = vec![
                    AppAction::ClearLogMarks,
                    AppAction::ViewLog(moved_to.clone()),
                ];
                if moved_to != selected {
                    actions.push(AppAction::ChangeHead(moved_to));
                }
                actions.push(AppAction::MarkTabsStale);

                Ok(Some(AppAction::Multiple(actions)))
            }
            Command::Describe { head, description } => {
                new_commander().run_describe(&head.commit_id, &description)?;
                Ok(Some(AppAction::Multiple(vec![
                    AppAction::ViewLog(new_commander().get_head_latest(&head)?),
                    AppAction::MarkTabsStale,
                ])))
            }
            Command::Rebase {
                source,
                source_mode,
                target,
                target_mode,
            } => {
                if let Err(err) = new_commander().run_rebase(
                    source_mode,
                    &source.commit_id,
                    target_mode,
                    &target.commit_id,
                ) {
                    return Ok(Some(message("Error", err.to_string())));
                }
                Ok(Some(AppAction::MarkTabsStale))
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
            Command::CreateBookmark(name) => match new_commander().create_bookmark(&name) {
                Ok(_) => Ok(Some(AppAction::Multiple(vec![
                    AppAction::ViewBookmark(name),
                    AppAction::MarkTabsStale,
                ]))),
                // Put the question back with what was typed, since a
                // refused name is usually one to correct rather than one
                // to give up on.
                Err(err) => Ok(Some(AppAction::SetPopup(Box::new(
                    BookmarkNamePopup::refused(BookmarkNameMode::Create, name, err),
                )))),
            },
            Command::RenameBookmark { old_name, new_name } => {
                match new_commander().rename_bookmark(&old_name, &new_name) {
                    Ok(()) => Ok(Some(AppAction::Multiple(vec![
                        AppAction::ViewBookmark(new_name),
                        AppAction::MarkTabsStale,
                    ]))),
                    Err(err) => Ok(Some(AppAction::SetPopup(Box::new(
                        BookmarkNamePopup::refused(
                            BookmarkNameMode::Rename { old_name },
                            new_name,
                            err,
                        ),
                    )))),
                }
            }
            Command::SetBookmark { name, commit_id } => {
                new_commander().set_bookmark_commit(&name, &commit_id)?;
                Ok(Some(AppAction::MarkTabsStale))
            }
            Command::DeleteBookmark(name) => match new_commander().delete_bookmark(&name) {
                Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                Err(err) => Ok(Some(message("Delete error", err.to_string()))),
            },
            Command::ForgetBookmark(name) => match new_commander().forget_bookmark(&name) {
                Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                Err(err) => Ok(Some(message("Forget error", err.to_string()))),
            },
            Command::TrackBookmark(bookmark) => {
                new_commander().track_bookmark(&bookmark)?;
                Ok(Some(AppAction::MarkTabsStale))
            }
            Command::UntrackBookmark(bookmark) => {
                new_commander().untrack_bookmark(&bookmark)?;
                Ok(Some(AppAction::MarkTabsStale))
            }
            Command::ShowBookmarkInLog(bookmark) => Ok(Some(AppAction::ViewLog(
                new_commander().get_bookmark_head(&bookmark)?,
            ))),
        }
    }
}

/// Asking for a new change from `revset`, which `target` names as the
/// user sees it: where the change goes is a question of its own.
pub fn ask_new_change(
    config: JjConfig,
    revset: Revset,
    source: NewSource,
    target: &str,
    describe: bool,
) -> AppAction {
    AppAction::SetPopup(Box::new(new_insert(config, target, |insert| {
        AppAction::Run(Command::New {
            revset: revset.clone(),
            source,
            insert,
            describe,
        })
    })))
}

/// Asking to squash into `selected`: the target it picks, the refusal
/// when that target cannot take it, or the question that runs it.
pub fn ask_squash(config: JjConfig, selected: &Head, ignore_immutable: bool) -> Result<AppAction> {
    // Squashing the change the working copy is on has nowhere to go but
    // its parent.
    let at = new_commander().get_current_head()?;
    let onto_parent = selected.change_id == at.change_id;
    let target = if onto_parent {
        match new_commander().get_commit_parent(&at.commit_id) {
            Ok(parent) => parent,
            Err(_) => return Ok(message("Squash", "Cannot squash onto current change")),
        }
    } else {
        selected.clone()
    };

    if target.immutable && !ignore_immutable {
        return Ok(message("Squash", "Cannot squash onto immutable change"));
    }

    let mut lines = vec![
        Line::from(if onto_parent {
            "Are you sure you want to squash @ into its parent?"
        } else {
            "Are you sure you want to squash @ into this change?"
        }),
        Line::from(format!("Squash into {}", target.change_id.as_str())),
    ];
    if ignore_immutable {
        lines.push(Line::from("This change is immutable."));
    }

    Ok(confirm(
        config,
        "Squash",
        Text::from(lines),
        Command::Squash {
            target,
            ignore_immutable,
        },
    ))
}

/// Asking to edit `target`: the refusal when it is immutable, or the
/// question that runs it.
pub fn ask_edit(config: JjConfig, target: &Head, ignore_immutable: bool) -> AppAction {
    if target.immutable && !ignore_immutable {
        return message(
            "Edit",
            "The change cannot be edited because it is immutable.",
        );
    }

    let mut lines = vec![
        Line::from("Are you sure you want to edit an existing change?"),
        Line::from(format!("Change: {}", target.change_id.as_str())),
    ];
    if ignore_immutable {
        lines.push(Line::from("This change is immutable."));
    }

    confirm(
        config,
        "Edit",
        Text::from(lines),
        Command::Edit {
            revset: Revset::from(&target.commit_id),
            ignore_immutable,
        },
    )
}

/// Asking to edit the change a bookmark points at, which unlike a change
/// picked out of the log has to be looked up to know whether it can be.
pub fn ask_edit_bookmark(
    config: JjConfig,
    bookmark: &Bookmark,
    ignore_immutable: bool,
) -> Result<AppAction> {
    let revset = Revset::expression(bookmark.to_string());
    if new_commander().check_revision_immutable(revset.as_str())? && !ignore_immutable {
        return Ok(message(
            "Edit",
            "The change cannot be edited because it is immutable.",
        ));
    }

    Ok(confirm(
        config,
        "Edit",
        Text::from(vec![
            Line::from("Are you sure you want to edit an existing change?"),
            Line::from(format!("Bookmark: {bookmark}")),
        ]),
        Command::Edit {
            revset,
            ignore_immutable,
        },
    ))
}

/// Asking to abandon the `marked` changes, or `selected` when none are
/// marked: the refusal when it is immutable, or the question that runs
/// it.
pub fn ask_abandon(config: JjConfig, selected: &Head, marked: Vec<CommitId>) -> AppAction {
    if selected.immutable {
        return message(
            "Abandon",
            "The change cannot be abandoned because it is immutable.",
        );
    }

    let text = if marked.is_empty() {
        Text::from(vec![
            Line::from("Are you sure you want to abandon this change?"),
            Line::from(format!("Change: {}", selected.change_id.as_str())),
        ])
    } else {
        Text::from(vec![Line::from(format!(
            "Are you sure you want to abandon {} marked changes?",
            marked.len()
        ))])
    };

    confirm(
        config,
        "Abandon",
        text,
        Command::Abandon {
            marked,
            selected: selected.clone(),
        },
    )
}

/// Asking to delete the bookmark of this name.
pub fn ask_delete_bookmark(config: JjConfig, name: &str) -> AppAction {
    confirm(
        config,
        "Delete",
        Text::from(format!(
            "Are you sure you want to delete the {name} bookmark?"
        )),
        Command::DeleteBookmark(name.to_owned()),
    )
}

/// Asking to forget the bookmark of this name.
pub fn ask_forget_bookmark(config: JjConfig, name: &str) -> AppAction {
    confirm(
        config,
        "Forget",
        Text::from(format!(
            "Are you sure you want to forget the {name} bookmark?"
        )),
        Command::ForgetBookmark(name.to_owned()),
    )
}

/// Put `question` to the user, running `command` if they say yes.
fn confirm(
    config: JjConfig,
    title: &'static str,
    question: Text<'static>,
    command: Command,
) -> AppAction {
    AppAction::SetPopup(Box::new(ConfirmPopup::new(
        config,
        title,
        question,
        AppAction::Run(command),
    )))
}

/// Put `change` up wherever a change shows, the repo having moved under
/// whatever else is on screen.
fn show_change(change: Head) -> AppAction {
    AppAction::Multiple(vec![
        AppAction::ViewLog(change.clone()),
        AppAction::ChangeHead(change),
        AppAction::MarkTabsStale,
    ])
}

/// Show the files of the working copy commit, the operation having
/// changed what is in it.
fn show_working_copy_files() -> Result<AppAction> {
    Ok(AppAction::Multiple(vec![
        AppAction::ViewFiles(new_commander().get_current_head()?),
        AppAction::MarkTabsStale,
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

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::commander::ids::ChangeId;

    fn head(change_id: &str, immutable: bool) -> Head {
        Head {
            change_id: ChangeId(change_id.to_owned()),
            commit_id: CommitId(format!("commit-{change_id}")),
            divergent: false,
            immutable,
        }
    }

    /// What the popup the action puts up says, as one string per row.
    fn rows(action: AppAction) -> Vec<String> {
        let AppAction::SetPopup(mut popup) = action else {
            panic!("the action puts a popup up");
        };

        let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("the test backend");
        terminal
            .draw(|f| popup.draw(f, f.area()).expect("the popup draws"))
            .expect("the frame is drawn");

        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn says(action: AppAction, text: &str) -> bool {
        rows(action).iter().any(|row| row.contains(text))
    }

    #[test]
    fn an_immutable_change_is_refused_rather_than_asked_about() {
        assert!(says(
            ask_edit(JjConfig::default(), &head("a", true), false),
            "because it is immutable"
        ));
    }

    #[test]
    fn an_immutable_change_is_asked_about_when_immutability_is_ignored() {
        assert!(says(
            ask_edit(JjConfig::default(), &head("a", true), true),
            "This change is immutable"
        ));
    }

    #[test]
    fn abandoning_names_the_selected_change_when_none_are_marked() {
        assert!(says(
            ask_abandon(JjConfig::default(), &head("a", false), vec![]),
            "Change: a"
        ));
    }

    #[test]
    fn abandoning_counts_the_marked_changes_rather_than_naming_them() {
        let marked = vec![CommitId("commit-a".into()), CommitId("commit-b".into())];

        assert!(says(
            ask_abandon(JjConfig::default(), &head("a", false), marked),
            "abandon 2 marked changes"
        ));
    }

    #[test]
    fn an_immutable_selection_is_never_abandoned_even_with_others_marked() {
        let marked = vec![CommitId("commit-a".into())];

        assert!(says(
            ask_abandon(JjConfig::default(), &head("a", true), marked),
            "because it is immutable"
        ));
    }
}
