use std::cmp::max;

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::crossterm::event::KeyModifiers;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::prelude::Stylize;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui_textarea::CursorMove;
use ratatui_textarea::TextArea;

use crate::ComponentInputResult;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::ui::Component;
use crate::ui::ComponentAction;
use crate::ui::utils::centered_rect_fixed;

pub struct DescribePopup<'a> {
    head: Head,
    textarea: TextArea<'a>,
}

impl DescribePopup<'_> {
    pub fn new(head: Head, lines: Vec<String>) -> DescribePopup<'static> {
        let mut textarea = TextArea::new(lines);
        textarea.move_cursor(CursorMove::End);
        DescribePopup { head, textarea }
    }
}

impl Component for DescribePopup<'_> {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = Block::bordered()
            .title(Span::styled(" Describe ", Style::new().bold().cyan()))
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green));

        const MAX_COMMIT_WIDTH: u16 = 72;
        const MIN_COMMIT_HEIGHT: u16 = 5;
        let area = centered_rect_fixed(
            area,
            MAX_COMMIT_WIDTH + 2,
            max(MIN_COMMIT_HEIGHT + 4, area.height / 2),
        );
        f.render_widget(Clear, area);
        f.render_widget(&block, area);

        let popup_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(2)])
            .split(block.inner(area));

        f.render_widget(&self.textarea, popup_chunks[0]);

        let help = Paragraph::new(vec!["Ctrl+s: save | Escape: cancel".into()])
            .fg(Color::DarkGray)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        f.render_widget(help, popup_chunks[1]);

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match (key.code, key.modifiers) {
                (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => {
                    new_commander().run_describe(
                        self.head.commit_id.as_str(),
                        &self.textarea.lines().join("\n"),
                    )?;
                    let latest = new_commander().get_head_latest(&self.head)?;
                    return Ok(ComponentInputResult::HandledAction(
                        ComponentAction::Multiple(vec![
                            ComponentAction::SetPopup(None),
                            ComponentAction::ViewLog(latest),
                        ]),
                    ));
                }
                (KeyCode::Esc, _) => {
                    return Ok(ComponentInputResult::HandledAction(
                        ComponentAction::SetPopup(None),
                    ));
                }
                _ => {}
            }
        }
        self.textarea.input(event);
        Ok(ComponentInputResult::Handled)
    }
}
