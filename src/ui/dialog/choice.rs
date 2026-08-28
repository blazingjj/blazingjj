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
use ratatui::crossterm::event::MouseButton;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Position;
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
use crate::ui::utils::anchored_rect_fixed;
use crate::ui::utils::centered_rect;
use crate::ui::utils::centered_rect_fixed;
use crate::ui::utils::chrome;

const HELP: &str = "j/k: scroll | Enter: select | Escape: cancel";

/// The help line and the border it sits under
const HELP_HEIGHT: u16 = 2;

pub struct ChoicePopup<T> {
    title: &'static str,
    items: Vec<(Line<'static>, T)>,
    /// Rows listed under the choices that cannot be picked
    footnote: Vec<Line<'static>>,
    list_state: ListState,
    /// Top-left of the popup. When `None`, the popup is centered.
    anchor: Option<Position>,
    /// Whole popup area, updated on every draw
    popup_area: Rect,
    /// List area inside the popup, updated on every draw
    list_area: Rect,
    config: JjConfig,
    tx: Sender<T>,
}

impl<T> ChoicePopup<T> {
    pub fn new(
        config: JjConfig,
        tx: Sender<T>,
        anchor: Option<Position>,
        title: &'static str,
        items: Vec<(Line<'static>, T)>,
    ) -> Self {
        Self {
            title,
            items,
            footnote: vec![],
            list_state: ListState::default().with_selected(Some(0)),
            anchor,
            popup_area: Rect::ZERO,
            list_area: Rect::ZERO,
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

    /// Every row the list holds, pickable or not
    fn rows(&self) -> impl Iterator<Item = &Line<'static>> {
        self.items
            .iter()
            .map(|(label, _)| label)
            .chain(self.footnote.iter())
    }

    /// Where to put the popup in `area`: at its anchor or centered, and
    /// no larger than the list needs, up to the share of the screen we
    /// are willing to take.
    fn popup_rect(&self, area: Rect, block: &Block) -> Rect {
        let max = centered_rect(area, 50, 60);
        let [extra_width, extra_height] = chrome(block, max);

        let rows_width = self.rows().map(Line::width).max().unwrap_or(0) as u16;
        // The title sits in the top border, padded by a space on either
        // side and between the two corners.
        let title_width = Line::raw(self.title).width() as u16 + 4;
        let width = rows_width
            .max(Line::raw(HELP).width() as u16)
            .saturating_add(extra_width)
            .max(title_width)
            .min(max.width);
        let height = (self.rows().count() as u16 + HELP_HEIGHT)
            .saturating_add(extra_height)
            .min(max.height);

        match self.anchor {
            Some(anchor) => anchored_rect_fixed(area, anchor, width, height),
            None => centered_rect_fixed(area, width, height),
        }
    }

    /// The choice `pos` points at, if it points at one at all. The
    /// footnote rows are listed below the choices and cannot be picked.
    fn item_at(&self, pos: Position) -> Option<usize> {
        if !self.list_area.contains(pos) {
            return None;
        }
        let row = (pos.y - self.list_area.y) as usize;
        let index = self.list_state.offset() + row;

        (index < self.items.len()).then_some(index)
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
        let area = self.popup_rect(area, &block);
        self.popup_area = area;
        f.render_widget(Clear, area);
        f.render_widget(&block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(HELP_HEIGHT)])
            .split(block.inner(area));
        self.list_area = chunks[0];

        let rows: Vec<Line<'static>> = self.rows().cloned().collect();
        let list = List::new(rows)
            .scroll_padding(3)
            .highlight_style(Style::default().bg(self.config.highlight_color()));

        f.render_stateful_widget(list, chunks[0], &mut self.list_state);

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
        match event {
            Event::Key(key) => match key.code {
                KeyCode::Char('j') | KeyCode::Down => self.scroll(1),
                KeyCode::Char('k') | KeyCode::Up => self.scroll(-1),
                KeyCode::Char('J') => self.scroll(self.list_area.height as isize / 2),
                KeyCode::Char('K') => {
                    self.scroll((self.list_area.height as isize / 2).saturating_neg())
                }
                KeyCode::Enter => return self.confirm(),
                KeyCode::Char('q') | KeyCode::Esc => return Self::close(),
                _ => {}
            },
            // Where the popup sits is only known once it has been drawn,
            // and events are handled in batches without a draw between
            // them, so the one that opened us may be in this very batch.
            Event::Mouse(mouse) if !self.popup_area.is_empty() => {
                let pos = Position::new(mouse.column, mouse.row);
                match mouse.kind {
                    MouseEventKind::ScrollUp => self.scroll(-1),
                    MouseEventKind::ScrollDown => self.scroll(1),
                    MouseEventKind::Down(MouseButton::Left) => {
                        if !self.popup_area.contains(pos) {
                            return Self::close();
                        }
                        if let Some(index) = self.item_at(pos) {
                            self.list_state.select(Some(index));
                            return self.confirm();
                        }
                    }
                    // A right click outside still names what it hit, so
                    // we let it through rather than only taking it as a
                    // dismissal.
                    MouseEventKind::Down(MouseButton::Right) => {
                        if self.popup_area.contains(pos) {
                            return Self::close();
                        }
                        return Ok(ComponentInputResult::Dismissed);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(ComponentInputResult::Handled)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;
    use std::sync::mpsc::channel;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyModifiers;
    use ratatui::crossterm::event::MouseEvent;

    use super::*;

    fn popup(count: u8, footnote: usize) -> (ChoicePopup<u8>, Receiver<u8>) {
        let (tx, rx) = channel();
        let items = (0..count)
            .map(|i| (Line::raw(format!("item {i}")), i))
            .collect();
        let popup = ChoicePopup::new(JjConfig::default(), tx, None, "Choose", items)
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

    fn mouse(
        popup: &mut ChoicePopup<u8>,
        kind: MouseEventKind,
        column: u16,
        row: u16,
    ) -> ComponentInputResult {
        popup
            .input(Event::Mouse(MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }))
            .expect("the popup handles mouse events")
    }

    fn click(popup: &mut ChoicePopup<u8>, column: u16, row: u16) -> ComponentInputResult {
        mouse(popup, MouseEventKind::Down(MouseButton::Left), column, row)
    }

    /// A wheel notch over the middle of a popup drawn by `draw`
    fn wheel(popup: &mut ChoicePopup<u8>, kind: MouseEventKind) -> ComponentInputResult {
        mouse(popup, kind, 50, 20)
    }

    /// Put the popup on a 100x40 screen, so that it knows where it is
    fn draw(popup: &mut ChoicePopup<u8>) {
        Terminal::new(TestBackend::new(100, 40))
            .expect("the test backend")
            .draw(|f| popup.draw(f, f.area()).expect("the popup draws"))
            .expect("the frame is drawn");
    }

    #[test]
    fn the_hit_test_looks_where_the_popup_was_drawn() {
        let (mut popup, _rx) = popup(3, 2);

        draw(&mut popup);

        // Five rows and the help line under its own border, centered
        assert_eq!(popup.popup_area, Rect::new(26, 15, 48, 9));
        // Inside the popup border and its padding, above the help line
        assert_eq!(popup.list_area, Rect::new(28, 16, 44, 5));
    }

    #[test]
    fn clicking_a_choice_sends_it() {
        let (mut popup, rx) = popup(3, 2);
        draw(&mut popup);

        // The third row of the list
        assert!(matches!(
            click(&mut popup, 30, 18),
            ComponentInputResult::HandledAction(AppAction::PopupCanceled)
        ));

        assert_eq!(rx.try_recv(), Ok(2));
    }

    #[test]
    fn clicking_a_choice_in_a_scrolled_list_sends_it() {
        let (mut popup, rx) = popup(30, 0);
        draw(&mut popup);
        for _ in 0..29 {
            press(&mut popup, KeyCode::Char('j'));
        }
        // The list is scrolled only when it is drawn again
        draw(&mut popup);
        assert_eq!(popup.list_state.offset(), 10);

        // The topmost row on show, which is the eleventh choice
        let top = popup.list_area.y;
        assert!(matches!(
            click(&mut popup, 30, top),
            ComponentInputResult::HandledAction(AppAction::PopupCanceled)
        ));

        assert_eq!(rx.try_recv(), Ok(10));
    }

    #[test]
    fn clicking_the_footnote_sends_nothing() {
        let (mut popup, rx) = popup(3, 2);
        draw(&mut popup);

        // The first row below the three choices
        assert!(matches!(
            click(&mut popup, 30, 19),
            ComponentInputResult::Handled
        ));

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn clicking_the_help_line_sends_nothing_and_stays_open() {
        let (mut popup, rx) = popup(3, 2);
        draw(&mut popup);

        // Below the list, inside the popup
        assert!(matches!(
            click(&mut popup, 30, 21),
            ComponentInputResult::Handled
        ));

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn clicking_outside_the_popup_cancels() {
        let (mut popup, rx) = popup(3, 0);
        draw(&mut popup);

        assert!(matches!(
            click(&mut popup, 0, 0),
            ComponentInputResult::HandledAction(AppAction::PopupCanceled)
        ));

        assert!(rx.try_recv().is_err());
    }

    fn right_click(popup: &mut ChoicePopup<u8>, column: u16, row: u16) -> ComponentInputResult {
        mouse(popup, MouseEventKind::Down(MouseButton::Right), column, row)
    }

    #[test]
    fn right_clicking_a_choice_cancels_without_sending_it() {
        let (mut popup, rx) = popup(3, 2);
        draw(&mut popup);

        assert!(matches!(
            right_click(&mut popup, 30, 18),
            ComponentInputResult::HandledAction(AppAction::PopupCanceled)
        ));

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn right_clicking_outside_the_popup_dismisses_it_and_leaves_the_event_to_others() {
        let (mut popup, rx) = popup(3, 0);
        draw(&mut popup);

        assert!(matches!(
            right_click(&mut popup, 0, 0),
            ComponentInputResult::Dismissed
        ));

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn the_wheel_moves_the_selection_over_the_choices() {
        let (mut popup, _rx) = popup(3, 2);
        draw(&mut popup);

        for _ in 0..5 {
            wheel(&mut popup, MouseEventKind::ScrollDown);
        }
        assert_eq!(popup.list_state.selected(), Some(2));

        for _ in 0..5 {
            wheel(&mut popup, MouseEventKind::ScrollUp);
        }
        assert_eq!(popup.list_state.selected(), Some(0));
    }

    #[test]
    fn clicking_before_the_popup_is_drawn_does_not_cancel_it() {
        let (mut popup, rx) = popup(3, 0);

        assert!(matches!(
            click(&mut popup, 0, 0),
            ComponentInputResult::Handled
        ));

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn the_popup_is_no_larger_than_the_list() {
        let (popup, _rx) = popup(3, 2);
        let block = create_popup_block("Choose");

        let rect = popup.popup_rect(Rect::new(0, 0, 100, 40), &block);

        // The five rows, plus the popup border around them
        assert_eq!(rect.height, 5 + HELP_HEIGHT + 2);
        // The help line is wider than any row, and the popup borders and
        // pads around it
        assert_eq!(rect.width, Line::raw(HELP).width() as u16 + 4);
    }

    #[test]
    fn a_row_wider_than_the_help_line_widens_the_popup() {
        let (tx, _rx) = channel();
        let label = "x".repeat(Line::raw(HELP).width() + 10);
        let popup = ChoicePopup::new(
            JjConfig::default(),
            tx,
            None,
            "Choose",
            vec![(Line::raw(label.clone()), 0u8)],
        );

        let rect = popup.popup_rect(Rect::new(0, 0, 200, 40), &create_popup_block("Choose"));

        assert_eq!(rect.width, label.len() as u16 + 4);
    }

    #[test]
    fn a_long_list_stops_at_the_share_of_the_screen_we_take() {
        let (popup, _rx) = popup(200, 0);

        let rect = popup.popup_rect(Rect::new(0, 0, 100, 40), &create_popup_block("Choose"));

        assert_eq!(rect.height, 24);
    }

    #[test]
    fn a_wide_row_stops_at_the_share_of_the_screen_we_take() {
        let (tx, _rx) = channel();
        let popup = ChoicePopup::new(
            JjConfig::default(),
            tx,
            None,
            "Choose",
            vec![(Line::raw("x".repeat(200)), 0u8)],
        );

        let rect = popup.popup_rect(Rect::new(0, 0, 100, 40), &create_popup_block("Choose"));

        assert_eq!(rect.width, 50);
    }

    #[test]
    fn a_title_wider_than_the_rows_still_fits_between_the_corners() {
        let (tx, _rx) = channel();
        let title = "A rather wordy popup title that outgrows its help line";
        let popup = ChoicePopup::new(
            JjConfig::default(),
            tx,
            None,
            title,
            vec![(Line::raw("x"), 0u8)],
        );

        let rect = popup.popup_rect(Rect::new(0, 0, 200, 40), &create_popup_block(title));

        assert_eq!(rect.width, title.len() as u16 + 4);
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
