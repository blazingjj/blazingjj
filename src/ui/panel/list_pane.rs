use ratatui::Frame;
use ratatui::crossterm::event::MouseButton;
use ratatui::crossterm::event::MouseEvent;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Margin;
use ratatui::layout::Position;
use ratatui::layout::Rect;
use ratatui::widgets::Block;
use ratatui::widgets::List;
use ratatui::widgets::ListState;
use ratatui::widgets::Scrollbar;
use ratatui::widgets::ScrollbarOrientation;
use ratatui::widgets::ScrollbarState;

use super::MouseInput;
use super::PanelMouseInput;

/// The list a tab shows in its main panel, with a scrollbar and mouse
/// handling.
///
/// The list itself is built by the caller, which also owns the
/// [`ListState`] so that scroll offset and selection have a single source
/// of truth. Every list drawn through a pane gives each item a single row,
/// which is what lets row and item counts be used interchangeably.
#[derive(Default)]
pub struct ListPane {
    /// Area the pane was drawn in, borders included.
    panel_rect: Rect,

    /// Area the items were drawn in, borders excluded.
    content_rect: Rect,

    /// Number of items the list held when it was drawn.
    item_count: usize,

    /// Index of the first item drawn, as chosen by the list itself.
    offset: usize,
}

impl ListPane {
    /// Number of items that fit on screen, as a scroll delta.
    pub fn visible_items(&self) -> isize {
        self.content_rect.height as isize
    }

    /// Draw `widget` inside `block`, plus a scrollbar if the items do not
    /// all fit, and record the geometry that mouse input is resolved
    /// against.
    pub fn render<'a>(
        &mut self,
        f: &mut Frame,
        area: Rect,
        block: Block<'a>,
        widget: List<'a>,
        list_state: &mut ListState,
    ) {
        self.panel_rect = area;
        self.content_rect = block.inner(area);
        self.item_count = widget.len();
        f.render_stateful_widget(&widget.block(block), area, list_state);
        // The list picks the offset it needs to keep the selection on
        // screen, so it is only known once it has been drawn.
        self.offset = list_state.offset();
        if self.item_count > self.content_rect.height as usize {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            let mut scrollbar_state = ScrollbarState::default()
                .content_length(self.item_count)
                .position(list_state.selected().unwrap_or(0));
            // The scrollbar is drawn onto the right border, so it gets the
            // full width and only skips the corners.
            let scrollbar_rect = area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            });
            f.render_stateful_widget(scrollbar, scrollbar_rect, &mut scrollbar_state);
        }
    }

    /// Index of the item drawn at `pos`, if any.
    fn item_at(&self, pos: Position) -> Option<usize> {
        if !self.content_rect.contains(pos) {
            return None;
        }
        let index = self.offset + (pos.y - self.content_rect.y) as usize;
        (index < self.item_count).then_some(index)
    }
}

impl PanelMouseInput for ListPane {
    fn input_mouse(&mut self, mouse: MouseEvent) -> MouseInput {
        let pos = Position::new(mouse.column, mouse.row);
        if !self.panel_rect.contains(pos) {
            return MouseInput::NotHandled;
        }
        match mouse.kind {
            MouseEventKind::ScrollDown => MouseInput::Scroll(1),
            MouseEventKind::ScrollUp => MouseInput::Scroll(-1),
            MouseEventKind::Down(MouseButton::Left) => match self.item_at(pos) {
                Some(index) => MouseInput::Select(index),
                // A click in the pane is ours even when it hits no item.
                None => MouseInput::Handled,
            },
            _ => MouseInput::NotHandled,
        }
    }
}
