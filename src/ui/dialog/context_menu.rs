/*! The common operations to run against whatever a tab has selected,
each of them settled before the menu goes up, so that picking one asks
for exactly what its keybinding would.
*/

use anyhow::Result;
use ratatui::layout::Position;
use ratatui::text::Line;

use crate::app::command;
use crate::app::command::Command;
use crate::commander::bookmarks::Bookmark;
use crate::commander::files::File;
use crate::commander::ids::CommitId;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::commander::operation::Operation;
use crate::commander::revset::Revset;
use crate::commander::workspace::Workspace;
use crate::env::JjConfig;
use crate::ui::AppAction;
use crate::ui::dialog::BookmarkNamePopup;
use crate::ui::dialog::ChoicePopup;
use crate::ui::dialog::push_menu;

/// The context menu for `selected`, put where it was opened. Squashing
/// the change the working copy is on goes to its parent, and rebasing it
/// has nowhere to go at all, so the menu says as much.
pub fn log_context_menu(
    config: JjConfig,
    anchor: Option<Position>,
    selected: &Head,
    marked: &[CommitId],
) -> Result<ChoicePopup> {
    let at = new_commander().get_current_head()?;
    let selected_is_at = selected.change_id == at.change_id;

    let mut items = vec![
        (
            Line::raw("Edit"),
            command::ask_edit(
                config.clone(),
                selected,
                format!("Change: {}", selected.change_id.as_str()),
                false,
            ),
        ),
        (
            Line::raw("New"),
            command::ask_new_change_from_selection(config.clone(), selected, marked, false),
        ),
        (
            Line::raw("New & describe"),
            command::ask_new_change_from_selection(config.clone(), selected, marked, true),
        ),
        (Line::raw("Describe"), command::describe(selected)?),
        (
            Line::raw("Absorb"),
            AppAction::Run(Command::Absorb(selected.clone())),
        ),
        (
            Line::raw("Abandon"),
            command::ask_abandon(config.clone(), selected, marked.to_vec()),
        ),
        (
            Line::raw("Duplicate"),
            AppAction::Run(Command::Duplicate(Revset::from(&selected.change_id))),
        ),
        (
            Line::raw(if selected_is_at {
                "Squash @ into its parent"
            } else {
                "Squash @ into this"
            }),
            command::ask_squash(config.clone(), selected, false)?,
        ),
    ];
    if !selected_is_at {
        items.push((Line::raw("Rebase @ to this"), command::rebase(selected)?));
    }
    items.extend([
        (
            Line::raw("Push"),
            AppAction::SetPopup(Box::new(push_menu(config.clone(), anchor, selected))),
        ),
        (
            Line::raw("Set bookmark"),
            command::set_bookmark(config.clone(), selected),
        ),
        (
            Line::raw("Copy change id"),
            AppAction::Run(Command::Copy(selected.change_id.as_string())),
        ),
        (
            Line::raw("Copy commit id"),
            AppAction::Run(Command::Copy(selected.commit_id.as_str().to_owned())),
        ),
    ]);

    Ok(ChoicePopup::new(config, anchor, "Actions", items))
}

/// The context menu for `file`, `open` being what opening it in an
/// editor takes, which depends on what the tab is showing.
pub fn files_context_menu(
    config: JjConfig,
    anchor: Option<Position>,
    file: &File,
    open: AppAction,
) -> ChoicePopup {
    let items = vec![
        (Line::raw("Open in editor"), open),
        (
            Line::raw("Restore"),
            AppAction::Run(Command::RestoreFile(file.clone())),
        ),
        (
            Line::raw("Untrack"),
            AppAction::Run(Command::UntrackFile(file.clone())),
        ),
    ];

    ChoicePopup::new(config, anchor, "File actions", items)
}

/// The context menu for `version` of `change`.
pub fn evolog_context_menu(
    config: JjConfig,
    anchor: Option<Position>,
    version: &Head,
    change: &Head,
) -> ChoicePopup {
    let items = vec![
        (
            Line::raw("Show files"),
            command::show_version_files(version, change),
        ),
        (
            Line::raw("Duplicate"),
            AppAction::Run(Command::Duplicate(Revset::from(&version.commit_id))),
        ),
        (
            Line::raw("Copy commit id"),
            AppAction::Run(Command::Copy(version.commit_id.as_str().to_owned())),
        ),
    ];

    ChoicePopup::new(config, anchor, "Version actions", items)
}

/// The context menu for `operation`.
pub fn op_log_context_menu(
    config: JjConfig,
    anchor: Option<Position>,
    operation: &Operation,
) -> ChoicePopup {
    let items = vec![
        (
            Line::raw("Restore the repo to this operation"),
            command::ask_op_restore(config.clone(), operation),
        ),
        (
            Line::raw("Revert this operation"),
            command::ask_op_revert(config.clone(), operation),
        ),
        (
            Line::raw("Copy operation id"),
            AppAction::Run(Command::Copy(operation.id.as_str().to_owned())),
        ),
    ];

    ChoicePopup::new(config, anchor, "Operation actions", items)
}

/// The context menu for `selected`, which is None when the selection is
/// not on a workspace there is anything to do to: then there is nothing
/// but adding one.
pub fn workspaces_context_menu(
    config: JjConfig,
    anchor: Option<Position>,
    selected: Option<&Workspace>,
) -> ChoicePopup {
    let mut items = vec![(Line::raw("Add workspace"), command::ask_add_workspace())];
    if let Some(workspace) = selected {
        items.extend([
            (
                Line::raw("Work in this workspace"),
                command::ask_switch_workspace(config.clone(), workspace),
            ),
            (
                Line::raw("Rename"),
                command::ask_rename_workspace(workspace),
            ),
            (
                Line::raw("Forget"),
                command::ask_forget_workspace(config.clone(), workspace),
            ),
            (
                Line::raw("View the change it holds in the log"),
                AppAction::ViewLog(workspace.target.clone()),
            ),
        ]);
    }

    ChoicePopup::new(config, anchor, "Workspace actions", items)
}

/// The context menu for `selected`, the bookmark and the change its line
/// points at, which is None when the selection is not on a bookmark there
/// is anything to do to: then there is nothing but creating one. Tracking
/// only applies to the bookmarks on a remote.
pub fn bookmarks_context_menu(
    config: JjConfig,
    anchor: Option<Position>,
    selected: Option<(&Bookmark, &Head)>,
) -> ChoicePopup {
    let mut items = vec![(
        Line::raw("Create bookmark"),
        AppAction::SetPopup(Box::new(BookmarkNamePopup::new_create())),
    )];
    if let Some((bookmark, head)) = selected {
        items.extend([
            (
                Line::raw("Rename"),
                AppAction::SetPopup(Box::new(BookmarkNamePopup::new_rename(
                    bookmark.name.clone(),
                ))),
            ),
            (
                Line::raw("Delete"),
                command::ask_delete_bookmark(config.clone(), &bookmark.name),
            ),
            (
                Line::raw("Forget"),
                command::ask_forget_bookmark(config.clone(), &bookmark.name),
            ),
        ]);
        if bookmark.remote.is_some() {
            items.extend([
                (
                    Line::raw("Track"),
                    AppAction::Run(Command::TrackBookmark(bookmark.clone())),
                ),
                (
                    Line::raw("Untrack"),
                    AppAction::Run(Command::UntrackBookmark(bookmark.clone())),
                ),
            ]);
        }
        items.extend([
            (
                Line::raw("Edit the change"),
                command::ask_edit(config.clone(), head, format!("Bookmark: {bookmark}"), false),
            ),
            (
                Line::raw("New change"),
                command::ask_new_change_from_bookmark(config.clone(), bookmark, head, false),
            ),
            (
                Line::raw("New change & describe"),
                command::ask_new_change_from_bookmark(config.clone(), bookmark, head, true),
            ),
            (Line::raw("View in log"), AppAction::ViewLog(head.clone())),
        ]);
    }

    ChoicePopup::new(config, anchor, "Bookmark actions", items)
}
