use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::List;
use ratatui::widgets::ListState;
use ratatui::widgets::Paragraph;

use crate::ComponentInputResult;
use crate::commander::log::Head;
use crate::env::JjConfig;
use crate::ui::Component;
use crate::ui::ComponentAction;
use crate::ui::utils::centered_rect;

pub struct ParentSelectPopup {
    parents: Vec<(Head, String)>,
    list_state: ListState,
    list_height: u16,
    config: JjConfig,
    tx: std::sync::mpsc::Sender<Option<Head>>,
}

impl ParentSelectPopup {
    pub fn new(
        parents: Vec<(Head, String)>,
        config: JjConfig,
        tx: std::sync::mpsc::Sender<Option<Head>>,
    ) -> Self {
        Self {
            parents,
            list_state: ListState::default().with_selected(Some(0)),
            list_height: 0,
            config,
            tx,
        }
    }

    fn scroll(&mut self, scroll: isize) {
        self.list_state.select(Some(
            self.list_state
                .selected()
                .map(|s| s.saturating_add_signed(scroll))
                .unwrap_or(0)
                .min(self.parents.len().saturating_sub(1)),
        ));
    }
}

impl Component for ParentSelectPopup {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = Block::bordered()
            .title(Span::styled(" Select parent ", Style::new().bold().cyan()))
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green));
        let area = centered_rect(area, 50, 60);
        f.render_widget(Clear, area);
        f.render_widget(&block, area);

        let popup_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(2)])
            .split(block.inner(area));

        let list_items = self.parents.iter().map(|(head, desc)| {
            let short_id: String = head.change_id.as_str().chars().take(8).collect();
            let desc_span = if desc.trim().is_empty() {
                Span::styled(" (no description)", Style::default().fg(Color::DarkGray))
            } else {
                let first_line = desc.lines().next().unwrap_or("");
                Span::styled(format!(" {first_line}"), Style::default())
            };
            ratatui::text::Text::from(ratatui::text::Line::from(vec![
                Span::styled(short_id, Style::default().fg(Color::Magenta)),
                desc_span,
            ]))
        });

        let list = List::new(list_items)
            .scroll_padding(3)
            .highlight_style(Style::default().bg(self.config.highlight_color()));

        f.render_stateful_widget(list, popup_chunks[0], &mut self.list_state);
        self.list_height = popup_chunks[0].height;

        let help = Paragraph::new(vec!["j/k: scroll | Enter: select | Escape: cancel".into()])
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
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => self.scroll(1),
                KeyCode::Char('k') | KeyCode::Up => self.scroll(-1),
                KeyCode::Char('J') => self.scroll(self.list_height as isize / 2),
                KeyCode::Char('K') => self.scroll((self.list_height as isize / 2).saturating_neg()),
                KeyCode::Enter => {
                    if let Some(idx) = self.list_state.selected() {
                        if let Some((head, _)) = self.parents.get(idx) {
                            self.tx.send(Some(head.clone()))?;
                            return Ok(ComponentInputResult::HandledAction(
                                ComponentAction::SetPopup(None),
                            ));
                        }
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.tx.send(None)?;
                    return Ok(ComponentInputResult::HandledAction(
                        ComponentAction::SetPopup(None),
                    ));
                }
                _ => {}
            }
        }
        Ok(ComponentInputResult::Handled)
    }
}
