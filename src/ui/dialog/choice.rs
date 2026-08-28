/*! A popup that lists labelled choices and sends the one picked over a
channel, leaving it to whoever put the popup up to act on it in its own
`update`.
*/

use std::sync::mpsc::Sender;

use anyhow::Result;
use anyhow::anyhow;
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
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::List;
use ratatui::widgets::ListState;
use ratatui::widgets::Paragraph;

use crate::env::JjConfig;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::styles::create_popup_block;
use crate::ui::utils::centered_rect;

const HELP: &str = "j/k: scroll | Enter: select | Escape: cancel";

pub struct ChoicePopup<T> {
    title: &'static str,
    items: Vec<(Line<'static>, T)>,
    /// Rows listed under the choices that cannot be picked
    footnote: Vec<Line<'static>>,
    list_state: ListState,
    list_height: u16,
    config: JjConfig,
    tx: Sender<T>,
}

impl<T> ChoicePopup<T> {
    pub fn new(
        config: JjConfig,
        tx: Sender<T>,
        title: &'static str,
        items: Vec<(Line<'static>, T)>,
    ) -> Self {
        Self {
            title,
            items,
            footnote: vec![],
            list_state: ListState::default().with_selected(Some(0)),
            list_height: 0,
            config,
            tx,
        }
    }

    pub fn footnote(mut self, footnote: Vec<Line<'static>>) -> Self {
        self.footnote = footnote;
        self
    }

    fn scroll(&mut self, delta: isize) {
        self.list_state.select(Some(
            self.list_state
                .selected()
                .map(|selected| selected.saturating_add_signed(delta))
                .unwrap_or(0)
                .min(self.items.len().saturating_sub(1)),
        ));
    }

    fn close() -> Result<ComponentInputResult> {
        Ok(ComponentInputResult::HandledAction(
            AppAction::PopupCanceled,
        ))
    }

    fn confirm(&self) -> Result<ComponentInputResult>
    where
        T: Clone,
    {
        if let Some((_, choice)) = self.list_state.selected().and_then(|i| self.items.get(i)) {
            self.tx
                .send(choice.clone())
                .map_err(|_| anyhow!("Nothing is listening for the choice"))?;
        }
        Self::close()
    }
}

impl<T: Clone> Component for ChoicePopup<T> {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = create_popup_block(self.title);
        let area = centered_rect(area, 50, 60);
        f.render_widget(Clear, area);
        f.render_widget(&block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(2)])
            .split(block.inner(area));

        let rows: Vec<Line<'_>> = self
            .items
            .iter()
            .map(|(label, _)| label.clone())
            .chain(self.footnote.iter().cloned())
            .collect();
        let list = List::new(rows)
            .scroll_padding(3)
            .highlight_style(Style::default().bg(self.config.highlight_color()));

        f.render_stateful_widget(list, chunks[0], &mut self.list_state);
        self.list_height = chunks[0].height;

        let help = Paragraph::new(vec![HELP.into()])
            .fg(Color::DarkGray)
            .alignment(Alignment::Center)
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
                KeyCode::Char('j') | KeyCode::Down => self.scroll(1),
                KeyCode::Char('k') | KeyCode::Up => self.scroll(-1),
                KeyCode::Char('J') => self.scroll(self.list_height as isize / 2),
                KeyCode::Char('K') => self.scroll((self.list_height as isize / 2).saturating_neg()),
                KeyCode::Enter => return self.confirm(),
                KeyCode::Char('q') | KeyCode::Esc => return Self::close(),
                _ => {}
            }
        }
        Ok(ComponentInputResult::Handled)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;
    use std::sync::mpsc::channel;

    use super::*;

    fn popup(count: u8, footnote: usize) -> (ChoicePopup<u8>, Receiver<u8>) {
        let (tx, rx) = channel();
        let items = (0..count)
            .map(|i| (Line::raw(format!("item {i}")), i))
            .collect();
        let popup = ChoicePopup::new(JjConfig::default(), tx, "Choose", items)
            .footnote(vec![Line::raw("footnote"); footnote]);

        (popup, rx)
    }

    fn press(popup: &mut ChoicePopup<u8>, key: KeyCode) -> ComponentInputResult {
        popup
            .input(Event::Key(key.into()))
            .expect("the popup handles key presses")
    }

    #[test]
    fn scrolling_is_clamped_to_the_choices() {
        let (mut popup, _rx) = popup(3, 2);

        for _ in 0..5 {
            press(&mut popup, KeyCode::Char('j'));
        }
        assert_eq!(popup.list_state.selected(), Some(2));

        for _ in 0..5 {
            press(&mut popup, KeyCode::Char('k'));
        }
        assert_eq!(popup.list_state.selected(), Some(0));
    }

    #[test]
    fn enter_sends_the_selected_choice() {
        let (mut popup, rx) = popup(3, 0);

        press(&mut popup, KeyCode::Char('j'));
        assert!(matches!(
            press(&mut popup, KeyCode::Enter),
            ComponentInputResult::HandledAction(AppAction::PopupCanceled)
        ));

        assert_eq!(rx.try_recv(), Ok(1));
    }

    #[test]
    fn cancelling_sends_no_choice() {
        let (mut popup, rx) = popup(3, 0);

        assert!(matches!(
            press(&mut popup, KeyCode::Esc),
            ComponentInputResult::HandledAction(AppAction::PopupCanceled)
        ));

        assert!(rx.try_recv().is_err());
    }
}
