/*! The menu of what a push can send, which is where the targets that
have no key of their own are reached from.
*/

use ratatui::layout::Position;
use ratatui::text::Line;

use crate::app::command;
use crate::commander::log::Head;
use crate::env::JjConfig;
use crate::keybinds::PushScope;
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

/// The push menu for `selected`, put where it was opened. Every entry
/// asks for what the keybinding of its scope would.
pub fn push_menu(config: JjConfig, anchor: Option<Position>, selected: &Head) -> ChoicePopup {
    let items = SCOPES.iter().map(|(scope, key, label)| {
        Choice::new(
            Line::raw(format!("[{key}] {label}")),
            command::push(selected, *scope),
        )
        .key(*key)
    });

    ChoicePopup::new(config, anchor, "Push", items)
}
