use ratatui::Frame;
use ratatui::crossterm::event::MouseButton;
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
use crate::event::Mouse;

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

    /// Item the last press hit. Selecting one can scroll the list, so a
    /// second press on the cell may well be about another item, which is
    /// not what a double click is.
    pressed: Option<usize>,
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

    /// Where to put something that wants to show up next to the item at
    /// `index`, which takes `height` rows: just below it and indented, so
    /// that the item itself stays readable. None unless the item is on
    /// screen.
    pub fn item_anchor(&self, index: usize, height: u16) -> Option<Position> {
        /// Columns to indent by, enough to tell the item from what shows
        /// up next to it.
        const INDENT: u16 = 6;

        let row = index.checked_sub(self.offset)?;
        (row < self.content_rect.height as usize).then(|| {
            Position::new(
                self.content_rect.x + INDENT,
                self.content_rect.y + row as u16 + height,
            )
        })
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
    fn input_mouse(&mut self, mouse: Mouse) -> MouseInput {
        let pos = mouse.position();
        if !self.panel_rect.contains(pos) {
            return MouseInput::NotHandled;
        }
        match mouse.kind() {
            MouseEventKind::ScrollDown => MouseInput::Scroll(1),
            MouseEventKind::ScrollUp => MouseInput::Scroll(-1),
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = self.item_at(pos);
                let double = mouse.clicks() == 2 && hit == self.pressed;
                self.pressed = hit;
                match hit {
                    // The second press of a double click acts on the item
                    // the first one selected; a third does nothing new.
                    Some(_) if double => MouseInput::Activate,
                    Some(index) => MouseInput::Select(index),
                    // A click in the pane is ours even when it hits no item.
                    None => MouseInput::Handled,
                }
            }
            MouseEventKind::Down(MouseButton::Right) => match self.item_at(pos) {
                Some(index) => MouseInput::Context(index),
                None => MouseInput::Handled,
            },
            _ => MouseInput::NotHandled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pane showing ten items in five rows, drawn from `offset` on.
    fn pane(offset: usize) -> ListPane {
        ListPane {
            panel_rect: Rect::new(0, 0, 10, 7),
            content_rect: Rect::new(1, 1, 8, 5),
            item_count: 10,
            offset,
            pressed: None,
        }
    }

    fn press(pane: &mut ListPane, row: u16, clicks: u8) -> MouseInput {
        pane.input_mouse(Mouse::new(
            MouseEventKind::Down(MouseButton::Left),
            Position::new(2, row),
            clicks,
        ))
    }

    #[test]
    fn a_press_selects_the_item_it_hits() {
        let mut pane = pane(3);

        assert!(matches!(press(&mut pane, 2, 1), MouseInput::Select(4)));
    }

    #[test]
    fn a_second_press_on_the_item_activates_it() {
        let mut pane = pane(3);
        press(&mut pane, 2, 1);

        assert!(matches!(press(&mut pane, 2, 2), MouseInput::Activate));
    }

    /// Selecting an item may scroll the list, which puts another item
    /// under the pointer. Acting on that one is not what the double click
    /// was aimed at, so it only selects.
    #[test]
    fn a_second_press_on_another_item_selects_it() {
        let mut pane = pane(3);
        press(&mut pane, 2, 1);
        pane.offset = 2;

        assert!(matches!(press(&mut pane, 2, 2), MouseInput::Select(3)));
    }

    #[test]
    fn a_third_press_selects_the_item_again() {
        let mut pane = pane(3);
        press(&mut pane, 2, 1);
        press(&mut pane, 2, 2);

        assert!(matches!(press(&mut pane, 2, 3), MouseInput::Select(4)));
    }
}
