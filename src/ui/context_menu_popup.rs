/*! A popup that lists common single-commit operations to run against
the currently selected change. The chosen action is sent as a
`LogTabEvent` over a channel; `LogTab::update` picks it up and
dispatches it through the regular event handler so that all the
existing pre-flight checks (immutability, confirmations, …) apply.
*/

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Position;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::List;
use ratatui::widgets::ListItem;
use ratatui::widgets::ListState;
use ratatui::widgets::Paragraph;

use crate::ComponentInputResult;
use crate::env::JjConfig;
use crate::keybinds::LogTabEvent;
use crate::ui::Component;
use crate::ui::ComponentAction;
use crate::ui::styles::create_popup_block;
use crate::ui::utils::anchored_rect_fixed;
use crate::ui::utils::centered_rect_fixed;

pub struct ContextMenuPopup {
    items: Vec<(String, LogTabEvent)>,
    list_state: ListState,
    config: JjConfig,
    tx: std::sync::mpsc::Sender<LogTabEvent>,
    /// Top-left of the popup. When `None`, the popup is centered.
    anchor: Option<Position>,
    /// Whole popup area, updated on every draw.
    popup_rect: Rect,
    /// List area inside the popup, updated on every draw.
    list_rect: Rect,
}

impl ContextMenuPopup {
    pub fn new(
        config: JjConfig,
        tx: std::sync::mpsc::Sender<LogTabEvent>,
        anchor: Option<Position>,
        selected_is_at: bool,
    ) -> Self {
        let mut items: Vec<(String, LogTabEvent)> = vec![
            (
                "Edit".into(),
                LogTabEvent::EditChange {
                    ignore_immutable: false,
                },
            ),
            (
                "New child".into(),
                LogTabEvent::CreateNew { describe: false },
            ),
            (
                "New child & describe".into(),
                LogTabEvent::CreateNew { describe: true },
            ),
            (
                "New after".into(),
                LogTabEvent::CreateNewAfter { describe: false },
            ),
            (
                "New after & describe".into(),
                LogTabEvent::CreateNewAfter { describe: true },
            ),
            (
                "New before".into(),
                LogTabEvent::CreateNewBefore { describe: false },
            ),
            (
                "New before & describe".into(),
                LogTabEvent::CreateNewBefore { describe: true },
            ),
            ("Describe".into(), LogTabEvent::Describe),
            ("Absorb".into(), LogTabEvent::Absorb),
            ("Abandon".into(), LogTabEvent::Abandon),
            ("Duplicate".into(), LogTabEvent::Duplicate),
        ];
        if !selected_is_at {
            items.extend([
                (
                    "Squash @ into this".into(),
                    LogTabEvent::Squash {
                        ignore_immutable: false,
                    },
                ),
                ("Rebase @ to this".into(), LogTabEvent::Rebase),
            ]);
        }
        items.extend([
            ("Set bookmark".into(), LogTabEvent::SetBookmark),
            ("Copy change id".into(), LogTabEvent::CopyChangeId),
            ("Copy commit id".into(), LogTabEvent::CopyRev),
        ]);

        Self {
            items,
            list_state: ListState::default().with_selected(Some(0)),
            config,
            tx,
            anchor,
            popup_rect: Rect::ZERO,
            list_rect: Rect::ZERO,
        }
    }

    fn scroll(&mut self, delta: isize) {
        let max = self.items.len().saturating_sub(1);
        let next = self
            .list_state
            .selected()
            .map(|i| i.saturating_add_signed(delta).min(max))
            .unwrap_or(0);
        self.list_state.select(Some(next));
    }

    fn close() -> Result<ComponentInputResult> {
        Ok(ComponentInputResult::HandledAction(
            ComponentAction::SetPopup(None),
        ))
    }

    fn confirm(&self) -> Result<ComponentInputResult> {
        if let Some(event) = self
            .list_state
            .selected()
            .and_then(|i| self.items.get(i))
            .map(|(_, event)| *event)
        {
            self.tx.send(event)?;
        }
        Self::close()
    }
}

impl Component for ContextMenuPopup {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let width: u16 = self
            .items
            .iter()
            .map(|(label, _)| label.chars().count())
            .max()
            .unwrap_or(20)
            .max(20) as u16
            + 4;
        // items + title border + bottom help
        let height: u16 = self.items.len() as u16 + 4;
        let area = match self.anchor {
            Some(anchor) => anchored_rect_fixed(area, anchor, width, height),
            None => centered_rect_fixed(area, width, height),
        };
        self.popup_rect = area;

        f.render_widget(Clear, area);
        let block = create_popup_block("Actions");
        f.render_widget(&block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(2)])
            .split(block.inner(area));
        self.list_rect = chunks[0];

        let list_items: Vec<ListItem> = self
            .items
            .iter()
            .map(|(label, _)| ListItem::new(Line::from(Span::raw(label.clone()))))
            .collect();
        let list = List::new(list_items)
            .scroll_padding(1)
            .highlight_style(Style::default().bg(self.config.highlight_color()));
        f.render_stateful_widget(list, chunks[0], &mut self.list_state);

        let help = Paragraph::new(vec!["Enter: run | j/k: scroll | Esc: cancel".into()])
            .fg(Color::DarkGray)
            .alignment(ratatui::layout::Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        f.render_widget(help, chunks[1]);

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.scroll(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.scroll(-1);
                }
                KeyCode::Enter => return self.confirm(),
                KeyCode::Esc | KeyCode::Char('q') => return Self::close(),
                _ => return Ok(ComponentInputResult::NotHandled),
            }
            return Ok(ComponentInputResult::Handled);
        }
        if let Event::Mouse(mouse_event) = event {
            let mouse_pos = Position::new(mouse_event.column, mouse_event.row);
            match mouse_event.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll(-1);
                    return Ok(ComponentInputResult::Handled);
                }
                MouseEventKind::ScrollDown => {
                    self.scroll(1);
                    return Ok(ComponentInputResult::Handled);
                }
                MouseEventKind::Up(_) => {
                    if !self.popup_rect.contains(mouse_pos) {
                        return Self::close();
                    }
                    if self.list_rect.contains(mouse_pos) {
                        let row_offset = (mouse_pos.y - self.list_rect.y) as usize;
                        let index = self.list_state.offset() + row_offset;
                        if index < self.items.len() {
                            self.list_state.select(Some(index));
                            return self.confirm();
                        }
                    }
                    return Ok(ComponentInputResult::Handled);
                }
                _ => {}
            }
        }
        Ok(ComponentInputResult::NotHandled)
    }
}
