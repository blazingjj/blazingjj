mod details_panel;
mod list_pane;
mod log_panel;

pub use details_panel::DetailsPanel;
pub use details_panel::LargeStringContent;
pub use details_panel::TextContent;
pub use list_pane::ListPane;
pub use log_panel::DragAction;
pub use log_panel::DragMode;
pub use log_panel::LogPanel;
pub use log_panel::decode_drag_modifiers;
use ratatui::crossterm::event::MouseEvent;

pub(crate) enum MouseInput {
    NotHandled,
    Handled,
    Scroll(isize),
    Select(usize),
}

pub(crate) trait PanelMouseInput {
    fn input_mouse(&mut self, mouse: MouseEvent) -> MouseInput;
}

pub(crate) fn route_mouse(
    mouse: MouseEvent,
    panels: &mut [&mut dyn PanelMouseInput],
) -> MouseInput {
    for panel in panels {
        match panel.input_mouse(mouse) {
            MouseInput::NotHandled => {}
            result => return result,
        }
    }
    MouseInput::NotHandled
}
