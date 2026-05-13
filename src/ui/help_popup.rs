use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::{self};
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

use crate::ComponentInputResult;
use crate::ui::Component;
use crate::ui::styles::create_popup_block;
use crate::ui::utils::centered_rect_fixed;

pub struct HelpPopup {
    pub main_items: Vec<(String, String)>,
    pub details_items: Vec<(String, String)>,
    pub global_items: Vec<(String, String)>,
    height: u16,
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
            height: 0,
            // Can't use TableState as it's broken: https://github.com/ratatui-org/ratatui/issues/1179
            scroll: 0,
        }
    }

    fn create_table(
        &self,
        items: &[(String, String)],
        title: String,
        scroll: usize,
        key_col_width: u16,
    ) -> Table<'_> {
        let items: Vec<&(String, String)> = items.iter().skip(scroll).collect();
        let rows: Vec<Row> = items
            .iter()
            .map(|row| Row::new([row.0.clone(), row.1.clone()]))
            .collect();
        let widths = [Constraint::Length(key_col_width), Constraint::Fill(1)];
        Table::new(rows, widths).block(Block::new().title(Span::from(title).bold()))
    }
}

impl Component for HelpPopup {
    fn draw(
        &mut self,
        f: &mut ratatui::prelude::Frame<'_>,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        let col_width =
            |items: &[(String, String)]| items.iter().map(|r| r.0.len()).max().unwrap_or(0) as u16;
        let desc_width =
            |items: &[(String, String)]| items.iter().map(|r| r.1.len()).max().unwrap_or(0) as u16;
        let left_key_w = col_width(&self.main_items);
        let right_key_w = col_width(&self.details_items).max(col_width(&self.global_items));
        let left_col_w = left_key_w + 4 + desc_width(&self.main_items);
        let right_desc_w = desc_width(&self.details_items).max(desc_width(&self.global_items));
        let right_col_w = right_key_w + 4 + right_desc_w;
        let required_width = left_col_w + 3 + right_col_w + 4; // +3 separator, +2 borders +2 padding
        let popup_width = required_width.min(area.width);
        let left_height = self.main_items.len() as u16 + 1;
        let details_h = self.details_items.len() as u16 + 1;
        let global_h = self.global_items.len() as u16 + 1;
        let right_height = details_h + global_h + 1;
        let popup_height = (left_height.max(right_height) + 2).min(area.height); // +2 for block borders
        let area = centered_rect_fixed(area, popup_width, popup_height);
        f.render_widget(Clear, area);

        let block = create_popup_block("Help");
        let block_inner = block.inner(area);
        self.height = block_inner.height;
        f.render_widget(&block, area);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(left_col_w),
                Constraint::Length(3),
                Constraint::Length(right_col_w),
            ])
            .split(block_inner);

        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .split(chunks[2]);

        let separator = Rect {
            x: chunks[1].x + chunks[1].width / 2,
            y: chunks[1].y,
            width: 1,
            height: chunks[1].height,
        };
        f.render_widget(Block::new().borders(Borders::LEFT), separator);
        f.render_widget(Block::new().borders(Borders::TOP), right_chunks[1]);

        f.render_widget(
            self.create_table(
                &self.main_items,
                "Main panel".into(),
                self.scroll,
                left_key_w,
            ),
            chunks[0],
        );
        f.render_widget(
            self.create_table(&self.details_items, "Details panel".into(), 0, right_key_w),
            right_chunks[0],
        );
        f.render_widget(
            self.create_table(&self.global_items, "Global".into(), 0, right_key_w),
            right_chunks[2],
        );

        Ok(())
    }

    fn input(&mut self, event: Event) -> anyhow::Result<crate::ComponentInputResult> {
        if let Event::Key(key) = event
            && key.kind == event::KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('j') => {
                    self.scroll = (self.scroll + 1)
                        .min(self.main_items.len().saturating_sub(self.height as usize));
                }
                KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
                _ => return Ok(ComponentInputResult::NotHandled),
            }

            return Ok(ComponentInputResult::Handled);
        }

        Ok(ComponentInputResult::NotHandled)
    }
}
