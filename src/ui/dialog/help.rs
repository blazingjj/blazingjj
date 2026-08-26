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
use crate::ui::utils::centered_rect;

/// Space between the description and the key column of a table
const COLUMN_SPACING: u16 = 4;

/// Width of the gap holding the vertical rule between the two halves
const SEPARATOR_WIDTH: u16 = 3;

fn key_width(items: &[(String, String)]) -> u16 {
    items.iter().map(|(key, _)| key.len()).max().unwrap_or(0) as u16
}

/// How far the tables have to scroll for the last item of the longest one to
/// come into view.
fn max_scroll(tables: [(usize, Rect); 3]) -> usize {
    tables
        .into_iter()
        // A table spends its first row on its title.
        .map(|(items, area)| items.saturating_sub(area.height.saturating_sub(1) as usize))
        .max()
        .unwrap_or(0)
}

pub struct HelpPopup {
    pub main_items: Vec<(String, String)>,
    pub details_items: Vec<(String, String)>,
    pub global_items: Vec<(String, String)>,
    max_scroll: usize,
    scroll: usize,
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

    fn create_table(&self, items: &[(String, String)], title: String) -> Table<'_> {
        let rows: Vec<Row> = items
            .iter()
            .skip(self.scroll)
            .map(|(key, description)| Row::new([description.clone(), key.clone()]))
            .collect();
        let widths = [Constraint::Fill(1), Constraint::Length(key_width(items))];

        Table::new(rows, widths)
            .column_spacing(COLUMN_SPACING)
            .block(
                Block::new()
                    .title(Span::from(title).bold().underlined())
                    .title_alignment(Alignment::Center),
            )
    }

    fn do_scroll(&mut self, delta: isize) {
        let max = self.max_scroll as isize;
        self.scroll = (self.scroll as isize + delta).clamp(0, max) as usize;
    }
}

impl Component for HelpPopup {
    fn draw(
        &mut self,
        f: &mut ratatui::prelude::Frame<'_>,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        let area = centered_rect(area, 60, 60);
        f.render_widget(Clear, area);

        let block = create_popup_block("Help");
        let block_inner = block.inner(area);
        f.render_widget(&block, area);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(SEPARATOR_WIDTH),
                Constraint::Fill(1),
            ])
            .split(block_inner);

        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(self.details_items.len() as u16 + 1),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .split(chunks[2]);

        self.max_scroll = max_scroll([
            (self.main_items.len(), chunks[0]),
            (self.details_items.len(), right_chunks[0]),
            (self.global_items.len(), right_chunks[2]),
        ]);
        self.scroll = self.scroll.min(self.max_scroll);

        f.render_widget(
            Block::new().borders(Borders::LEFT),
            Rect {
                x: chunks[1].x + chunks[1].width / 2,
                ..chunks[1]
            },
        );
        f.render_widget(Block::new().borders(Borders::TOP), right_chunks[1]);

        f.render_widget(
            self.create_table(&self.main_items, "Main panel".into()),
            chunks[0],
        );
        f.render_widget(
            self.create_table(&self.details_items, "Details panel".into()),
            right_chunks[0],
        );
        f.render_widget(
            self.create_table(&self.global_items, "Global".into()),
            right_chunks[2],
        );

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

    fn wheel(kind: MouseEventKind) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        })
    }

    #[test]
    fn test_no_scroll_when_every_table_fits() {
        assert_eq!(max_scroll([(3, area(10)), (5, area(6)), (14, area(15))]), 0);
    }

    #[test]
    fn test_scroll_follows_the_table_that_overflows_most() {
        // The global list is 14 items in 9 rows, one of which holds its title.
        assert_eq!(max_scroll([(3, area(22)), (5, area(6)), (14, area(9))]), 6);
        // The main panel overflows further than the global list does.
        assert_eq!(max_scroll([(30, area(22)), (5, area(6)), (14, area(9))]), 9);
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
