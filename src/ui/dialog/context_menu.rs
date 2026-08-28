/*! The common single-commit operations to run against the currently
selected change, each of them settled before the menu goes up, so that
picking one asks for exactly what its keybinding would.
*/

use anyhow::Result;
use ratatui::layout::Position;
use ratatui::text::Line;

use crate::app::command;
use crate::app::command::Command;
use crate::commander::ids::CommitId;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::commander::revset::Revset;
use crate::env::JjConfig;
use crate::ui::AppAction;
use crate::ui::dialog::ChoicePopup;

/// The context menu for `selected`, put where it was opened. Squashing
/// the change the working copy is on goes to its parent, and rebasing it
/// has nowhere to go at all, so the menu says as much.
pub fn context_menu(
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
