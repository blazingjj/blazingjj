/*! Where a new change goes relative to the one it is created from.
*/

use ratatui::text::Line;

use crate::commander::jj::NewInsertMode;
use crate::env::JjConfig;
use crate::ui::AppAction;
use crate::ui::dialog::ChoicePopup;

/// A popup offering the insertion points around `target`, which names
/// what the new change is created from.
pub fn new_insert(
    config: JjConfig,
    target: &str,
    action: impl Fn(NewInsertMode) -> AppAction,
) -> ChoicePopup {
    let items = vec![
        (
            Line::raw(format!("New child of {target}")),
            action(NewInsertMode::Child),
        ),
        (
            Line::raw(format!("Insert after {target}")),
            action(NewInsertMode::After),
        ),
        (
            Line::raw(format!("Insert before {target}")),
            action(NewInsertMode::Before),
        ),
    ];

    ChoicePopup::new(config, None, "New", items)
}
