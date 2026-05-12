use ratatui::Frame;
use ratatui::crossterm::event::MouseButton;
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
    content_rect: Rect,
    item_count: usize,
    offset: usize,
}

impl ListPane {
    pub fn half_page_delta(&self) -> isize {
        self.content_rect.height as isize / 2
    }

    pub fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        widget: List<'_>,
        list_state: &mut ListState,
    ) {
        self.panel_rect = area;
        self.content_rect = area.inner(Margin {
            vertical: 1,
            horizontal: 0,
        });
        self.item_count = widget.len();
        f.render_stateful_widget(&widget, area, list_state);
        self.offset = list_state.offset();
        if self.item_count > self.content_rect.height as usize {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            let mut scrollbar_state = ScrollbarState::default()
                .content_length(self.item_count)
                .position(list_state.selected().unwrap_or(0));
            f.render_stateful_widget(scrollbar, self.content_rect, &mut scrollbar_state);
        }
    }
}

impl PanelMouseInput for ListPane {
    fn input_mouse(&mut self, mouse: MouseEvent) -> super::MouseInput {
        let pos = Position::new(mouse.column, mouse.row);
        if !self.panel_rect.contains(pos) {
            return super::MouseInput::NotHandled;
        }
        match mouse.kind {
            MouseEventKind::ScrollDown if self.content_rect.contains(pos) => {
                super::MouseInput::Scroll(1)
            }
            MouseEventKind::ScrollUp if self.content_rect.contains(pos) => {
                super::MouseInput::Scroll(-1)
            }
            MouseEventKind::Down(MouseButton::Left) if self.content_rect.contains(pos) => {
                let content_row = (mouse.row - self.content_rect.y) as usize;
                let index = self.offset + content_row;
                if index < self.item_count {
                    super::MouseInput::Select(index)
                } else {
                    super::MouseInput::NotHandled
                }
            }
            _ => super::MouseInput::NotHandled,
        }
    }
}
