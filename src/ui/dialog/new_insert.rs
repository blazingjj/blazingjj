/*! Where a new change goes relative to the one it is created from. The
pick is sent as a [`NewInsertMode`] over a channel; whoever put the popup
up runs the command in its own `update`.
*/

use std::sync::mpsc::Sender;

use ratatui::text::Line;

use crate::commander::jj::NewInsertMode;
use crate::env::JjConfig;
use crate::ui::dialog::ChoicePopup;

/// A popup offering the insertion points around `target`, which names
/// what the new change is created from.
pub fn new_insert(
    config: JjConfig,
    tx: Sender<NewInsertMode>,
    target: &str,
) -> ChoicePopup<NewInsertMode> {
    let items = vec![
        (
            Line::raw(format!("New child of {target}")),
            NewInsertMode::Child,
        ),
        (
            Line::raw(format!("Insert after {target}")),
            NewInsertMode::After,
        ),
        (
            Line::raw(format!("Insert before {target}")),
            NewInsertMode::Before,
        ),
    ];

    ChoicePopup::new(config, tx, "New", items)
}
