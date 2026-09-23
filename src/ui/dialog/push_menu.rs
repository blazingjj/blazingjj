/*! The menu of what a push can send, which is where the targets that
have no key of their own are reached from.
*/

use anyhow::Result;
use ratatui::layout::Position;
use ratatui::text::Line;
use ratatui::text::Span;

use crate::app::command;
use crate::app::command::Command;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::keybinds::PushScope;
use crate::theme::Role;
use crate::ui::AppAction;
use crate::ui::dialog::Choice;
use crate::ui::dialog::ChoicePopup;

/// What every scope says for itself in the menu, and the key that picks
/// it. The keys are the letters of the jj flags they stand for.
const SCOPES: [(PushScope, char, &str); 6] = [
    (PushScope::Selected, 'r', "Bookmarks on this change"),
    (
        PushScope::SelectedWithNew,
        'b',
        "Bookmarks on this change, new ones included",
    ),
    (PushScope::Tracked, 't', "All tracked bookmarks"),
    (PushScope::All, 'a', "All bookmarks, new ones included"),
    (
        PushScope::Change,
        'c',
        "This change with auto-generated bookmark",
    ),
    (PushScope::Named, 'n', "This change with named bookmark"),
];

/// The key that opens the choice of remote. A choice's key is matched in
/// lower case.
const REMOTE_KEY: char = 'o';

/// The push menu for `selected`, put where it was opened, pushing to
/// `remote` or to jj's default remote when [None]. Every entry asks for
/// what the keybinding of its scope would, given the same remote.
pub fn push_menu(
    anchor: Option<Position>,
    selected: &Head,
    remote: Option<String>,
) -> Result<ChoicePopup> {
    let commander = new_commander();
    let remotes = commander.git_remotes()?;

    let items = SCOPES.iter().map(|(scope, key, label)| {
        Choice::new(
            Line::raw(format!("[{key}] {label}")),
            command::push(selected, *scope, remote.clone()),
        )
        .key(*key)
    });
    let menu = ChoicePopup::new(anchor, "Push", items);
    // With a single remote, there is nothing to choose.
    if remotes.len() <= 1 {
        return Ok(menu);
    }

    let current = remote.unwrap_or_else(|| commander.default_push_remote(&remotes));
    let label = Line::from(vec![
        Span::raw(format!("[{REMOTE_KEY}] Remote: ")),
        Span::styled(current.clone(), Role::Remote.style()),
    ]);
    let choose = AppAction::SetPopup(Box::new(remote_menu(anchor, selected, &remotes, &current)));

    Ok(menu.group([Choice::new(label, choose).key(REMOTE_KEY)]))
}

/// The choice of the remote to push to, which reopens the push menu for
/// `selected` with the one picked.
fn remote_menu(
    anchor: Option<Position>,
    selected: &Head,
    remotes: &[String],
    current: &str,
) -> ChoicePopup {
    // The push menu has an entry for this menu itself, so it is only
    // built once picked.
    let items = remotes.iter().map(|remote| {
        Choice::new(
            Line::raw(remote.clone()),
            AppAction::Run(Command::PushMenu {
                anchor,
                selected: selected.clone(),
                remote: remote.clone(),
            }),
        )
    });
    let current = remotes.iter().position(|remote| remote == current);

    ChoicePopup::new(anchor, "Push to remote", items).selected(current.unwrap_or(0))
}
