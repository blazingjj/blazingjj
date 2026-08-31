/*! A yes/no question, put with the action to take when the answer is
yes.
*/

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Clear;
use ratatui::widgets::Padding;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;

use crate::env::JjConfig;
use crate::keybinds::ConfirmPopupEvent;
use crate::keybinds::ConfirmPopupKeybinds;
use crate::keybinds::PopupEvent;
use crate::keybinds::PopupKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::utils::centered_rect_fixed;
use crate::ui::utils::mark_key;

const YES: &str = "Yes";
const NO: &str = "No";

/// Blank cells around the question, and either side of a button label.
const PADDING: u16 = 2;

/// The narrowest we put a question, however short it is.
const MIN_WIDTH: u16 = 40;

/// The button row, which sits on the bottom border.
const BUTTONS_HEIGHT: u16 = 2;

pub struct ConfirmPopup {
    title: &'static str,
    question: Text<'static>,
    /// What a yes asks for, taken as the popup raises it
    confirmed: Option<AppAction>,
    /// Whether Enter presses yes rather than no
    yes_selected: bool,
    /// First row of the question on show, for a question too long to fit
    scroll: usize,
    /// Rows the question is shown in, updated on every draw
    question_height: u16,
    /// Rows the question takes once wrapped, updated on every draw
    question_rows: usize,
    config: JjConfig,
    keybinds: PopupKeybinds,
    own_keybinds: ConfirmPopupKeybinds,
}

impl ConfirmPopup {
    /// A question titled `title`, raising `confirmed` when answered yes.
    pub fn new(
        config: JjConfig,
        title: &'static str,
        question: Text<'static>,
        confirmed: AppAction,
    ) -> Self {
        Self {
            title,
            question,
            confirmed: Some(confirmed),
            yes_selected: true,
            scroll: 0,
            question_height: 0,
            question_rows: 0,
            config,
            keybinds: PopupKeybinds::dialog(),
            own_keybinds: ConfirmPopupKeybinds::new(),
        }
    }

    /// The question, wrapped to whatever width it is given. A question
    /// that lays its lines out, as command output does, keeps whatever
    /// indentation they have.
    fn paragraph(&self) -> Paragraph<'static> {
        Paragraph::new(self.question.clone()).wrap(Wrap { trim: false })
    }

    /// Where to put the popup in `area`: centered, and no larger than the
    /// question needs or the screen holds. A question that outgrows the
    /// screen is scrolled through.
    fn popup_rect(&self, area: Rect) -> Rect {
        let width = self
            .question
            .lines
            .iter()
            .map(|line| line.width() as u16 + PADDING * 2)
            .max()
            .unwrap_or(0)
            .max(MIN_WIDTH)
            .min(area.width);
        let rows = self
            .paragraph()
            .line_count(width.saturating_sub(PADDING * 2)) as u16;
        let height = (rows + PADDING * 2 + BUTTONS_HEIGHT).min(area.height);

        centered_rect_fixed(area, width, height)
    }

    fn max_scroll(&self) -> usize {
        self.question_rows
            .saturating_sub(self.question_height as usize)
    }

    fn scroll(&mut self, delta: isize) {
        let max = self.max_scroll() as isize;
        self.scroll = (self.scroll as isize + delta).clamp(0, max) as usize;
    }

    fn close() -> Result<ComponentInputResult> {
        Ok(ComponentInputResult::HandledAction(AppAction::ClosePopup))
    }

    /// Take the popup down, raising what a yes was to ask for.
    fn answer(&mut self, yes: bool) -> Result<ComponentInputResult> {
        let Some(confirmed) = self.confirmed.take().filter(|_| yes) else {
            return Self::close();
        };

        Ok(ComponentInputResult::HandledAction(AppAction::Multiple(
            vec![AppAction::ClosePopup, confirmed],
        )))
    }

    /// What a button label takes with the padding either side of it.
    fn button_width(label: &str) -> u16 {
        label.chars().count() as u16 + PADDING * 2
    }

    /// A button label, marking the key that presses it.
    fn label(&self, answer: &str, yes: bool) -> String {
        mark_key(
            answer,
            self.own_keybinds.shortcut(ConfirmPopupEvent::Answer(yes)),
        )
    }

    /// A button, marked when Enter is what presses it.
    fn button(&self, label: String, selected: bool) -> Paragraph<'static> {
        let style = if selected {
            Style::default()
                .bg(self.config.highlight_color())
                .underlined()
        } else {
            Style::default()
        };

        Paragraph::new(Span::styled(label, style))
    }

    /// Say which way there is more of the question to see, in the blank
    /// rows the padding leaves either side of it.
    fn draw_scroll_indicators(&self, f: &mut Frame<'_>, area: Rect) {
        let style = Style::default().fg(Color::DarkGray);
        let arrow = |f: &mut Frame<'_>, y: u16, arrow: &'static str| {
            f.render_widget(
                Paragraph::new(Line::from(arrow).centered()).style(style),
                Rect {
                    y,
                    height: 1,
                    ..area
                },
            );
        };

        if self.scroll > 0 {
            arrow(f, area.y + 1, "▲");
        }
        if self.scroll < self.max_scroll() {
            arrow(f, area.y + area.height.saturating_sub(2), "▼");
        }
    }

    fn draw_buttons(&self, f: &mut Frame<'_>, area: Rect) {
        let yes = self.label(YES, true);
        let no = self.label(NO, false);
        let yes_width = Self::button_width(&yes);
        let no_width = Self::button_width(&no);
        let margin = area.width.saturating_sub(yes_width + no_width) / 2;

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(margin),
                Constraint::Max(yes_width),
                Constraint::Max(no_width),
                Constraint::Length(margin),
            ])
            .split(area);

        f.render_widget(self.button(yes, self.yes_selected), chunks[1]);
        f.render_widget(self.button(no, !self.yes_selected), chunks[2]);
    }
}

impl Component for ConfirmPopup {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let area = self.popup_rect(area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Max(BUTTONS_HEIGHT)])
            .split(area);
        self.question_height = chunks[0].height.saturating_sub(PADDING * 2);
        self.question_rows = self
            .paragraph()
            .line_count(chunks[0].width.saturating_sub(PADDING * 2));
        // The screen may have shrunk since the last draw, leaving the
        // scroll past the end of the question.
        self.scroll(0);

        f.render_widget(Clear, area);
        // The question is padded away from the border the block draws
        // over it afterwards.
        f.render_widget(
            self.paragraph()
                .block(Block::new().padding(Padding::uniform(PADDING)))
                .scroll((self.scroll as u16, 0)),
            chunks[0],
        );
        self.draw_scroll_indicators(f, chunks[0]);
        f.render_widget(
            Block::bordered()
                .title(Span::styled(
                    format!(" {} ", self.title),
                    Style::new().bold().cyan(),
                ))
                .title_alignment(Alignment::Center)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
            area,
        );
        self.draw_buttons(f, chunks[1]);

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        let key = match event {
            Event::Key(key) => key,
            Event::Mouse(mouse) => {
                match mouse.kind {
                    MouseEventKind::ScrollDown => self.scroll(3),
                    MouseEventKind::ScrollUp => self.scroll(-3),
                    _ => {}
                }
                return Ok(ComponentInputResult::Handled);
            }
            _ => return Ok(ComponentInputResult::Handled),
        };
        if key.kind != KeyEventKind::Press {
            return Ok(ComponentInputResult::Handled);
        }

        let page = self.question_height as isize;
        match self.keybinds.match_event(key) {
            PopupEvent::Accept => return self.answer(self.yes_selected),
            PopupEvent::Cancel => return Self::close(),
            PopupEvent::ScrollDown => self.scroll(1),
            PopupEvent::ScrollUp => self.scroll(-1),
            PopupEvent::ScrollDownHalf => self.scroll(page / 2),
            PopupEvent::ScrollUpHalf => self.scroll(-page / 2),
            PopupEvent::ScrollDownPage => self.scroll(page),
            PopupEvent::ScrollUpPage => self.scroll(-page),
            PopupEvent::Unbound => {}
        }

        // The answers are the buttons the question puts up, so they are
        // its own keys rather than any popup's.
        match self.own_keybinds.match_event(key) {
            ConfirmPopupEvent::Answer(yes) => return self.answer(yes),
            ConfirmPopupEvent::Select(yes) => self.yes_selected = yes,
            ConfirmPopupEvent::Unbound => {}
        }

        Ok(ComponentInputResult::Handled)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::KeyCode;
    use ratatui::crossterm::event::KeyEvent;
    use ratatui::text::Line;

    use super::*;
    use crate::commander::ids::ChangeId;
    use crate::commander::ids::CommitId;
    use crate::commander::log::Head;

    /// The buttons as the default bindings mark them
    const MARKED_YES: &str = "(Y)es";
    const MARKED_NO: &str = "(N)o";

    /// A question raising a recognisable action when answered yes.
    fn popup() -> ConfirmPopup {
        ConfirmPopup::new(
            JjConfig::default(),
            "Abandon",
            Text::from(vec![
                Line::from("Are you sure you want to abandon this change?"),
                Line::from("Change: abcdefgh"),
            ]),
            AppAction::ViewLog(Head {
                change_id: ChangeId("confirmed".into()),
                commit_id: CommitId("confirmed".into()),
                divergent: false,
                immutable: false,
            }),
        )
    }

    /// A question of `lines` numbered lines, which is more than a screen
    /// of them.
    fn long_popup(lines: usize) -> ConfirmPopup {
        ConfirmPopup::new(
            JjConfig::default(),
            "Push",
            Text::from(
                (0..lines)
                    .map(|line| Line::from(format!("line {line}")))
                    .collect::<Vec<_>>(),
            ),
            AppAction::ClosePopup,
        )
    }

    fn press(popup: &mut ConfirmPopup, key: KeyCode) -> ComponentInputResult {
        popup
            .input(Event::Key(KeyEvent::from(key)))
            .expect("the popup handles key presses")
    }

    /// Whether the result closes the popup, and what it asks for besides.
    fn closed_with(result: ComponentInputResult) -> Option<Option<String>> {
        match result {
            ComponentInputResult::HandledAction(AppAction::ClosePopup) => Some(None),
            ComponentInputResult::HandledAction(AppAction::Multiple(actions)) => {
                let [AppAction::ClosePopup, AppAction::ViewLog(head)] = actions.as_slice() else {
                    return None;
                };
                Some(Some(head.change_id.0.clone()))
            }
            _ => None,
        }
    }

    /// What the popup puts on a 60x16 screen
    fn render(popup: &mut ConfirmPopup) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(60, 16)).expect("the test backend");
        terminal
            .draw(|f| popup.draw(f, f.area()).expect("the popup draws"))
            .expect("the frame is drawn");

        terminal.backend().buffer().clone()
    }

    fn rows(buffer: &Buffer) -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    /// The rows the popup itself takes up, from its top border to its
    /// bottom one.
    fn dialog_rows(buffer: &Buffer) -> Vec<String> {
        rows(buffer)
            .into_iter()
            .filter(|row| row.contains('│') || row.contains('╭') || row.contains('╰'))
            .collect()
    }

    /// Whether the cell the label starts in is the highlighted one.
    fn is_selected(buffer: &Buffer, label: &str) -> bool {
        let rows = rows(buffer);
        let (row, line) = rows
            .iter()
            .enumerate()
            .find(|(_, line)| line.contains(label))
            .expect("the label is on screen");
        // The rows hold box-drawing characters, so the column the label
        // starts in is a count of characters rather than of bytes.
        let column = line[..line.find(label).expect("the label is on the line")]
            .chars()
            .count();

        buffer[(column as u16, row as u16)].style().bg
            == Some(JjConfig::default().highlight_color())
    }

    #[test]
    fn the_question_sits_under_the_title_and_over_the_buttons() {
        let mut popup = popup();

        let rows = dialog_rows(&render(&mut popup));

        assert!(rows[0].contains("Abandon"), "{rows:?}");
        assert!(
            rows.iter()
                .any(|row| row.contains("Are you sure you want to abandon this change?")),
            "{rows:?}"
        );
        let buttons = rows.len() - 2;
        assert!(rows[buttons].contains(MARKED_YES), "{rows:?}");
        assert!(rows[buttons].contains(MARKED_NO), "{rows:?}");
    }

    #[test]
    fn a_question_longer_than_the_screen_stops_at_it_and_scrolls() {
        let mut popup = long_popup(40);

        let rows = dialog_rows(&render(&mut popup));

        // The whole screen, and the first line of the question under the
        // top border and the blank row below it
        assert_eq!(rows.len(), 16, "{rows:?}");
        assert!(rows[2].contains("line 0"), "{rows:?}");
        assert!(rows.iter().any(|row| row.contains('▼')), "{rows:?}");

        for _ in 0..3 {
            press(&mut popup, KeyCode::Char('j'));
        }
        let rows = dialog_rows(&render(&mut popup));

        assert!(rows[2].contains("line 3"), "{rows:?}");
        assert!(rows[1].contains('▲'), "{rows:?}");
    }

    #[test]
    fn scrolling_stops_at_the_end_of_the_question() {
        let mut popup = long_popup(40);
        // The popup only knows how much of the question it shows once it
        // has been drawn.
        render(&mut popup);

        for _ in 0..100 {
            press(&mut popup, KeyCode::Char('j'));
        }
        let rows = dialog_rows(&render(&mut popup));

        // The last of the ten rows the question is shown in
        assert!(rows[11].contains("line 39"), "{rows:?}");
        assert!(!rows.iter().any(|row| row.contains('▼')), "{rows:?}");
    }

    #[test]
    fn yes_is_the_button_enter_presses_until_the_other_is_picked() {
        let mut popup = popup();

        assert!(is_selected(&render(&mut popup), MARKED_YES));
        assert!(!is_selected(&render(&mut popup), MARKED_NO));

        press(&mut popup, KeyCode::Right);

        assert!(is_selected(&render(&mut popup), MARKED_NO));
        assert!(!is_selected(&render(&mut popup), MARKED_YES));

        press(&mut popup, KeyCode::Left);

        assert!(is_selected(&render(&mut popup), MARKED_YES));
    }

    #[test]
    fn answering_yes_raises_what_the_question_was_asked_for() {
        let mut popup = popup();

        press(&mut popup, KeyCode::Left);

        assert_eq!(
            closed_with(press(&mut popup, KeyCode::Enter)),
            Some(Some("confirmed".into()))
        );
    }

    #[test]
    fn answering_no_only_takes_the_question_down() {
        let mut popup = popup();

        press(&mut popup, KeyCode::Right);

        assert_eq!(closed_with(press(&mut popup, KeyCode::Enter)), Some(None));
    }

    #[test]
    fn a_button_can_be_pressed_by_its_letter_whichever_is_picked() {
        assert_eq!(
            closed_with(press(&mut popup(), KeyCode::Char('y'))),
            Some(Some("confirmed".into()))
        );
        assert_eq!(
            closed_with(press(&mut popup(), KeyCode::Char('n'))),
            Some(None)
        );
    }

    #[test]
    fn the_question_stays_up_until_it_is_answered() {
        let mut popup = popup();

        assert!(matches!(
            press(&mut popup, KeyCode::Right),
            ComponentInputResult::Handled
        ));
    }

    #[test]
    fn quitting_takes_the_question_down_unanswered() {
        assert_eq!(
            closed_with(press(&mut popup(), KeyCode::Char('q'))),
            Some(None)
        );
        assert_eq!(closed_with(press(&mut popup(), KeyCode::Esc)), Some(None));
    }
}
