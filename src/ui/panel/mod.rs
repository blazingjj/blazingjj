mod commit_show;
mod details_panel;
mod evolog_show;
mod list_pane;
mod log_panel;
mod output_cache;
mod output_panel;

pub use commit_show::CommitShowKey;
pub use commit_show::CommitShowPanel;
pub use details_panel::DetailsPanel;
pub use details_panel::LargeStringContent;
pub use details_panel::TextContent;
pub use evolog_show::EvologShowKey;
pub use evolog_show::EvologShowPanel;
pub use list_pane::ListPane;
pub use log_panel::LogPanel;
pub use output_cache::OutputKey;
pub use output_cache::OutputRequest;
pub use output_panel::OutputPanel;

use crate::app::command::Command;
use crate::event::Mouse;
use crate::ui::AppAction;
use crate::ui::ComponentInputResult;

/// What a panel did with a mouse event.
pub(crate) enum MouseInput {
    NotHandled,
    Handled,
    /// The panel wants to be scrolled by this many items, negative
    /// meaning towards the top.
    Scroll(isize),
    /// The item at this index was clicked. A panel has no knowledge of
    /// what its items represent, so it is up to the caller to map the
    /// index onto its own domain type.
    Select(usize),
    /// The item at this index was right-clicked, asking for whatever the
    /// caller offers as a context menu.
    Context(usize),
    /// This text was marked with the mouse, to be copied.
    Copy(String),
}

/// What a tab is to do about text a panel of it had marked, which is the
/// same wherever the panel sits.
pub(crate) fn copy_marked(text: String) -> ComponentInputResult {
    ComponentInputResult::HandledAction(AppAction::Run(Command::Copy(text)))
}

pub(crate) trait PanelMouseInput {
    fn input_mouse(&mut self, mouse: Mouse) -> MouseInput;
}

/// Offer `mouse` to each panel in turn and report what the first one that
/// takes it did with it.
pub(crate) fn route_mouse(mouse: Mouse, panels: &mut [&mut dyn PanelMouseInput]) -> MouseInput {
    for panel in panels {
        match panel.input_mouse(mouse) {
            MouseInput::NotHandled => {}
            result => return result,
        }
    }
    MouseInput::NotHandled
}
