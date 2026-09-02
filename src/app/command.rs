/*! The operations against the repo, each naming both what to do and
what to do it to, so that whatever asks for one does not have to be the
component that holds the selection it acts on.

Only the app runs them. What it is to show once one is done comes back
as an [AppAction].
*/

use std::fmt::Display;
use std::path::Path;

use ansi_to_tui::IntoText;
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
use crate::commander::files::DiffType;
use crate::commander::files::File;
use crate::commander::ids::ChangeId;
use crate::commander::ids::CommitId;
use crate::commander::ids::OperationId;
use crate::commander::jj::NewInsertMode;
use crate::commander::jj::PushTarget;
use crate::commander::jj::RebaseSource;
use crate::commander::jj::RebaseTarget;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::commander::operation::Operation;
use crate::commander::program::Program;
use crate::commander::revset::Revset;
use crate::env::Editor;
use crate::env::EditorMode;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::keybinds::PushScope;
use crate::ui::AppAction;
use crate::ui::Interactive;
use crate::ui::dialog::BookmarkNameMode;
use crate::ui::dialog::BookmarkNamePopup;
use crate::ui::dialog::BookmarkSetPopup;
use crate::ui::dialog::ChoicePopup;
use crate::ui::dialog::ConfirmPopup;
use crate::ui::dialog::DescribePopup;
use crate::ui::dialog::LoaderPopup;
use crate::ui::dialog::MessagePopup;
use crate::ui::dialog::RebasePopup;
use crate::ui::dialog::describe_action;
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

/// Which version of a file an editor is opened on. The editor edits the
/// working copy either way, so reaching another change's version of a
/// file means moving the working copy there first.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum OpenAt {
    /// The file as the working copy has it.
    WorkingCopy,
    /// The file as this change has it, which is checked out to get at.
    Checkout(CommitId),
    /// The file as this change has it, reached by a new change on top of
    /// it, for a change that cannot be edited itself.
    NewOnTop(CommitId),
}

/// The change the set-bookmark dialog was opened for, which is all it
/// takes to put it back up.
pub struct BookmarkSetDialog {
    pub config: JjConfig,
    pub change_id: Option<ChangeId>,
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
    Push(PushTarget),
    /// Ask what the push would do, and put the answer as the question
    /// whether to go ahead with it.
    PreviewPush(PushTarget),
    Fetch {
        all_remotes: bool,
    },
    /// Take the repo back to the state this operation left it in.
    RestoreOperation(OperationId),
    /// Take back this one operation, leaving what came after it in place.
    RevertOperation(OperationId),
    RestoreFile(File),
    UntrackFile(File),
    /// Open the file in the configured editor, on the version of it
    /// `at` names.
    OpenFile {
        file: File,
        at: OpenAt,
    },
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
        /// What the set-bookmark dialog needs to come back up when the
        /// name is refused, for the asking that goes through it.
        dialog: Option<Box<BookmarkSetDialog>>,
    },
    DeleteBookmark(String),
    ForgetBookmark(String),
    TrackBookmark(Bookmark),
    UntrackBookmark(Bookmark),
    /// Set an option in the user's config, `value` being the TOML
    /// expression to set it to.
    SetSetting {
        key: String,
        value: String,
    },
    /// Take an option out of the user's config, leaving whatever the
    /// rest of the configuration says.
    UnsetSetting {
        key: String,
    },
}

impl Command {
    /// Run the operation, returning what the app is to show for it.
    pub fn run(self, background_tasks: &BackgroundTasks) -> Result<Option<AppAction>> {
        match self {
            Command::Copy(text) => {
                let _ = execute!(std::io::stdout(), CopyToClipboard::to_clipboard_from(text));
                Ok(None)
            }
            Command::Duplicate(revset) => match new_commander().run_duplicate(revset) {
                Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                Err(err) => Ok(Some(refused("Duplicate", err))),
            },
            Command::Absorb(head) => match new_commander().run_absorb(&head.commit_id) {
                Ok(()) => Ok(Some(show_change(new_commander().get_head_latest(&head)?))),
                Err(err) => Ok(Some(refused("Absorb", err))),
            },
            Command::New {
                revset,
                source,
                insert,
                describe,
            } => {
                // Inserting can hit immutable changes, so the marks are
                // left for another attempt.
                if let Err(err) = new_commander().run_new_with_insert(revset, insert) {
                    return Ok(Some(refused("New", err)));
                }

                let head = new_commander().get_current_head()?;
                let mut actions = vec![show_change(head.clone())];
                if source == NewSource::Marks {
                    actions.push(AppAction::ClearLogMarks);
                }
                if describe {
                    actions.push(describe_action(&head, || Ok(vec![]))?);
                }

                Ok(Some(AppAction::Multiple(actions)))
            }
            Command::Squash {
                target,
                ignore_immutable,
            } => match new_commander().run_squash(&target.commit_id, ignore_immutable) {
                Ok(()) => Ok(Some(show_change(new_commander().get_current_head()?))),
                Err(err) => Ok(Some(refused("Squash", err))),
            },
            Command::Edit {
                revset,
                ignore_immutable,
            } => match new_commander().run_edit(revset, ignore_immutable) {
                Ok(()) => Ok(Some(show_change(new_commander().get_current_head()?))),
                Err(err) => Ok(Some(refused("Edit", err))),
            },
            Command::Abandon { marked, selected } => {
                let (revset, abandoned) = match Revset::union(&marked) {
                    Some(revset) => (revset, marked.as_slice()),
                    None => (
                        Revset::from(&selected.commit_id),
                        std::slice::from_ref(&selected.commit_id),
                    ),
                };

                // A tab following a change that is gone falls back to the
                // working copy, which may be nowhere near what was being
                // read, so take the selection to the parent instead.
                let mut moved_to = selected.clone();
                while abandoned.contains(&moved_to.commit_id) {
                    moved_to = new_commander().get_commit_parent(&moved_to.commit_id)?;
                }

                if let Err(err) = new_commander().run_abandon(revset) {
                    return Ok(Some(refused("Abandon", err)));
                }

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
                match new_commander().run_describe(&head.commit_id, &description) {
                    Ok(()) => Ok(Some(AppAction::Multiple(vec![
                        AppAction::ClosePopup,
                        AppAction::ViewLog(new_commander().get_head_latest(&head)?),
                        AppAction::MarkTabsStale,
                    ]))),
                    // Put the editor back with what was written, since a
                    // refused description is one to correct rather than
                    // one to lose.
                    Err(err) => Ok(Some(AppAction::SetPopup(Box::new(DescribePopup::refused(
                        head,
                        description,
                        err,
                    ))))),
                }
            }
            Command::Rebase {
                source,
                source_mode,
                target,
                target_mode,
            } => match new_commander().run_rebase(
                source_mode,
                &source.commit_id,
                target_mode,
                &target.commit_id,
            ) {
                Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                Err(err) => Ok(Some(refused("Rebase", err))),
            },
            Command::Push(target) => Ok(Some(with_loader(
                background_tasks,
                "Pushing",
                TaskSlot::GitPush,
                move || Ok(new_commander().git_push(&target, false)?),
            ))),
            Command::PreviewPush(target) => {
                let asked = target.clone();
                let popup = loader(
                    background_tasks,
                    "Previewing the push",
                    TaskSlot::GitPushDryRun,
                    move || Ok(new_commander().git_push(&target, true)?),
                )
                .on_output(move |output| ask_push(asked, output));

                Ok(Some(AppAction::SetPopup(Box::new(popup))))
            }
            Command::Fetch { all_remotes } => Ok(Some(with_loader(
                background_tasks,
                "Fetching",
                TaskSlot::GitFetch,
                move || Ok(new_commander().git_fetch(all_remotes)?),
            ))),
            Command::RestoreOperation(id) => match new_commander().run_op_restore(&id) {
                Ok(()) => Ok(Some(repo_moved()?)),
                Err(err) => Ok(Some(refused("Restore", err))),
            },
            Command::RevertOperation(id) => match new_commander().run_op_revert(&id) {
                Ok(()) => Ok(Some(repo_moved()?)),
                Err(err) => Ok(Some(refused("Revert", err))),
            },
            Command::RestoreFile(file) => match new_commander().restore_file(&file) {
                Ok(_) => Ok(Some(show_working_copy_files()?)),
                Err(err) => Ok(Some(refused("Restore", err))),
            },
            // This works even for deleted files, as jj does not fail on
            // those.
            Command::UntrackFile(file) => match new_commander().untrack_file(&file) {
                Ok(_) => Ok(Some(show_working_copy_files()?)),
                Err(err) => Ok(Some(refused("Untrack", err))),
            },
            Command::OpenFile { file, at } => open_file(&file, &at),
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
            Command::SetBookmark {
                name,
                commit_id,
                dialog,
            } => match new_commander().set_bookmark_commit(&name, &commit_id) {
                Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                // Put the question back with the name that was refused,
                // which is usually one to correct rather than one to
                // give up on.
                Err(err) => Ok(Some(match dialog {
                    Some(dialog) => AppAction::SetPopup(Box::new(BookmarkSetPopup::refused(
                        dialog.config,
                        dialog.change_id,
                        commit_id,
                        name,
                        err,
                    ))),
                    None => refused("Set bookmark", err),
                })),
            },
            Command::DeleteBookmark(name) => match new_commander().delete_bookmark(&name) {
                Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                Err(err) => Ok(Some(refused("Delete", err))),
            },
            Command::ForgetBookmark(name) => match new_commander().forget_bookmark(&name) {
                Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                Err(err) => Ok(Some(refused("Forget", err))),
            },
            Command::TrackBookmark(bookmark) => match new_commander().track_bookmark(&bookmark) {
                Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                Err(err) => Ok(Some(refused("Track", err))),
            },
            Command::UntrackBookmark(bookmark) => {
                match new_commander().untrack_bookmark(&bookmark) {
                    Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                    Err(err) => Ok(Some(refused("Untrack", err))),
                }
            }
            Command::SetSetting { key, value } => {
                match new_commander().set_user_config(&key, &value) {
                    Ok(()) => Ok(Some(AppAction::ConfigChanged)),
                    Err(err) => Ok(Some(refused("Set", err))),
                }
            }
            Command::UnsetSetting { key } => match new_commander().unset_user_config(&key) {
                Ok(()) => Ok(Some(AppAction::ConfigChanged)),
                Err(err) => Ok(Some(refused("Unset", err))),
            },
        }
    }
}

/// Open `file` in the configured editor, having taken the working copy
/// to where `at` says the version to open is.
fn open_file(file: &File, at: &OpenAt) -> Result<Option<AppAction>> {
    let Some(path) = file.path.as_deref() else {
        return Ok(Some(message("Open", "The line names no file to open.")));
    };
    // Whatever is in the way of opening the file is in the way before the
    // working copy has been taken anywhere for it.
    let Some(editor) = get_env().jj_config.editor() else {
        return Ok(Some(message(
            "Open",
            "There is no editor to open the file in. Set `blazingjj.editor`, \
             or `VISUAL` or `EDITOR` in the environment.",
        )));
    };

    let moved = match at {
        OpenAt::WorkingCopy => None,
        OpenAt::Checkout(commit_id) => match new_commander().run_edit(commit_id, false) {
            Ok(()) => Some(show_change(new_commander().get_current_head()?)),
            Err(err) => return Ok(Some(refused("Edit", err))),
        },
        OpenAt::NewOnTop(commit_id) => {
            match new_commander().run_new_with_insert(commit_id, NewInsertMode::Child) {
                Ok(()) => Some(show_change(new_commander().get_current_head()?)),
                Err(err) => return Ok(Some(refused("New", err))),
            }
        }
    };

    let opened = open_in_editor(&editor, path);

    Ok(match moved {
        Some(moved) => Some(AppAction::Multiple(
            [moved].into_iter().chain(opened).collect(),
        )),
        None => opened,
    })
}

/// Open the working copy's `path` in `editor`, either with the terminal
/// handed over to it or left running on its own, as the configuration
/// says.
fn open_in_editor(editor: &Editor, path: &str) -> Option<AppAction> {
    let env = get_env();
    let program = Program::new(editor.program(), env.root.clone()).args(editor.args(path));

    match env.jj_config.editor_mode() {
        EditorMode::Terminal => Some(AppAction::RunInteractive(Interactive {
            program,
            // The editor leaves the file it edited on the screen, which
            // is not something to read once it is closed.
            hold_screen: false,
        })),
        EditorMode::Detached => match program.run_detached() {
            Ok(()) => None,
            Err(err) => Some(refused("Open", err)),
        },
    }
}

/// Asking where to open `file`, which is shown at `head` rather than in
/// the working copy: what is on disk now, or what that change has, which
/// means taking the working copy there first. Only the versions there
/// are to open are offered.
pub fn ask_open_file(config: JjConfig, head: &Head, file: &File) -> AppAction {
    let open = |at| {
        AppAction::Run(Command::OpenFile {
            file: file.clone(),
            at,
        })
    };

    let Some(path) = file.path.as_deref() else {
        return message("Open", "The line names no file to open.");
    };

    let mut items = Vec::new();
    if in_working_copy(path) {
        items.push((
            Line::raw("Open the file as the working copy has it"),
            open(OpenAt::WorkingCopy),
        ));
    }
    if file.diff_type != Some(DiffType::Deleted) {
        if !head.immutable {
            items.push((
                Line::raw("Check this change out and open the file there"),
                open(OpenAt::Checkout(head.commit_id.clone())),
            ));
        }
        items.push((
            Line::raw("Create a change on top of it and open the file there"),
            open(OpenAt::NewOnTop(head.commit_id.clone())),
        ));
    }

    if items.is_empty() {
        return message("Open", "There is no version of the file to open.");
    }

    AppAction::SetPopup(Box::new(ChoicePopup::new(config, None, "Open", items)))
}

/// Whether the working copy has `path`. A file another change added or
/// deleted is not there to open, however the change shows it. A symlink
/// is there whether or not what it points at is.
fn in_working_copy(path: &str) -> bool {
    Path::new(&get_env().root)
        .join(path)
        .symlink_metadata()
        .is_ok()
}

/// Asking for a new change from the marked changes, or from `selected`
/// when none are marked.
pub fn ask_new_change_from_selection(
    config: JjConfig,
    selected: &Head,
    marked: &[CommitId],
    describe: bool,
) -> AppAction {
    let target = if marked.is_empty() {
        selected.change_id.as_str().chars().take(8).collect()
    } else {
        format!("the {} marked changes", marked.len())
    };
    let revset = Revset::union(marked).unwrap_or_else(|| Revset::from(&selected.commit_id));
    let source = if marked.is_empty() {
        NewSource::Change
    } else {
        NewSource::Marks
    };

    ask_new_change(config, revset, source, &target, describe)
}

/// Asking for a new change from the one a bookmark points at.
pub fn ask_new_change_from_bookmark(
    config: JjConfig,
    bookmark: &Bookmark,
    head: &Head,
    describe: bool,
) -> AppAction {
    ask_new_change(
        config,
        Revset::from(&head.commit_id),
        NewSource::Change,
        &bookmark.to_string(),
        describe,
    )
}

/// Asking to see the files of one version of `change`. The newest
/// version is the change as it stands, so the files tab may as well keep
/// up with it.
pub fn show_version_files(version: &Head, change: &Head) -> AppAction {
    if version.commit_id == change.commit_id {
        AppAction::ViewFiles(version.clone())
    } else {
        AppAction::ViewVersionFiles(version.clone())
    }
}

/// Asking to describe `head`: the refusal when it is immutable, or the
/// editor with what it says now.
pub fn describe(head: &Head) -> Result<AppAction> {
    if head.immutable {
        return Ok(message(
            "Describe",
            "The change cannot be described because it is immutable.",
        ));
    }

    describe_action(head, || {
        Ok(new_commander()
            .get_commit_description(&head.commit_id)?
            .split('\n')
            .map(str::to_owned)
            .collect())
    })
}

/// Asking to rebase the working copy commit onto `destination`.
pub fn rebase(destination: &Head) -> Result<AppAction> {
    Ok(AppAction::SetPopup(Box::new(RebasePopup::new(
        new_commander().get_current_head()?,
        destination.clone(),
    ))))
}

/// Asking whether to send the push `preview` describes, that being what
/// jj answered when asked what it would do.
fn ask_push(target: PushTarget, preview: String) -> AppAction {
    // That the push has not happened is what the question is about, so
    // jj saying so as well only makes the answer harder to find.
    let preview: String = preview
        .lines()
        .filter(|line| !line.contains("Dry-run requested"))
        .fold(String::new(), |text, line| text + line + "\n");
    if preview.trim().is_empty() {
        return message("Push", "jj said nothing about what this push would do");
    }
    // A push with nothing to send is nothing to answer for, so it is
    // reported rather than asked about. Whatever jj had to say about why
    // stands as the report.
    if preview.contains("Nothing changed") {
        return AppAction::SetPopup(Box::new(
            MessagePopup::new("Push", preview).text_align(Alignment::Left),
        ));
    }

    // The preview is laid out as jj wrote it, colors and indentation
    // included.
    let text = preview.into_text().ok();
    let preview = text.unwrap_or_else(|| Text::raw(preview));

    let mut question = Text::from("The push would perform the following actions:");
    question.push_line("");
    question.extend(preview.lines);
    question.push_line("");
    question.push_line("Do you want to push?");

    AppAction::SetPopup(Box::new(ConfirmPopup::new(
        get_env().jj_config.clone(),
        "Push Preview",
        question,
        AppAction::Run(Command::Push(target)),
    )))
}

/// Pushing what `scope` says of `selected`. Pushing the new bookmarks of
/// a change means naming them, as jj only tracks a bookmark the remote
/// does not have yet when it is asked for by name.
pub fn push(selected: &Head, scope: PushScope) -> AppAction {
    let revset = Revset::from(&selected.commit_id);
    let target = match scope {
        // Both of these send the bookmarks the change has, so neither
        // has anything to send when it has none.
        PushScope::Selected | PushScope::SelectedWithNew => {
            let bookmarks = match new_commander().get_local_bookmarks(&revset) {
                Ok(bookmarks) => bookmarks,
                Err(err) => return refused("Push", err),
            };
            if bookmarks.is_empty() {
                return message("Push", "This change has no bookmark to push");
            }

            if scope == PushScope::Selected {
                PushTarget::Revision(revset)
            } else {
                PushTarget::Bookmarks(bookmarks.into_iter().map(|it| it.name).collect())
            }
        }
        PushScope::Tracked => PushTarget::Tracked,
        PushScope::All => PushTarget::All,
        PushScope::Change => PushTarget::Change(revset),
        // The name is the user's to give, so the push waits for it.
        PushScope::Named => {
            return AppAction::SetPopup(Box::new(BookmarkNamePopup::new_push(revset)));
        }
    };

    AppAction::Run(push_command(target))
}

/// Sending `target`, shown and asked about first unless that is turned
/// off.
pub fn push_command(target: PushTarget) -> Command {
    if get_env().jj_config.confirm_push() {
        Command::PreviewPush(target)
    } else {
        Command::Push(target)
    }
}

/// Asking to put a bookmark on `head`.
pub fn set_bookmark(config: JjConfig, head: &Head) -> AppAction {
    AppAction::SetPopup(Box::new(BookmarkSetPopup::new(
        config,
        Some(head.change_id.clone()),
        head.commit_id.clone(),
    )))
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

/// Asking to edit `target`, which the question names as `subject`: the
/// refusal when it is immutable, or the question that runs it.
pub fn ask_edit(
    config: JjConfig,
    target: &Head,
    subject: String,
    ignore_immutable: bool,
) -> AppAction {
    if target.immutable && !ignore_immutable {
        return message(
            "Edit",
            "The change cannot be edited because it is immutable.",
        );
    }

    let mut lines = vec![
        Line::from("Are you sure you want to edit an existing change?"),
        Line::from(subject),
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

/// Asking to take the repo back to the state `operation` left it in.
pub fn ask_op_restore(config: JjConfig, operation: &Operation) -> AppAction {
    confirm(
        config,
        "Restore",
        Text::from(vec![
            Line::from("Are you sure you want to restore the repo to this operation?"),
            Line::from(name_of(operation)),
        ]),
        Command::RestoreOperation(operation.id.clone()),
    )
}

/// Asking to take back `operation` alone.
pub fn ask_op_revert(config: JjConfig, operation: &Operation) -> AppAction {
    confirm(
        config,
        "Revert",
        Text::from(vec![
            Line::from("Are you sure you want to revert this operation?"),
            Line::from(name_of(operation)),
        ]),
        Command::RevertOperation(operation.id.clone()),
    )
}

/// How a question names an operation: as short as the operation log
/// writes it, and with what it says of itself.
fn name_of(operation: &Operation) -> String {
    format!(
        "Operation: {} {}",
        operation.id.short(),
        operation.description
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

/// Asking to put `bookmark` on `head`, which for one of several targets
/// is what settles it on that one.
pub fn ask_set_bookmark(config: JjConfig, bookmark: &Bookmark, head: &Head) -> AppAction {
    confirm(
        config,
        "Set",
        Text::from(vec![
            Line::from(format!(
                "Are you sure you want to move the {} bookmark?",
                bookmark.name
            )),
            Line::from(format!("Onto: {}", head.change_id.as_str())),
        ]),
        Command::SetBookmark {
            name: bookmark.name.clone(),
            commit_id: head.commit_id.clone(),
            dialog: None,
        },
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

/// The repo is somewhere else altogether, an operation of the operation
/// log having been taken back. Every tab is behind, and the change the
/// working copy is on is not the one it was.
fn repo_moved() -> Result<AppAction> {
    Ok(AppAction::Multiple(vec![
        AppAction::ChangeHead(new_commander().get_current_head()?),
        AppAction::MarkTabsStale,
    ]))
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

/// What jj said when it would not do what `operation` asked. Its answer
/// is laid out as it wrote it, being several lines as often as not.
fn refused(operation: &'static str, err: impl Display) -> AppAction {
    AppAction::SetPopup(Box::new(
        MessagePopup::new(operation, format!("{err:#}")).text_align(Alignment::Left),
    ))
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
    AppAction::SetPopup(Box::new(loader(
        background_tasks,
        operation_name,
        slot,
        operation,
    )))
}

/// The loader popup [with_loader] puts up, for a caller that has more to
/// say about what becomes of the output.
fn loader<F>(
    background_tasks: &BackgroundTasks,
    operation_name: &str,
    slot: TaskSlot,
    operation: F,
) -> LoaderPopup
where
    F: FnOnce() -> TaskOutput + Send + 'static,
{
    background_tasks.submit_uninterruptible(slot.clone(), operation);

    LoaderPopup::new(operation_name.to_owned(), slot)
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::commander::ids::ChangeId;
    use crate::env::set_test_env;

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
        says_where(&rows(action), text)
    }

    fn says_where(rows: &[String], text: &str) -> bool {
        rows.iter().any(|row| row.contains(text))
    }

    /// What jj answers when asked what a push would do
    const PREVIEW: &str = "Changes to push to origin:\n  Add bookmark here to 0123abcd\nDry-run requested, not pushing.\n";

    #[test]
    fn the_push_question_holds_what_jj_said_the_push_would_do() {
        set_test_env();

        let rows = rows(ask_push(PushTarget::Tracked, PREVIEW.to_owned()));

        assert!(says_where(&rows, "Push Preview"), "{rows:?}");
        assert!(
            says_where(&rows, "would perform the following actions"),
            "{rows:?}"
        );
        assert!(says_where(&rows, "Changes to push to origin:"), "{rows:?}");
        assert!(says_where(&rows, "Do you want to push?"), "{rows:?}");
        // The lines jj indents stay indented, past the blank column the
        // popup pads them with.
        assert!(says_where(&rows, "│   Add bookmark here"), "{rows:?}");
        // That the push has not happened is what is being asked about.
        assert!(!says_where(&rows, "Dry-run requested"), "{rows:?}");
    }

    #[test]
    fn a_push_with_nothing_to_send_is_reported_rather_than_asked_about() {
        set_test_env();

        let rows = rows(ask_push(
            PushTarget::Tracked,
            "Warning: No bookmarks point to the specified revisions: @\nNothing changed.\n"
                .to_owned(),
        ));

        assert!(says_where(&rows, "No bookmarks point to"), "{rows:?}");
        assert!(!says_where(&rows, "Do you want to push?"), "{rows:?}");
    }

    #[test]
    fn a_push_jj_says_nothing_about_is_not_asked_about() {
        set_test_env();

        assert!(says(
            ask_push(
                PushTarget::Tracked,
                "Dry-run requested, not pushing.\n".to_owned()
            ),
            "jj said nothing"
        ));
    }

    /// A file the working copy does not have is one there is nothing to
    /// open at `@`, whatever the change being shown says about it.
    #[test]
    fn only_a_file_the_working_copy_has_is_offered_as_it_is() {
        set_test_env();
        let opening = |path: &str| {
            ask_open_file(
                JjConfig::default(),
                &head("a", false),
                &File {
                    line: format!("M {path}"),
                    path: Some(path.to_owned()),
                    diff_type: Some(DiffType::Modified),
                },
            )
        };

        assert!(says(opening("Cargo.toml"), "as the working copy has it"));
        assert!(!says(opening("gone.txt"), "as the working copy has it"));
    }

    /// An immutable change cannot be checked out to edit, so the only way
    /// to the file is a change of one's own on top of it.
    #[test]
    fn an_immutable_change_is_not_offered_for_checking_out() {
        set_test_env();
        let opening = |immutable| {
            ask_open_file(
                JjConfig::default(),
                &head("a", immutable),
                &File {
                    line: "M Cargo.toml".to_owned(),
                    path: Some("Cargo.toml".to_owned()),
                    diff_type: Some(DiffType::Modified),
                },
            )
        };

        assert!(says(opening(false), "Check this change out"));
        assert!(!says(opening(true), "Check this change out"));
        assert!(says(opening(true), "Create a change on top of it"));
    }

    /// Taking the working copy to a change that deletes the file leaves
    /// nothing to open there, so neither way of getting to that version
    /// is one to offer.
    #[test]
    fn a_file_the_change_deletes_is_not_offered_at_that_change() {
        set_test_env();
        let opening = |path: &str| {
            ask_open_file(
                JjConfig::default(),
                &head("a", false),
                &File {
                    line: format!("D {path}"),
                    path: Some(path.to_owned()),
                    diff_type: Some(DiffType::Deleted),
                },
            )
        };

        assert!(!says(opening("Cargo.toml"), "Check this change out"));
        assert!(!says(opening("Cargo.toml"), "Create a change on top of it"));
        assert!(says(opening("Cargo.toml"), "as the working copy has it"));
        assert!(says(
            opening("gone.txt"),
            "There is no version of the file to open"
        ));
    }

    /// A line naming no file is one to say so about rather than one to
    /// take the working copy anywhere for.
    #[test]
    fn a_line_naming_no_file_moves_the_working_copy_nowhere() -> Result<()> {
        set_test_env();
        let file = File {
            line: "Some other line".to_owned(),
            path: None,
            diff_type: None,
        };

        assert!(says(
            ask_open_file(JjConfig::default(), &head("a", false), &file),
            "names no file to open"
        ));

        let action = open_file(&file, &OpenAt::Checkout(CommitId("commit-a".to_owned())))?
            .expect("the line is refused");
        assert!(says(action, "names no file to open"));

        Ok(())
    }

    #[test]
    fn an_immutable_change_is_refused_rather_than_asked_about() {
        assert!(says(
            ask_edit(
                JjConfig::default(),
                &head("a", true),
                "Change: a".to_owned(),
                false,
            ),
            "because it is immutable"
        ));
    }

    #[test]
    fn an_immutable_change_is_asked_about_when_immutability_is_ignored() {
        assert!(says(
            ask_edit(
                JjConfig::default(),
                &head("a", true),
                "Change: a".to_owned(),
                true,
            ),
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
    fn taking_an_operation_back_names_it_as_the_operation_log_does() {
        let operation = Operation {
            id: OperationId("0123456789abcdef".to_owned()),
            description: "describe commit".to_owned(),
            current: false,
            root: false,
        };

        assert!(says(
            ask_op_restore(JjConfig::default(), &operation),
            "Operation: 0123456789ab describe commit"
        ));
        assert!(says(
            ask_op_revert(JjConfig::default(), &operation),
            "Operation: 0123456789ab describe commit"
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
