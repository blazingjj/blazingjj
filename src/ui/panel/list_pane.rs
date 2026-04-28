use ratatui::Frame;
use ratatui::crossterm::event::MouseEvent;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Margin;
use ratatui::layout::Position;
use ratatui::layout::Rect;
use ratatui::widgets::List;
use ratatui::widgets::ListState;
use ratatui::widgets::Scrollbar;
use ratatui::widgets::ScrollbarOrientation;
use ratatui::widgets::ScrollbarState;

use super::PanelMouseInput;

#[derive(Default)]
pub struct ListPane {
    panel_rect: Rect,
    height: u16,
}

impl ListPane {
    pub fn half_page_delta(&self) -> isize {
        self.height as isize / 2
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        widget: List<'_>,
        list_state: &mut ListState,
    ) {
        self.panel_rect = area;
        self.height = area.height.saturating_sub(2);
        let item_count = widget.len();
        f.render_stateful_widget(&widget, area, list_state);
        if item_count > self.height as usize {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            let mut scrollbar_state = ScrollbarState::default()
                .content_length(item_count)
                .position(list_state.selected().unwrap_or(0));
            f.render_stateful_widget(
                scrollbar,
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }
    }
}

impl PanelMouseInput for ListPane {
    fn input_mouse(&mut self, mouse: MouseEvent) -> super::MouseInput {
        if !self
            .panel_rect
            .contains(Position::new(mouse.column, mouse.row))
        {
            return super::MouseInput::NotHandled;
        }
        match mouse.kind {
            MouseEventKind::ScrollDown => super::MouseInput::Scroll(1),
            MouseEventKind::ScrollUp => super::MouseInput::Scroll(-1),
            _ => super::MouseInput::NotHandled,
        }
    }
}
