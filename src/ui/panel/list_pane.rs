use ratatui::Frame;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::widgets::Block;
use ratatui::widgets::List;
use ratatui::widgets::ListState;
use ratatui::widgets::Scrollbar;
use ratatui::widgets::ScrollbarOrientation;
use ratatui::widgets::ScrollbarState;

/// The list a tab shows in its main panel, with a scrollbar.
///
/// The list itself is built by the caller, which also owns the
/// [`ListState`] so that scroll offset and selection have a single source
/// of truth. Every list drawn through a pane gives each item a single row,
/// which is what lets row and item counts be used interchangeably.
#[derive(Default)]
pub struct ListPane {
    /// Area the items were drawn in, borders excluded.
    content_rect: Rect,

    /// Number of items the list held when it was drawn.
    item_count: usize,
}

impl ListPane {
    /// Number of items that fit on screen, as a scroll delta.
    pub fn visible_items(&self) -> isize {
        self.content_rect.height as isize
    }

    /// Draw `widget` inside `block`, plus a scrollbar if the items do not
    /// all fit.
    pub fn render<'a>(
        &mut self,
        f: &mut Frame,
        area: Rect,
        block: Block<'a>,
        widget: List<'a>,
        list_state: &mut ListState,
    ) {
        self.content_rect = block.inner(area);
        self.item_count = widget.len();
        f.render_stateful_widget(&widget.block(block), area, list_state);
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
}
