/*! The common operations to run against whatever a tab has selected,
each of them settled before the menu goes up, so that picking one asks
for exactly what its keybinding would.

What a tab offers is every item its selection has anything to do to;
which of them a menu holds and in which order is
[the menu's](crate::menus::context_menu) to say. An item is named by the
id the keybinding for the same action is configured under, so that an
action goes by one name however it is reached.
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
use crate::env::JjConfig;
use crate::menus::Item;
use crate::menus::Menu;
use crate::menus::context_menu;
use crate::selection::Selection;
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
    selection: &Selection,
    selected: &Head,
    marked: &[CommitId],
) -> Result<ChoicePopup> {
    let at = new_commander().get_current_head()?;
    let selected_is_at = selected.change_id == at.change_id;

    let mut items = vec![
        Item::new(
            "edit-change",
            Line::raw("Edit"),
            command::ask_edit(
                config.clone(),
                selected,
                format!("Change: {}", selected.change_id.as_str()),
                false,
            ),
        ),
        Item::new(
            "create-new",
            Line::raw("New"),
            command::ask_new_change_from_selection(config.clone(), selected, marked, false),
        ),
        Item::new(
            "create-new-describe",
            Line::raw("New & describe"),
            command::ask_new_change_from_selection(config.clone(), selected, marked, true),
        ),
        Item::new(
            "describe",
            Line::raw("Describe"),
            command::describe(selected)?,
        ),
        Item::new(
            "absorb",
            Line::raw("Absorb"),
            AppAction::Run(Command::Absorb(selected.clone())),
        ),
        Item::new(
            "abandon",
            Line::raw("Abandon"),
            command::ask_abandon(config.clone(), selected, marked.to_vec()),
        ),
        Item::new(
            "duplicate",
            Line::raw("Duplicate"),
            AppAction::Run(Command::Duplicate(Revset::from(&selected.change_id))),
        ),
        Item::new(
            "squash",
            Line::raw(if selected_is_at {
                "Squash @ into its parent"
            } else {
                "Squash @ into this"
            }),
            command::ask_squash(config.clone(), selected, false)?,
        ),
    ];
    if !selected_is_at {
        items.push(Item::new(
            "rebase",
            Line::raw("Rebase @ to this"),
            command::rebase(selected)?,
        ));
    }
    items.extend([
        Item::new(
            "push-menu",
            Line::raw("Push"),
            AppAction::SetPopup(Box::new(push_menu(config.clone(), anchor, selected))),
        ),
        Item::new(
            "set-bookmark",
            Line::raw("Set bookmark"),
            command::set_bookmark(config.clone(), selected),
        ),
        Item::new(
            "copy-change-id",
            Line::raw("Copy change id"),
            AppAction::Run(Command::Copy(selected.change_id.as_string())),
        ),
        Item::new(
            "copy-rev",
            Line::raw("Copy commit id"),
            AppAction::Run(Command::Copy(selected.commit_id.as_str().to_owned())),
        ),
    ]);

    Ok(context_menu(&config, anchor, Menu::Log, selection, items))
}

/// The context menu for `file`, `open` being what opening it in an
/// editor takes, which depends on what the tab is showing.
pub fn files_context_menu(
    config: JjConfig,
    anchor: Option<Position>,
    selection: &Selection,
    file: &File,
    open: AppAction,
) -> ChoicePopup {
    let items = vec![
        Item::new("open", Line::raw("Open in editor"), open),
        Item::new(
            "restore",
            Line::raw("Restore"),
            AppAction::Run(Command::RestoreFile(file.clone())),
        ),
        Item::new(
            "untrack",
            Line::raw("Untrack"),
            AppAction::Run(Command::UntrackFile(file.clone())),
        ),
    ];

    context_menu(&config, anchor, Menu::Files, selection, items)
}

/// The context menu for `version` of `change`.
pub fn evolog_context_menu(
    config: JjConfig,
    anchor: Option<Position>,
    selection: &Selection,
    version: &Head,
    change: &Head,
) -> ChoicePopup {
    let items = vec![
        Item::new(
            "open-files",
            Line::raw("Show files"),
            command::show_version_files(version, change),
        ),
        Item::new(
            "duplicate",
            Line::raw("Duplicate"),
            AppAction::Run(Command::Duplicate(Revset::from(&version.commit_id))),
        ),
        Item::new(
            "copy-rev",
            Line::raw("Copy commit id"),
            AppAction::Run(Command::Copy(version.commit_id.as_str().to_owned())),
        ),
    ];

    context_menu(&config, anchor, Menu::Evolog, selection, items)
}

/// The context menu for `operation`.
pub fn op_log_context_menu(
    config: JjConfig,
    anchor: Option<Position>,
    selection: &Selection,
    operation: &Operation,
) -> ChoicePopup {
    let items = vec![
        Item::new(
            "restore",
            Line::raw("Restore the repo to this operation"),
            command::ask_op_restore(config.clone(), operation),
        ),
        Item::new(
            "revert",
            Line::raw("Revert this operation"),
            command::ask_op_revert(config.clone(), operation),
        ),
        Item::new(
            "copy-id",
            Line::raw("Copy operation id"),
            AppAction::Run(Command::Copy(operation.id.as_str().to_owned())),
        ),
    ];

    context_menu(&config, anchor, Menu::OpLog, selection, items)
}

/// The context menu for `selected`, the bookmark and the change its line
/// points at, which is None when the selection is not on a bookmark there
/// is anything to do to: then there is nothing but creating one. Tracking
/// only applies to the bookmarks on a remote.
pub fn bookmarks_context_menu(
    config: JjConfig,
    anchor: Option<Position>,
    selection: &Selection,
    selected: Option<(&Bookmark, &Head)>,
) -> ChoicePopup {
    let mut items = vec![Item::new(
        "create-bookmark",
        Line::raw("Create bookmark"),
        AppAction::SetPopup(Box::new(BookmarkNamePopup::new_create())),
    )];
    if let Some((bookmark, head)) = selected {
        items.extend([
            Item::new(
                "rename-bookmark",
                Line::raw("Rename"),
                AppAction::SetPopup(Box::new(BookmarkNamePopup::new_rename(
                    bookmark.name.clone(),
                ))),
            ),
            Item::new(
                "delete-bookmark",
                Line::raw("Delete"),
                command::ask_delete_bookmark(config.clone(), &bookmark.name),
            ),
            Item::new(
                "forget-bookmark",
                Line::raw("Forget"),
                command::ask_forget_bookmark(config.clone(), &bookmark.name),
            ),
        ]);
        if bookmark.remote.is_some() {
            items.extend([
                Item::new(
                    "track-bookmark",
                    Line::raw("Track"),
                    AppAction::Run(Command::TrackBookmark(bookmark.clone())),
                ),
                Item::new(
                    "untrack-bookmark",
                    Line::raw("Untrack"),
                    AppAction::Run(Command::UntrackBookmark(bookmark.clone())),
                ),
            ]);
        }
        items.extend([
            Item::new(
                "edit-change",
                Line::raw("Edit the change"),
                command::ask_edit(config.clone(), head, format!("Bookmark: {bookmark}"), false),
            ),
            Item::new(
                "create-new",
                Line::raw("New change"),
                command::ask_new_change_from_bookmark(config.clone(), bookmark, head, false),
            ),
            Item::new(
                "create-new-describe",
                Line::raw("New change & describe"),
                command::ask_new_change_from_bookmark(config.clone(), bookmark, head, true),
            ),
            Item::new(
                "view-in-log",
                Line::raw("View in log"),
                AppAction::ViewLog(head.clone()),
            ),
        ]);
    }

    context_menu(&config, anchor, Menu::Bookmarks, selection, items)
}
