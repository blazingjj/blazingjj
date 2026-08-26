use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::crossterm::event::{self};
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Row;
use ratatui::widgets::Table;

use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::styles::create_popup_block;
use crate::ui::utils::centered_rect_fixed;

/// Space between the description and the key column of a table
const COLUMN_SPACING: u16 = 4;

/// Width of the gap holding the vertical rule between the two halves
const SEPARATOR_WIDTH: u16 = 3;

fn key_width(items: &[(String, String)]) -> u16 {
    items.iter().map(|(key, _)| key.len()).max().unwrap_or(0) as u16
}

fn description_width(items: &[(String, String)]) -> u16 {
    items
        .iter()
        .map(|(_, description)| description.len())
        .max()
        .unwrap_or(0) as u16
}

/// How much wider and taller than its contents a `block` is, in the `area` it
/// has to draw itself in. Its border is not all of it, it also pads.
fn chrome(block: &Block, area: Rect) -> [u16; 2] {
    let inner = block.inner(area);

    [
        area.width.saturating_sub(inner.width),
        area.height.saturating_sub(inner.height),
    ]
}

/// Where a part of the popup's contents that is `height` rows tall and starts
/// at row `top` of them lands in `viewport`, and how many of its rows `scroll`
/// has taken off the top. `None` once it has scrolled out of view entirely.
fn place(viewport: Rect, scroll: u16, top: u16, height: u16) -> Option<(Rect, u16)> {
    let skipped = scroll.saturating_sub(top);
    let y = viewport.y + top.saturating_sub(scroll);
    let height = height
        .saturating_sub(skipped)
        .min(viewport.bottom().saturating_sub(y));

    (height > 0).then_some((
        Rect {
            y,
            height,
            ..viewport
        },
        skipped,
    ))
}

pub struct HelpPopup {
    pub main_items: Vec<(String, String)>,
    pub details_items: Vec<(String, String)>,
    pub global_items: Vec<(String, String)>,
    max_scroll: u16,
    scroll: u16,
}

impl HelpPopup {
    pub fn new(
        main_items: Vec<(String, String)>,
        details_items: Vec<(String, String)>,
        global_items: Vec<(String, String)>,
    ) -> Self {
        Self {
            main_items,
            details_items,
            global_items,
            max_scroll: 0,
            // Can't use TableState as it's broken: https://github.com/ratatui-org/ratatui/issues/1179
            scroll: 0,
        }
    }

    /// A table of `items` under its `title`, with `skipped` of its rows taken
    /// off the top. The title is the first of those rows.
    fn create_table(
        &self,
        items: &[(String, String)],
        title: &'static str,
        key_width: u16,
        skipped: u16,
    ) -> Table<'_> {
        let rows: Vec<Row> = items
            .iter()
            .skip(skipped.saturating_sub(1) as usize)
            .map(|(key, description)| Row::new([description.clone(), key.clone()]))
            .collect();
        let widths = [Constraint::Fill(1), Constraint::Length(key_width)];
        let table = Table::new(rows, widths).column_spacing(COLUMN_SPACING);

        if skipped > 0 {
            return table;
        }

        table.block(
            Block::new()
                .title(Span::from(title).bold().underlined())
                .title_alignment(Alignment::Center),
        )
    }

    fn do_scroll(&mut self, delta: i32) {
        let max = i32::from(self.max_scroll);
        self.scroll = (i32::from(self.scroll) + delta).clamp(0, max) as u16;
    }
}

impl Component for HelpPopup {
    fn draw(
        &mut self,
        f: &mut ratatui::prelude::Frame<'_>,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        let left_key_width = key_width(&self.main_items);
        let left_width = left_key_width + COLUMN_SPACING + description_width(&self.main_items);

        // The stacked tables share a key column so that their rows line up
        let right_key_width = key_width(&self.details_items).max(key_width(&self.global_items));
        let right_width = right_key_width
            + COLUMN_SPACING
            + description_width(&self.details_items).max(description_width(&self.global_items));

        // Each table takes a line for its title, the right half one for its rule
        let left_height = self.main_items.len() as u16 + 1;
        let right_height = self.details_items.len() as u16 + self.global_items.len() as u16 + 3;

        let block = create_popup_block("Help");
        let [extra_width, extra_height] = chrome(&block, area);
        // The two halves and the separator between them
        let contents_width = left_width + SEPARATOR_WIDTH + right_width;
        let width = (contents_width + extra_width).min(area.width);
        let height = (left_height.max(right_height) + extra_height).min(area.height);

        let area = centered_rect_fixed(area, width, height);
        f.render_widget(Clear, area);

        let block_inner = block.inner(area);
        f.render_widget(&block, area);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(left_width),
                Constraint::Length(SEPARATOR_WIDTH),
                Constraint::Length(right_width),
            ])
            .split(block_inner);

        self.max_scroll = left_height
            .max(right_height)
            .saturating_sub(block_inner.height);
        self.scroll = self.scroll.min(self.max_scroll);

        f.render_widget(
            Block::new().borders(Borders::LEFT),
            Rect {
                x: chunks[1].x + chunks[1].width / 2,
                ..chunks[1]
            },
        );

        if let Some((area, skipped)) = place(chunks[0], self.scroll, 0, left_height) {
            let table = self.create_table(&self.main_items, "Main panel", left_key_width, skipped);
            f.render_widget(table, area);
        }

        // Both halves scroll as one, so the right one lays its two tables and
        // the rule between them out at their full height and lets the scroll
        // take them out of view.
        let details_height = self.details_items.len() as u16 + 1;
        if let Some((area, skipped)) = place(chunks[2], self.scroll, 0, details_height) {
            let table = self.create_table(
                &self.details_items,
                "Details panel",
                right_key_width,
                skipped,
            );
            f.render_widget(table, area);
        }
        if let Some((area, _)) = place(chunks[2], self.scroll, details_height, 1) {
            f.render_widget(Block::new().borders(Borders::TOP), area);
        }
        if let Some((area, skipped)) = place(
            chunks[2],
            self.scroll,
            details_height + 1,
            self.global_items.len() as u16 + 1,
        ) {
            let table = self.create_table(&self.global_items, "Global", right_key_width, skipped);
            f.render_widget(table, area);
        }

        Ok(())
    }

    fn input(&mut self, event: Event) -> anyhow::Result<ComponentInputResult> {
        match event {
            Event::Key(key) if key.kind == event::KeyEventKind::Press => {
                let delta = match key.code {
                    KeyCode::Char('j') => 1,
                    KeyCode::Char('k') => -1,
                    _ => return Ok(ComponentInputResult::NotHandled),
                };

                self.do_scroll(delta);
                Ok(ComponentInputResult::Handled)
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => {
                    self.do_scroll(3);
                    Ok(ComponentInputResult::Handled)
                }
                MouseEventKind::ScrollUp => {
                    self.do_scroll(-3);
                    Ok(ComponentInputResult::Handled)
                }
                _ => Ok(ComponentInputResult::NotHandled),
            },
            _ => Ok(ComponentInputResult::NotHandled),
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyModifiers;
    use ratatui::crossterm::event::MouseEvent;

    use super::*;

    fn area(height: u16) -> Rect {
        Rect::new(0, 0, 40, height)
    }

    fn rect(y: u16, height: u16) -> Rect {
        Rect::new(0, y, 40, height)
    }

    fn wheel(kind: MouseEventKind) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        })
    }

    #[test]
    fn test_unscrolled_contents_keep_their_place() {
        assert_eq!(place(area(10), 0, 0, 6), Some((rect(0, 6), 0)));
        // What does not fit is cut off at the bottom of the viewport.
        assert_eq!(place(area(10), 0, 8, 5), Some((rect(8, 2), 0)));
    }

    #[test]
    fn test_scrolling_takes_rows_off_the_top() {
        // The scroll eats into a part of the contents ...
        assert_eq!(place(area(10), 3, 0, 6), Some((rect(0, 3), 3)));
        // ... only once it has consumed everything above it.
        assert_eq!(place(area(10), 3, 5, 4), Some((rect(2, 4), 0)));
    }

    #[test]
    fn test_parts_scrolled_past_are_gone() {
        assert_eq!(place(area(10), 8, 0, 6), None);
    }

    #[test]
    fn test_scroll_is_clamped_to_max() {
        let mut popup = HelpPopup::new(vec![], vec![], vec![]);
        popup.max_scroll = 2;

        for _ in 0..5 {
            popup
                .input(Event::Key(KeyCode::Char('j').into()))
                .expect("scrolling down should be handled");
        }
        assert_eq!(popup.scroll, 2);

        for _ in 0..5 {
            popup
                .input(Event::Key(KeyCode::Char('k').into()))
                .expect("scrolling up should be handled");
        }
        assert_eq!(popup.scroll, 0);
    }

    #[test]
    fn test_the_wheel_scrolls_more_than_a_row_at_a_time() {
        let mut popup = HelpPopup::new(vec![], vec![], vec![]);
        popup.max_scroll = 10;

        popup
            .input(wheel(MouseEventKind::ScrollDown))
            .expect("scrolling down should be handled");
        assert_eq!(popup.scroll, 3);

        popup
            .input(wheel(MouseEventKind::ScrollUp))
            .expect("scrolling up should be handled");
        assert_eq!(popup.scroll, 0);
    }
}
