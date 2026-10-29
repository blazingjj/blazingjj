/*! Repository operations, each specifying both the action and its target,
so that the component requesting the operation does not need to hold
the selection it acts on.

Only the application runs them. The UI action to display once completed
is returned as an [AppAction].
*/

use std::fmt::Display;
use std::path::Path;

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
use crate::ui::dialog::RebaseSources;
use crate::ui::dialog::describe_action;
use crate::ui::dialog::new_insert;
use crate::ui::styles::AnsiText;

/// What an operation acts on: either the marked changes in the log, or a
/// fallback when none are marked. This distinction determines whether the log
/// clears its marks once the operation completes.
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

/// Specifies what the log should do once an operation using marked changes
/// has completed: it clears the marks.
fn marks_taken(marked: bool) -> Option<AppAction> {
    marked.then_some(AppAction::ClearLogMarks)
}

/// Specifies which version of a file an editor should open. Since the editor
/// always modifies the working copy, accessing another change's version of a
/// file requires moving the working copy there first.
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

/// The change for which the set-bookmark dialog was opened, which contains
/// everything needed to restore it.
pub struct BookmarkSetDialog {
    pub config: JjConfig,
    pub change_id: Option<ChangeId>,
}

pub enum Command {
    /// Place text onto the system clipboard.
    Copy(String),
    Duplicate(ActsOn),
    Parallelize(ActsOn),
    Absorb(Head),
    /// Create a change from the target specified by `acts_on`,
    /// inserted where `insert` dictates.
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
    /// Abandon the marked changes, or the selected one if none are marked,
    /// moving the selection away from them.
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
    Push(PushTarget),
    /// Preview the push actions and prompt the user with the preview
    /// to confirm whether to proceed.
    PreviewPush(PushTarget),
    Fetch {
        all_remotes: bool,
    },
    /// Restore the repository to the state it was in after this operation.
    RestoreOperation(OperationId),
    /// Revert this single operation, leaving subsequent operations intact.
    RevertOperation(OperationId),
    RestoreFile(File),
    UntrackFile(File),
    /// Open the file in the configured editor at the version specified by `at`.
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
        /// Information needed to restore the set-bookmark dialog
        /// if the name is rejected.
        dialog: Option<Box<BookmarkSetDialog>>,
    },
    DeleteBookmark(String),
    ForgetBookmark(String),
    TrackBookmark(Bookmark),
    UntrackBookmark(Bookmark),
    /// Set a user configuration option, with `value` as the TOML expression.
    SetSetting {
        key: String,
        value: String,
    },
    /// Remove an option from the user's configuration,
    /// falling back to defaults.
    UnsetSetting {
        key: String,
    },
    /// Update the working copy; jj refuses to read the repository until
    /// it is updated.
    UpdateStaleWorkspace,
}

impl Command {
    /// Whether the operation interacts with the repository, meaning an outdated
    /// view would still be valid when executing it.
    pub fn touches_the_repo(&self) -> bool {
        !matches!(
            self,
            Command::Copy(_) | Command::SetSetting { .. } | Command::UnsetSetting { .. }
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
                // Inserting can hit immutable changes, so the changes remain
                // marked for another attempt.
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
                // Sources here are the marked changes, which the log stops
                // marking once they are folded in.
                let marked = from.is_some();
                match new_commander().run_squash(from, &target.commit_id, ignore_immutable) {
                    // Folding the working copy moves it, so the view follows it;
                    // folding marked changes leaves the view where it was.
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

                // A tab following a deleted change falls back to the working
                // copy, which might be unrelated to what was being read, so
                // move the selection to the parent instead.
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
                    // Restore the editor with the written description so user
                    // input is not lost when rejected.
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
            Command::UpdateStaleWorkspace => match new_commander().update_stale_workspace() {
                // The working copy is now where the repository dictates,
                // which may differ from where tabs last read it.
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
            // This works even for deleted files, as jj handles them successfully.
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
                // Reopen the prompt with the typed name so the user can
                // correct it.
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
                // Reopen the prompt with the rejected name for correction.
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

/// Open `file` in the configured editor after moving the working copy
/// to the location specified by `at`.
fn open_file(file: &File, at: &OpenAt) -> Result<Option<AppAction>> {
    let Some(path) = file.path.as_deref() else {
        return Ok(Some(message("Open", "The line names no file to open.")));
    };
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
        // The editor reads the revision directly, so the working copy
        // stays put and the URL points to the file location.
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

/// Open `target` (a working copy file or a revision URL) in `editor`,
/// either running interactively in the terminal or detached, according
/// to configuration.
fn open_in_editor(editor: &Editor, target: &str) -> Option<AppAction> {
    let env = get_env();
    let program = Program::new(editor.program(), env.root.clone()).args(editor.args(target));

    match env.jj_config.editor_mode() {
        EditorMode::Terminal => Some(AppAction::RunInteractive(Interactive {
            program,
            // The editor leaves the edited file on the screen, which
            // doesn't need to persist once closed.
            hold_screen: false,
            on_success: Vec::new(),
        })),
        EditorMode::Detached => match program.run_detached() {
            Ok(()) => None,
            Err(err) => Some(refused("Open", err)),
        },
    }
}

/// Prompt the user where to open `file` (associated with `head`, identified by
/// `revision`), offering options such as the working copy version, the change
/// version (checking it out first), or directly via an editor URL if supported.
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

    // A deleted file has no version in the change, regardless of how the tab
    // shows it.
    let at_change = file.diff_type != Some(DiffType::Deleted);

    let mut items = Vec::new();
    // An editor that reads revisions directly accesses the file without
    // checking anything out, so prioritize it if configured.
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

/// Check whether `path` exists in the working copy. Files added or deleted
/// by other changes aren't present here, whereas symlinks exist regardless
/// of their target's presence.
fn in_working_copy(path: &str) -> bool {
    Path::new(&get_env().root)
        .join(path)
        .symlink_metadata()
        .is_ok()
}

/// Prompt to create a new change from the marked changes, or from `selected`
/// if none are marked.
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

/// Prompt to create a new change from the commit pointed to by a bookmark.
pub fn ask_new_change_from_bookmark(bookmark: &Bookmark, head: &Head, describe: bool) -> AppAction {
    ask_new_change(
        ActsOn::change(&head.commit_id),
        &bookmark.to_string(),
        describe,
    )
}

/// View files for a specific version of `change`.
pub fn show_version_files(version: &Head, change: &Head) -> AppAction {
    if version.commit_id == change.commit_id {
        AppAction::ViewFiles(version.clone())
    } else {
        AppAction::ViewVersionFiles(version.clone())
    }
}

/// Prompt to describe `head`, returning an error if immutable or opening
/// the editor with the current description.
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

/// Parallelize the marked changes.
/// if fewer than two are marked, return an error.
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

/// Prompt to rebase `marked` onto `destination`.
/// Rebase the working copy if 'marked' is empty.
pub fn rebase(marked: &[CommitId], destination: &Head) -> Result<AppAction> {
    // If the destination is among the marked sources, exclude it to avoid rebasing onto itself.
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

/// Prompt whether to proceed with the push described by `preview`.
fn ask_push(target: PushTarget, preview: String) -> AppAction {
    // Filter out the dry-run request notice to keep the preview clean.
    let preview: String = preview
        .lines()
        .filter(|line| !line.contains("Dry-run requested"))
        .fold(String::new(), |text, line| text + line + "\n");
    if preview.trim().is_empty() {
        return message("Push", "jj said nothing about what this push would do");
    }
    // If there is nothing to change, report it as a message instead of
    // asking for confirmation.
    if preview.contains("Nothing changed") {
        return AppAction::SetPopup(Box::new(
            MessagePopup::new("Push", preview).text_align(Alignment::Left),
        ));
    }

    // Preserve the layout, colors, and indentation of the jj output.
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
        AppAction::Run(Command::Push(target)),
    )))
}

/// Push according to `scope` relative to `selected`. Pushing new bookmarks
/// requires naming them explicitly, as jj only tracks remote bookmarks when
/// requested by name.
pub fn push(selected: &Head, scope: PushScope) -> AppAction {
    let revset = Revset::from(&selected.commit_id);
    let target = match scope {
        // Both scopes send the change's bookmarks;
        // neither has anything to send if none exist.
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

/// Send `target`, previewing first unless confirmation is disabled.
pub fn push_command(target: PushTarget) -> Command {
    if get_env().jj_config.confirm_push() {
        Command::PreviewPush(target)
    } else {
        Command::Push(target)
    }
}

/// Prompt to create or move a bookmark on `head`.
pub fn set_bookmark(config: JjConfig, head: &Head) -> AppAction {
    AppAction::SetPopup(Box::new(BookmarkSetPopup::new(
        config,
        Some(head.change_id.clone()),
        head.commit_id.clone(),
    )))
}

/// Prompt to create a new change from `revset`,
/// with `target` describing the source for the user.
pub fn ask_new_change(acts_on: ActsOn, target: &str, describe: bool) -> AppAction {
    AppAction::SetPopup(Box::new(new_insert(target, |insert| {
        AppAction::Run(Command::New {
            acts_on: acts_on.clone(),
            insert,
            describe,
        })
    })))
}

/// Prompt to squash the marked changes (or the working copy if none are marked)
/// into `selected`, handling target validation, immutability rejections, and
/// confirmation.
pub fn ask_squash(
    selected: &Head,
    marked: &[CommitId],
    ignore_immutable: bool,
) -> Result<AppAction> {
    // Marking the target change specifies folding the others into it,
    // as there is nowhere else they can go.
    let sources: Vec<_> = marked
        .iter()
        .filter(|source| **source != selected.commit_id)
        .cloned()
        .collect();
    if sources.is_empty() && !marked.is_empty() {
        return Ok(message("Squash", "Cannot squash a change into itself"));
    }

    // Marked sources are explicit, so the selection remains the target.
    // Squashing the working copy change, however, defaults to its parent.
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

/// Prompt to edit `target`, displaying `subject` in the prompt,
/// or rejecting if immutable.
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

/// Prompt to abandon the `marked` changes (or `selected` if none are marked),
/// rejecting if immutable.
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

/// Prompt to restore the repository to the state left by `operation`.
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

/// Prompt to revert `operation` alone.
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

/// Formats an operation name using its short identifier and description,
/// matching the operation log style.
fn name_of(operation: &Operation) -> String {
    format!(
        "Operation: {} {}",
        operation.id.short(),
        operation.description
    )
}

/// Prompt to delete the bookmark with this name.
pub fn ask_delete_bookmark(name: &str) -> AppAction {
    confirm(
        "Delete",
        Text::from(format!(
            "Are you sure you want to delete the {name} bookmark?"
        )),
        Command::DeleteBookmark(name.to_owned()),
    )
}

/// Prompt to forget the bookmark with this name.
pub fn ask_forget_bookmark(name: &str) -> AppAction {
    confirm(
        "Forget",
        Text::from(format!(
            "Are you sure you want to forget the {name} bookmark?"
        )),
        Command::ForgetBookmark(name.to_owned()),
    )
}

/// Prompt to place `bookmark` onto `head`.
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

/// Rejects operations requested on an outdated view where the repository has
/// moved on.
pub fn refuse_outdated_view() -> AppAction {
    message(
        "Out of date",
        "The repo has moved since this view was read. Refresh before running \
         this operation.",
    )
}

/// Prompt to update a stale working copy, which jj requires before reading the
/// repository.
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

/// Display `question` to the user, running `command` upon confirmation.
fn confirm(title: &'static str, question: Text<'static>, command: Command) -> AppAction {
    AppAction::SetPopup(Box::new(ConfirmPopup::new(
        title,
        question,
        AppAction::Run(command),
    )))
}

/// Actions to show after rewriting changes without moving the working copy:
/// invalidating tabs and clearing log marks if needed.
fn rewritten(taken: Option<AppAction>) -> AppAction {
    let mut actions = vec![AppAction::MarkTabsStale];
    actions.extend(taken);

    AppAction::Multiple(actions)
}

/// Display `change` in the view, updating the log and head following
/// repository changes.
fn show_change(change: Head) -> AppAction {
    AppAction::Multiple(vec![
        AppAction::ViewLog(change.clone()),
        AppAction::ChangeHead(change),
        AppAction::MarkTabsStale,
    ])
}

/// Handles repository movement due to an undone operation, updating tabs
/// and the working copy head.
fn repo_moved() -> Result<AppAction> {
    Ok(AppAction::Multiple(vec![
        AppAction::ChangeHead(new_commander().get_current_head()?),
        AppAction::MarkTabsStale,
    ]))
}

/// Show the files of the working copy commit after an operation modifies
/// its contents.
fn show_working_copy_files() -> Result<AppAction> {
    Ok(AppAction::Multiple(vec![
        AppAction::ViewFiles(new_commander().get_current_head()?),
        AppAction::MarkTabsStale,
    ]))
}

fn message(title: &'static str, text: impl Into<String>) -> AppAction {
    AppAction::SetPopup(Box::new(MessagePopup::new(title, text).wrapped()))
}

/// Displays the error message returned by jj when `operation` is rejected.
fn refused(operation: &'static str, err: impl Display) -> AppAction {
    AppAction::SetPopup(Box::new(
        MessagePopup::new(operation, format!("{err:#}"))
            .text_align(Alignment::Left)
            .wrapped(),
    ))
}

/// Run `operation` in `slot` and show a loader popup until the result arrives,
/// blocking input.
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

/// Creates the loader popup used by [with_loader] for callers needing custom
/// output handling.
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
}
