/*! The operations against the repo, each naming both what to do and
what to do it to, so that whatever asks for one does not have to be the
component that holds the selection it acts on.

Only the app runs them. What it is to show once one is done comes back
as an [AppAction].
*/

use std::fmt::Display;
use std::path::Path;

use anyhow::Result;
use ratatui::crossterm::clipboard::CopyToClipboard;
use ratatui::crossterm::execute;
use ratatui::layout::Alignment;
use ratatui::layout::Position;
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
use crate::commander::is_stale_working_copy;
use crate::commander::jj::NewInsertMode;
use crate::commander::jj::Push;
use crate::commander::jj::PushTarget;
use crate::commander::jj::RebaseSource;
use crate::commander::jj::RebaseTarget;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::commander::operation::Operation;
use crate::commander::program::Program;
use crate::commander::revset::Revset;
use crate::commander::workspace::MoveTarget;
use crate::commander::workspace::Workspace;
use crate::commands::CustomCommand;
use crate::commands::CustomRun;
use crate::env::Editor;
use crate::env::EditorMode;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::keybinds::PushScope;
use crate::selection::Selection;
use crate::theme::Role;
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
use crate::ui::dialog::RebaseSources;
use crate::ui::dialog::WorkspaceMode;
use crate::ui::dialog::WorkspacePopup;
use crate::ui::dialog::describe_action;
use crate::ui::dialog::new_insert;
use crate::ui::dialog::push_menu;
use crate::ui::styles::AnsiText;

/// What an operation acts on: the changes the log has marked, or what
/// it falls back to when none are. Which of the two it is decides
/// whether the log is done marking them once the operation has gone
/// through.
#[derive(Clone)]
pub struct ActsOn {
    changes: Revset,
    marked: bool,
}

impl ActsOn {
    /// The changes the log has marked.
    pub fn marked(changes: impl Into<Revset>) -> Self {
        Self {
            changes: changes.into(),
            marked: true,
        }
    }

    /// A change the log has not marked.
    pub fn change(changes: impl Into<Revset>) -> Self {
        Self {
            changes: changes.into(),
            marked: false,
        }
    }

    /// The union of the marked changes, or `fallback` when none are
    /// marked.
    pub fn marked_or(marked: &[CommitId], fallback: impl Into<Revset>) -> Self {
        Revset::union(marked).map_or_else(|| Self::change(fallback), Self::marked)
    }

    /// The changes to act on, and what the log is to do once the
    /// operation has gone through.
    fn into_parts(self) -> (Revset, Option<AppAction>) {
        (self.changes, marks_taken(self.marked))
    }
}

/// What the log is to do once an operation that was handed the marked
/// changes has gone through: it is done marking them.
fn marks_taken(marked: bool) -> Option<AppAction> {
    marked.then_some(AppAction::ClearLogMarks)
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
    /// The file at a revision, this URL naming both, for an editor that
    /// reads a revision itself and so needs nothing checked out.
    AtRevision(String),
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
    Duplicate(ActsOn),
    Parallelize(ActsOn),
    Absorb(Head),
    /// Create a change from what `acts_on` names, put where `insert`
    /// says.
    New {
        acts_on: ActsOn,
        insert: NewInsertMode,
        describe: bool,
    },
    /// Squash `from`, or the working copy when it is [None], into
    /// `target`.
    Squash {
        from: Option<Revset>,
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
        source: ActsOn,
        source_mode: RebaseSource,
        target: Head,
        target_mode: RebaseTarget,
    },
    Push(Push),
    /// Ask what the push would do, and put the answer as the question
    /// whether to go ahead with it.
    PreviewPush(Push),
    /// Open the push menu for `selected`, pushing to `remote`.
    PushMenu {
        anchor: Option<Position>,
        selected: Head,
        remote: String,
    },
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
    /// Update the working copy jj refuses to read the repo until it is
    /// updated.
    UpdateStaleWorkspace,
    /// Run a command of your own against what the tab it was picked
    /// from had selected, which the placeholders of the command stand
    /// for.
    RunCustom(Box<CustomRun>),
    /// Make a workspace in this directory, named after it.
    AddWorkspace {
        destination: String,
    },
    /// Stop tracking the working-copy commit of the workspace of this
    /// name, leaving what is on disk alone.
    ForgetWorkspace(String),
    /// Rename the workspace whose root is `root`.
    RenameWorkspace {
        root: String,
        new_name: String,
    },
    /// Move the working copy of the workspace whose root is `root` onto
    /// a change, or onto a new child of it, which changes the files in
    /// that directory.
    MoveWorkspace {
        root: String,
        name: String,
        revset: Revset,
        target: MoveTarget,
    },
}

impl Command {
    /// Whether the operation goes to the repo, so that a view the repo
    /// has moved on from is a view it would be carried out against.
    pub fn touches_the_repo(&self) -> bool {
        !matches!(
            self,
            Command::Copy(_)
                | Command::PushMenu { .. }
                | Command::SetSetting { .. }
                | Command::UnsetSetting { .. }
        )
    }

    /// Run the operation, returning what the app is to show for it.
    pub fn run(self, background_tasks: &BackgroundTasks) -> Result<Option<AppAction>> {
        match self {
            Command::Copy(text) => {
                let _ = execute!(std::io::stdout(), CopyToClipboard::to_clipboard_from(text));
                Ok(None)
            }
            Command::Duplicate(acts_on) => {
                let (changes, taken) = acts_on.into_parts();
                match new_commander().run_duplicate(changes) {
                    Ok(()) => Ok(Some(rewritten(taken))),
                    Err(err) => Ok(Some(refused("Duplicate", err))),
                }
            }
            Command::Parallelize(acts_on) => {
                let (changes, taken) = acts_on.into_parts();
                match new_commander().run_parallelize(changes) {
                    Ok(()) => Ok(Some(rewritten(taken))),
                    Err(err) => Ok(Some(refused("Parallelize", err))),
                }
            }
            Command::Absorb(head) => match new_commander().run_absorb(&head.commit_id) {
                Ok(()) => Ok(Some(show_change(new_commander().get_head_latest(&head)?))),
                Err(err) => Ok(Some(refused("Absorb", err))),
            },
            Command::New {
                acts_on,
                insert,
                describe,
            } => {
                // Inserting can hit immutable changes, so the changes stay
                // marked for another attempt, which has to ask for them again.
                let (changes, taken) = acts_on.into_parts();
                if let Err(err) = new_commander().run_new_with_insert(changes, insert) {
                    return Ok(Some(refused("New", err)));
                }

                let head = new_commander().get_current_head()?;
                let mut actions = vec![show_change(head.clone())];
                actions.extend(taken);
                if describe {
                    actions.push(describe_action(&head, || Ok(vec![]))?);
                }

                Ok(Some(AppAction::Multiple(actions)))
            }
            Command::Squash {
                from,
                target,
                ignore_immutable,
            } => {
                // Sources of its own are the marked changes, which the
                // log is done marking once they are folded in.
                let marked = from.is_some();
                match new_commander().run_squash(from, &target.commit_id, ignore_immutable) {
                    // Folding the working copy in moves it, so the view
                    // follows it there; folding the marked changes in
                    // leaves it where it was.
                    Ok(()) if marked => Ok(Some(rewritten(marks_taken(marked)))),
                    Ok(()) => Ok(Some(show_change(new_commander().get_current_head()?))),
                    Err(err) => Ok(Some(refused("Squash", err))),
                }
            }
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
            } => {
                let (changes, taken) = source.into_parts();
                match new_commander().run_rebase(
                    source_mode,
                    changes,
                    target_mode,
                    &target.commit_id,
                ) {
                    Ok(()) => Ok(Some(rewritten(taken))),
                    Err(err) => Ok(Some(refused("Rebase", err))),
                }
            }
            Command::Push(push) => Ok(Some(with_loader(
                background_tasks,
                "Pushing",
                TaskSlot::GitPush,
                move || Ok(new_commander().git_push(&push, false)?),
            ))),
            Command::PreviewPush(push) => {
                let asked = push.clone();
                let popup = loader(
                    background_tasks,
                    "Previewing the push",
                    TaskSlot::GitPushDryRun,
                    move || Ok(new_commander().git_push(&push, true)?),
                )
                .on_output(move |output| ask_push(asked, output));

                Ok(Some(AppAction::SetPopup(Box::new(popup))))
            }
            Command::PushMenu {
                anchor,
                selected,
                remote,
            } => Ok(Some(AppAction::SetPopup(Box::new(push_menu(
                anchor,
                &selected,
                Some(remote),
            )?)))),
            Command::Fetch { all_remotes } => Ok(Some(with_loader(
                background_tasks,
                "Fetching",
                TaskSlot::GitFetch,
                move || Ok(new_commander().git_fetch(all_remotes)?),
            ))),
            Command::UpdateStaleWorkspace => match new_commander().update_stale_workspace() {
                // The working copy is where the repo says it is now,
                // which is not where any tab last read it.
                Ok(_) => Ok(Some(repo_moved()?)),
                Err(err) => Ok(Some(refused("Update", err))),
            },
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
            Command::RunCustom(run) => {
                Ok(Some(run_custom(&run.name, &run.command, &run.selection)))
            }
            Command::AddWorkspace { destination } => {
                match new_commander().run_workspace_add(&destination) {
                    // The new workspace holds a change of its own, so
                    // the repo is not where any tab last read it.
                    Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                    // Put the question back up with what was typed.
                    Err(err) => Ok(Some(AppAction::SetPopup(Box::new(
                        WorkspacePopup::refused(WorkspaceMode::Add, destination, err),
                    )))),
                }
            }
            Command::ForgetWorkspace(name) => match new_commander().run_workspace_forget(&name) {
                Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                Err(err) => Ok(Some(refused("Forget", err))),
            },
            Command::RenameWorkspace { root, new_name } => {
                match new_commander().run_workspace_rename(&root, &new_name) {
                    Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                    Err(err) => Ok(Some(AppAction::SetPopup(Box::new(
                        WorkspacePopup::refused(WorkspaceMode::Rename { root }, new_name, err),
                    )))),
                }
            }
            Command::MoveWorkspace {
                root,
                name,
                revset,
                target,
            } => match new_commander().run_workspace_move(&root, revset, target) {
                Ok(()) => Ok(Some(AppAction::MarkTabsStale)),
                Err(err) if is_stale_working_copy(&err) => Ok(Some(message(
                    "Move workspace",
                    format!(
                        "The working copy of the {name} workspace is stale, so jj reads \
                         nothing in {root}. Work in that workspace to update it, and move \
                         it from there."
                    ),
                ))),
                Err(err) => Ok(Some(refused("Move workspace", err))),
            },
        }
    }
}

/// Run `command`, which is configured as `name`, against `selection`:
/// with the terminal handed over to it, or with what it writes captured
/// and put up.
fn run_custom(name: &str, command: &CustomCommand, selection: &Selection) -> AppAction {
    let said = |text: String| {
        AppAction::SetPopup(Box::new(
            MessagePopup::new(name.to_owned(), text).text_align(Alignment::Left),
        ))
    };

    let (program, uses_marks) = match command.program(selection) {
        Ok(program) => program,
        Err(missing) => {
            return said(format!("Nothing to run the command against\n\n{missing}"));
        }
    };

    if command.is_interactive() {
        return AppAction::RunInteractive(Interactive {
            program,
            hold_screen: true,
            on_success: uses_marks
                .then_some(AppAction::ClearLogMarks)
                .into_iter()
                .collect(),
        });
    }

    let mut ran = true;
    let report = match program.run_captured() {
        Ok(written) if written.trim().is_empty() => None,
        Ok(written) => Some(written),
        Err(err) => {
            ran = false;
            Some(format!("Failed to run the command\n\n{err}"))
        }
    };
    // The command may have moved the repo whatever it had to say about
    // it, so the tabs read it again either way.
    let mut actions = vec![AppAction::MarkTabsStale];
    if ran && uses_marks {
        actions.push(AppAction::ClearLogMarks);
    }
    actions.extend(report.map(said));

    AppAction::Multiple(actions)
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

    let (moved, target) = match at {
        OpenAt::WorkingCopy => (None, path),
        OpenAt::Checkout(commit_id) => match new_commander().run_edit(commit_id, false) {
            Ok(()) => (Some(show_change(new_commander().get_current_head()?)), path),
            Err(err) => return Ok(Some(refused("Edit", err))),
        },
        OpenAt::NewOnTop(commit_id) => {
            match new_commander().run_new_with_insert(commit_id, NewInsertMode::Child) {
                Ok(()) => (Some(show_change(new_commander().get_current_head()?)), path),
                Err(err) => return Ok(Some(refused("New", err))),
            }
        }
        // The editor reads the revision itself, so the working copy
        // stays where it is and the URL goes where the file would.
        OpenAt::AtRevision(url) => (None, url.as_str()),
    };

    let opened = open_in_editor(&editor, target);

    Ok(match moved {
        Some(moved) => Some(AppAction::Multiple(
            [moved].into_iter().chain(opened).collect(),
        )),
        None => opened,
    })
}

/// Open `target`, a file of the working copy or a URL naming one at a
/// revision, in `editor`, either with the terminal handed over to it or
/// left running on its own, as the configuration says.
fn open_in_editor(editor: &Editor, target: &str) -> Option<AppAction> {
    let env = get_env();
    let program = Program::new(editor.program(), env.root.clone()).args(editor.args(target));

    match env.jj_config.editor_mode() {
        EditorMode::Terminal => Some(AppAction::RunInteractive(Interactive {
            program,
            // The editor leaves the file it edited on the screen, which
            // is not something to read once it is closed.
            hold_screen: false,
            on_success: Vec::new(),
        })),
        EditorMode::Detached => match program.run_detached() {
            Ok(()) => None,
            Err(err) => Some(refused("Open", err)),
        },
    }
}

/// Asking where to open `file`, which is shown at `head`, named
/// `revision` to an editor, rather than in the working copy: what is on
/// disk now, what that change has, which means taking the working copy
/// there first, or, for an editor that reads a revision itself, the file
/// at `revision`. Only the versions there are to open are offered.
pub fn ask_open_file(config: JjConfig, head: &Head, revision: &str, file: &File) -> AppAction {
    let open = |at| {
        AppAction::Run(Command::OpenFile {
            file: file.clone(),
            at,
        })
    };

    let Some(path) = file.path.as_deref() else {
        return message("Open", "The line names no file to open.");
    };

    // The change has no version of a file it deletes, however the tab
    // shows it.
    let at_change = file.diff_type != Some(DiffType::Deleted);

    let mut items = Vec::new();
    // An editor that reads the revision itself gets at the file without
    // anything being checked out, which makes it the first thing to
    // offer wherever it is configured.
    if at_change && let Some(url) = config.editor_url(revision, path) {
        items.push((
            Line::raw("Open the file at this revision"),
            open(OpenAt::AtRevision(url)),
        ));
    }
    if in_working_copy(path) {
        items.push((
            Line::raw("Open the file as the working copy has it"),
            open(OpenAt::WorkingCopy),
        ));
    }
    if at_change {
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

    AppAction::SetPopup(Box::new(ChoicePopup::new(None, "Open", items)))
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
    selected: &Head,
    marked: &[CommitId],
    describe: bool,
) -> AppAction {
    let target = if marked.is_empty() {
        selected.change_id.as_str().chars().take(8).collect()
    } else {
        format!("the {} marked changes", marked.len())
    };
    let acts_on = ActsOn::marked_or(marked, &selected.commit_id);

    ask_new_change(acts_on, &target, describe)
}

/// Asking for a new change from the one a bookmark points at.
pub fn ask_new_change_from_bookmark(bookmark: &Bookmark, head: &Head, describe: bool) -> AppAction {
    ask_new_change(
        ActsOn::change(&head.commit_id),
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

/// Parallelizing the marked changes, or the refusal when there are
/// fewer than two of them to take apart from one another.
pub fn parallelize(marked: &[CommitId]) -> AppAction {
    if marked.len() < 2 {
        return message(
            "Parallelize",
            "Parallelizing acts on more than one marked change, once the marks are asked for",
        );
    }
    let changes = Revset::union(marked).expect("changes to unite");

    AppAction::Run(Command::Parallelize(ActsOn::marked(changes)))
}

/// Asking to rebase `sources`, or the working copy commit when none are
/// marked, onto `destination`.
pub fn rebase(marked: &[CommitId], destination: &Head) -> Result<AppAction> {
    // Marking the change being moved onto says to move the others onto
    // it, there being nowhere else it could go.
    let sources: Vec<_> = marked
        .iter()
        .filter(|source| **source != destination.commit_id)
        .cloned()
        .collect();
    if sources.is_empty() && !marked.is_empty() {
        return Ok(message("Rebase", "Cannot rebase a change onto itself"));
    }

    let sources = match Revset::union(&sources) {
        Some(changes) => RebaseSources::Marked {
            changes,
            count: sources.len(),
        },
        None => RebaseSources::WorkingCopy(new_commander().get_current_head()?),
    };

    Ok(AppAction::SetPopup(Box::new(RebasePopup::new(
        sources,
        destination.clone(),
    ))))
}

/// Asking whether to send the push `preview` describes, that being what
/// jj answered when asked what it would do.
fn ask_push(push: Push, preview: String) -> AppAction {
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
    let text = preview.owned_ansi_text().ok();
    let preview = text.unwrap_or_else(|| Text::raw(preview));

    let mut question = Text::from("The push would perform the following actions:");
    question.push_line("");
    question.extend(preview.lines);
    question.push_line("");
    question.push_line("Do you want to push?");

    AppAction::SetPopup(Box::new(ConfirmPopup::new(
        "Push Preview",
        question,
        AppAction::Run(Command::Push(push)),
    )))
}

/// Pushing what `scope` says of `selected` to `remote`, or to jj's default
/// remote when [None]. Pushing the new bookmarks of a change means naming
/// them, as jj only tracks a bookmark the remote does not have yet when it
/// is asked for by name.
pub fn push(selected: &Head, scope: PushScope, remote: Option<String>) -> AppAction {
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
        // The revset names the remote, so jj's default one cannot be left
        // to jj.
        PushScope::Ancestors => {
            let remote = match remote {
                Some(remote) => remote,
                None => {
                    let commander = new_commander();
                    match commander.git_remotes() {
                        Ok(remotes) => commander.default_push_remote(&remotes),
                        Err(err) => return refused("Push", err),
                    }
                }
            };
            let target = PushTarget::Revision(Revset::unpushed(&remote, &selected.commit_id));
            return AppAction::Run(push_command(Push {
                target,
                remote: Some(remote),
            }));
        }
        PushScope::Tracked => PushTarget::Tracked,
        PushScope::All => PushTarget::All,
        PushScope::Change => PushTarget::Change(revset),
        // The name is the user's to give, so the push waits for it.
        PushScope::Named => {
            return AppAction::SetPopup(Box::new(BookmarkNamePopup::new_push(revset, remote)));
        }
    };

    AppAction::Run(push_command(Push { target, remote }))
}

/// Sending `push`, shown and asked about first unless that is turned off.
pub fn push_command(push: Push) -> Command {
    if get_env().jj_config.confirm_push() {
        Command::PreviewPush(push)
    } else {
        Command::Push(push)
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
pub fn ask_new_change(acts_on: ActsOn, target: &str, describe: bool) -> AppAction {
    AppAction::SetPopup(Box::new(new_insert(target, |insert| {
        AppAction::Run(Command::New {
            acts_on: acts_on.clone(),
            insert,
            describe,
        })
    })))
}

/// Asking to squash the marked changes, or the working copy when none
/// are marked, into `selected`: the target it picks, the refusal when
/// that target cannot take it, or the question that runs it.
pub fn ask_squash(
    selected: &Head,
    marked: &[CommitId],
    ignore_immutable: bool,
) -> Result<AppAction> {
    // Marking the change being squashed into says to fold the others
    // into it, there being nowhere else they could go.
    let sources: Vec<_> = marked
        .iter()
        .filter(|source| **source != selected.commit_id)
        .cloned()
        .collect();
    if sources.is_empty() && !marked.is_empty() {
        return Ok(message("Squash", "Cannot squash a change into itself"));
    }

    // Marked sources name themselves, so the selection is the target
    // whatever it is. Squashing the change the working copy is on, on
    // the other hand, has nowhere to go but its parent.
    let from = Revset::union(&sources);
    let (target, question) = if from.is_some() {
        (
            selected.clone(),
            "Are you sure you want to squash the marked changes into this change?",
        )
    } else {
        let at = new_commander().get_current_head()?;
        if selected.change_id == at.change_id {
            match new_commander().get_commit_parent(&at.commit_id) {
                Ok(parent) => (parent, "Are you sure you want to squash @ into its parent?"),
                Err(_) => return Ok(message("Squash", "Cannot squash onto current change")),
            }
        } else {
            (
                selected.clone(),
                "Are you sure you want to squash @ into this change?",
            )
        }
    };

    if target.immutable && !ignore_immutable {
        return Ok(message("Squash", "Cannot squash onto immutable change"));
    }

    let mut lines = vec![
        Line::from(question),
        Line::from(format!("Squash into {}", target.change_id.as_str())),
    ];
    if ignore_immutable {
        lines.push(Line::from("This change is immutable."));
    }

    Ok(confirm(
        "Squash",
        Text::from(lines),
        Command::Squash {
            from,
            target,
            ignore_immutable,
        },
    ))
}

/// Asking to edit `target`, which the question names as `subject`: the
/// refusal when it is immutable, or the question that runs it.
pub fn ask_edit(target: &Head, subject: String, ignore_immutable: bool) -> AppAction {
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
pub fn ask_abandon(selected: &Head, marked: Vec<CommitId>) -> AppAction {
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
        "Abandon",
        text,
        Command::Abandon {
            marked,
            selected: selected.clone(),
        },
    )
}

/// Asking to take the repo back to the state `operation` left it in.
pub fn ask_op_restore(operation: &Operation) -> AppAction {
    confirm(
        "Restore",
        Text::from(vec![
            Line::from("Are you sure you want to restore the repo to this operation?"),
            Line::from(name_of(operation)),
        ]),
        Command::RestoreOperation(operation.id.clone()),
    )
}

/// Asking to take back `operation` alone.
pub fn ask_op_revert(operation: &Operation) -> AppAction {
    confirm(
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
pub fn ask_delete_bookmark(name: &str) -> AppAction {
    confirm(
        "Delete",
        Text::from(format!(
            "Are you sure you want to delete the {name} bookmark?"
        )),
        Command::DeleteBookmark(name.to_owned()),
    )
}

/// Asking to forget the bookmark of this name.
pub fn ask_forget_bookmark(name: &str) -> AppAction {
    confirm(
        "Forget",
        Text::from(format!(
            "Are you sure you want to forget the {name} bookmark?"
        )),
        Command::ForgetBookmark(name.to_owned()),
    )
}

/// Asking to put `bookmark` on `head`, which for one of several targets
/// is what settles it on that one.
pub fn ask_set_bookmark(bookmark: &Bookmark, head: &Head) -> AppAction {
    confirm(
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

/// Asking where to make a workspace.
pub fn ask_add_workspace() -> AppAction {
    AppAction::SetPopup(Box::new(WorkspacePopup::new_add()))
}

/// Asking what to call `workspace`, or saying why we cannot rename it
/// where we do not know its directory.
pub fn ask_rename_workspace(workspace: &Workspace) -> AppAction {
    let Some(root) = workspace.root.clone() else {
        return message(
            "Rename",
            format!(
                "{}, and jj renames the workspace a command runs in. \
                 Run `jj workspace rename <new-name>` in that directory yourself.",
                unknown_root(&workspace.name)
            ),
        );
    };

    AppAction::SetPopup(Box::new(WorkspacePopup::new_rename(
        root,
        workspace.name.clone(),
    )))
}

/// Asking to forget `workspace`, or refusing to when it is the one we
/// are running in.
pub fn ask_forget_workspace(workspace: &Workspace) -> AppAction {
    if workspace.current {
        return message(
            "Forget",
            "This is the workspace blazingjj is running in. Switch to another one \
             to forget it from there.",
        );
    }

    confirm(
        "Forget",
        Text::from(vec![
            Line::from(format!(
                "Are you sure you want to forget the {} workspace?",
                workspace.name
            )),
            Line::from("Whatever is in its directory is left alone."),
        ]),
        Command::ForgetWorkspace(workspace.name.clone()),
    )
}

/// Working in one of the workspaces whose working copy is `head`, other
/// than the one we are running in: what to call it and what picking it
/// asks for, or None where the change is no other workspace's to be
/// worked in. Several are put to a menu of their own, one being asked
/// about by name.
pub fn switch_to_workspace_on(
    anchor: Option<Position>,
    head: &Head,
) -> Result<Option<(String, AppAction)>> {
    let workspaces: Vec<Workspace> = new_commander()
        .get_workspaces_at(&head.commit_id)?
        .into_iter()
        .filter(|workspace| !workspace.current)
        .collect();

    Ok(match workspaces.as_slice() {
        [] => None,
        [only] => Some((
            format!("Work in the {} workspace", only.name),
            switch_workspace(only),
        )),
        several => Some((
            format!("Work in one of the {} workspaces here", several.len()),
            AppAction::SetPopup(Box::new(workspace_menu(anchor, several))),
        )),
    })
}

/// Asking to work in one of the workspaces whose working copy is
/// `head`, which where the change is no other workspace's leaves only
/// saying so: what the menu holds nothing about, a key has to answer
/// for.
pub fn ask_switch_to_workspace_on(head: &Head) -> Result<AppAction> {
    Ok(match switch_to_workspace_on(None, head)? {
        Some((_, action)) => action,
        None => message(
            "Switch",
            "No workspace other than the one blazingjj is running in has its working \
             copy on this change.",
        ),
    })
}

/// The menu of the workspaces a change is the working copy of, put at
/// `anchor` or centered when there is nowhere to point at.
fn workspace_menu(anchor: Option<Position>, workspaces: &[Workspace]) -> ChoicePopup {
    let items: Vec<(Line<'static>, AppAction)> = workspaces
        .iter()
        .map(|workspace| {
            (
                Line::raw(workspace.name.clone()),
                switch_workspace(workspace),
            )
        })
        .collect();

    ChoicePopup::new(anchor, "Work in a workspace", items)
}

/// Work in `workspace` from now on, or say why we cannot: it is the one
/// we are in, or we do not know its directory.
pub fn switch_workspace(workspace: &Workspace) -> AppAction {
    if workspace.current {
        return message(
            "Switch",
            "This is the workspace blazingjj is already running in.",
        );
    }
    let Some(root) = workspace.root.clone() else {
        return message(
            "Switch",
            format!(
                "{}, so there is nowhere for us to run. Start blazingjj in that \
                 directory to work in it.",
                unknown_root(&workspace.name)
            ),
        );
    };

    AppAction::WorkIn(root)
}

/// Moving a workspace other than the one we are running in onto `head`:
/// what to call it and what picking it asks for, or None where there is
/// no other workspace to move. Several are put to a menu of their own,
/// one being asked about by name.
pub fn move_workspace_to(
    anchor: Option<Position>,
    head: &Head,
) -> Result<Option<(String, AppAction)>> {
    let workspaces = new_commander().get_other_workspaces()?;

    Ok(match workspaces.as_slice() {
        [] => None,
        [only] => Some((
            format!("Move the {} workspace here", only.name),
            ask_move_workspace(anchor, only, head),
        )),
        several => Some((
            "Move a workspace here".to_owned(),
            AppAction::SetPopup(Box::new(move_workspace_menu(anchor, several, head))),
        )),
    })
}

/// Asking to move a workspace onto `head`, which where the repo has no
/// workspace but ours leaves only saying so: what the menu holds nothing
/// about, a key has to answer for.
pub fn ask_move_workspace_to(head: &Head) -> Result<AppAction> {
    Ok(match move_workspace_to(None, head)? {
        Some((_, action)) => action,
        None => message(
            "Move workspace",
            "The repo has no workspace other than the one blazingjj is running in.",
        ),
    })
}

/// The menu of the workspaces there are to move onto `head`, put at
/// `anchor` or centered when there is nowhere to point at.
fn move_workspace_menu(
    anchor: Option<Position>,
    workspaces: &[Workspace],
    head: &Head,
) -> ChoicePopup {
    let items: Vec<(Line<'static>, AppAction)> = workspaces
        .iter()
        .map(|workspace| {
            (
                Line::raw(workspace.name.clone()),
                ask_move_workspace(anchor, workspace, head),
            )
        })
        .collect();

    ChoicePopup::new(anchor, "Move a workspace here", items)
}

/// Asking where on `head` to move `workspace`: onto the change itself,
/// or onto a new change under it. The two are the whole question, so
/// picking one is what asks for the move.
pub fn ask_move_workspace(
    anchor: Option<Position>,
    workspace: &Workspace,
    head: &Head,
) -> AppAction {
    if workspace.current {
        return message(
            "Move workspace",
            "This is the workspace blazingjj is running in, which is moved by editing \
             the change or creating one.",
        );
    }
    let Some(root) = workspace.root.clone() else {
        return message(
            "Move workspace",
            format!(
                "{}, so there is nowhere for us to move it. Start blazingjj in that \
                 directory to move it from there.",
                unknown_root(&workspace.name)
            ),
        );
    };

    let name = workspace.name.clone();
    let change: String = head.change_id.as_str().chars().take(8).collect();
    let move_to = |target| {
        AppAction::Run(Command::MoveWorkspace {
            root: root.clone(),
            name: name.clone(),
            revset: Revset::from(&head.commit_id),
            target,
        })
    };

    let items = vec![
        (
            Line::raw(format!("Move {name} to {change}")),
            move_to(MoveTarget::Change),
        ),
        (
            Line::raw(format!("Move {name} to a new change under {change}")),
            move_to(MoveTarget::NewChild),
        ),
    ];

    AppAction::SetPopup(Box::new(
        ChoicePopup::new(anchor, "Move workspace", items).footnote(vec![Line::styled(
            format!("The files in {root} change with it."),
            Role::Hint.style(),
        )]),
    ))
}

/// How a refusal opens where the repo does not say what directory the
/// workspace of this name is in.
fn unknown_root(name: &str) -> String {
    format!(
        "The repo does not say where the {name} workspace is -- it was added before jj \
         recorded that, or its directory has been moved or deleted since"
    )
}

/// Turning down an operation asked for on a view the repo has moved on
/// from, which it would be carried out against.
pub fn refuse_outdated_view() -> AppAction {
    message(
        "Out of date",
        "The repo has moved since this view was read. Refresh before running \
         this operation.",
    )
}

/// Asking whether to update a stale working copy, which jj refuses to
/// read the repo until.
pub fn ask_update_stale_workspace() -> AppAction {
    confirm(
        "Stale working copy",
        Text::from(vec![
            Line::from("The working copy is stale: the repo has moved on since it was last"),
            Line::from("updated, and jj reads nothing here until it is updated."),
            Line::from(""),
            Line::from("Update it now?"),
        ]),
        Command::UpdateStaleWorkspace,
    )
}

/// Put `question` to the user, running `command` if they say yes.
fn confirm(title: &'static str, question: Text<'static>, command: Command) -> AppAction {
    AppAction::SetPopup(Box::new(ConfirmPopup::new(
        title,
        question,
        AppAction::Run(command),
    )))
}

/// What to show once an operation has rewritten changes without moving
/// the working copy: every tab is out of date, and the log may be done
/// marking what it handed over.
fn rewritten(taken: Option<AppAction>) -> AppAction {
    let mut actions = vec![AppAction::MarkTabsStale];
    actions.extend(taken);

    AppAction::Multiple(actions)
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
    AppAction::SetPopup(Box::new(MessagePopup::new(title, text).wrapped()))
}

/// What jj said when it would not do what `operation` asked. Its answer
/// is laid out as it wrote it, being several lines as often as not.
pub fn refused(operation: &'static str, err: impl Display) -> AppAction {
    AppAction::SetPopup(Box::new(
        MessagePopup::new(operation, format!("{err:#}"))
            .text_align(Alignment::Left)
            .wrapped(),
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
            local_bookmarks: Vec::new(),
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

    #[test]
    fn only_the_marked_changes_are_done_with_once_acted_on() {
        let marked = [CommitId("abc".to_owned()), CommitId("def".to_owned())];
        let fallback = CommitId("ghi".to_owned());

        let (changes, taken) = ActsOn::marked_or(&marked, &fallback).into_parts();
        assert_eq!(changes, Revset::expression("abc | def"));
        assert!(matches!(taken, Some(AppAction::ClearLogMarks)));

        let (changes, taken) = ActsOn::marked_or(&[], &fallback).into_parts();
        assert_eq!(changes, Revset::expression("ghi"));
        assert!(taken.is_none());
    }

    /// What jj answers when asked what a push would do
    const PREVIEW: &str = "Changes to push to origin:\n  Add bookmark here to 0123abcd\nDry-run requested, not pushing.\n";

    /// A push of every tracked bookmark to the default remote
    fn tracked() -> Push {
        Push {
            target: PushTarget::Tracked,
            remote: None,
        }
    }

    #[test]
    fn a_change_marked_as_its_own_rebase_destination_is_turned_down() {
        set_test_env();

        let onto = head("abc", false);
        let action = rebase(std::slice::from_ref(&onto.commit_id), &onto).expect("the question");

        assert!(says(action, "Cannot rebase a change onto itself"));
    }

    #[test]
    fn a_rebase_leaves_its_destination_out_of_the_changes_it_moves() {
        set_test_env();

        let onto = head("abc", false);
        let marked = [CommitId("def".to_owned()), onto.commit_id.clone()];
        let rows = rows(rebase(&marked, &onto).expect("the popup"));

        assert!(says_where(&rows, "Source: 1 marked change"), "{rows:?}");
    }

    #[test]
    fn a_change_marked_as_its_own_squash_destination_is_turned_down() {
        set_test_env();

        let into = head("abc", false);
        let action =
            ask_squash(&into, std::slice::from_ref(&into.commit_id), false).expect("the question");

        assert!(says(action, "Cannot squash a change into itself"));
    }

    #[test]
    fn the_marked_changes_are_the_sources_the_squash_asks_about() {
        set_test_env();

        let into = head("abc", false);
        let action = ask_squash(&into, &[CommitId("def".to_owned())], false).expect("the question");

        let rows = rows(action);
        assert!(says_where(&rows, "squash the marked changes"), "{rows:?}");
        assert!(says_where(&rows, "Squash into abc"), "{rows:?}");
    }

    #[test]
    fn parallelizing_takes_more_than_one_change() {
        set_test_env();

        let one = [CommitId("abc".to_owned())];
        assert!(says(parallelize(&one), "more than one marked change"));

        let two = [CommitId("abc".to_owned()), CommitId("def".to_owned())];
        assert!(matches!(
            parallelize(&two),
            AppAction::Run(Command::Parallelize(_))
        ));
    }

    #[test]
    fn the_push_question_holds_what_jj_said_the_push_would_do() {
        set_test_env();

        let rows = rows(ask_push(tracked(), PREVIEW.to_owned()));

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
            tracked(),
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
            ask_push(tracked(), "Dry-run requested, not pushing.\n".to_owned()),
            "jj said nothing"
        ));
    }

    /// Opening a file at a revision is only something an editor that
    /// reads revisions itself can do, which is what a URL to name one by
    /// says the configured editor does.
    #[test]
    fn the_file_at_the_revision_is_only_offered_with_a_url_to_name_it_by() {
        set_test_env();
        let opening = |config, diff_type| {
            ask_open_file(
                config,
                &head("a", false),
                "change-a",
                &File {
                    line: "M Cargo.toml".to_owned(),
                    path: Some("Cargo.toml".to_owned()),
                    diff_type: Some(diff_type),
                },
            )
        };
        let configured = || {
            toml::from_str::<JjConfig>(r#"blazingjj.editor-url = "jj://$revision/$file""#)
                .expect("the configuration parses")
        };

        assert!(!says(
            opening(JjConfig::default(), DiffType::Modified),
            "at this revision"
        ));
        assert!(says(
            opening(configured(), DiffType::Modified),
            "at this revision"
        ));
        assert!(
            !says(opening(configured(), DiffType::Deleted), "at this revision"),
            "the revision has no version of a file it deletes"
        );
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
                "change-a",
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
                "change-a",
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
                "change-a",
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
            ask_open_file(JjConfig::default(), &head("a", false), "change-a", &file),
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
            ask_edit(&head("a", true), "Change: a".to_owned(), false),
            "because it is immutable"
        ));
    }

    #[test]
    fn an_immutable_change_is_asked_about_when_immutability_is_ignored() {
        assert!(says(
            ask_edit(&head("a", true), "Change: a".to_owned(), true),
            "This change is immutable"
        ));
    }

    #[test]
    fn abandoning_names_the_selected_change_when_none_are_marked() {
        assert!(says(ask_abandon(&head("a", false), vec![]), "Change: a"));
    }

    #[test]
    fn abandoning_counts_the_marked_changes_rather_than_naming_them() {
        let marked = vec![CommitId("commit-a".into()), CommitId("commit-b".into())];

        assert!(says(
            ask_abandon(&head("a", false), marked),
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
            ask_op_restore(&operation),
            "Operation: 0123456789ab describe commit"
        ));
        assert!(says(
            ask_op_revert(&operation),
            "Operation: 0123456789ab describe commit"
        ));
    }

    #[test]
    fn an_immutable_selection_is_never_abandoned_even_with_others_marked() {
        let marked = vec![CommitId("commit-a".into())];

        assert!(says(
            ask_abandon(&head("a", true), marked),
            "because it is immutable"
        ));
    }

    fn workspace(root: Option<&str>, current: bool) -> Workspace {
        Workspace {
            name: "other".to_owned(),
            root: root.map(str::to_owned),
            target: head("a", false),
            current,
        }
    }

    #[test]
    fn the_workspace_we_are_in_is_not_forgotten() {
        set_test_env();
        let ours = workspace(Some("/tmp/ours"), true);

        assert!(says(ask_forget_workspace(&ours), "running in"));
    }

    #[test]
    fn a_workspace_without_a_directory_is_not_renamed() {
        set_test_env();
        let lost = workspace(None, false);

        assert!(says(ask_rename_workspace(&lost), "does not say where"));
    }

    #[test]
    fn only_another_workspace_with_a_directory_is_switched_to() {
        set_test_env();

        assert!(says(
            switch_workspace(&workspace(Some("/tmp/ours"), true)),
            "already running in"
        ));
        assert!(says(
            switch_workspace(&workspace(None, false)),
            "does not say where"
        ));
        assert!(matches!(
            switch_workspace(&workspace(Some("/tmp/other"), false)),
            AppAction::WorkIn(root) if root == "/tmp/other"
        ));
    }
}
