/*! A yes/no question, put with the action to take when the answer is
yes.
*/

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Clear;
use ratatui::widgets::Padding;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;

use crate::env::JjConfig;
use crate::keybinds::PopupEvent;
use crate::keybinds::PopupKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::utils::centered_rect_fixed;

const YES: &str = "(Y)es";
const YES_KEY: char = 'y';
const NO: &str = "(N)o";
const NO_KEY: char = 'n';

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
    config: JjConfig,
    keybinds: PopupKeybinds,
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
            config,
            keybinds: PopupKeybinds::dialog(),
        }
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
        label.len() as u16 + PADDING * 2
    }

    /// A button label, marked when Enter is what presses it.
    fn button(&self, label: &'static str, selected: bool) -> Paragraph<'static> {
        let style = if selected {
            Style::default()
                .bg(self.config.highlight_color())
                .underlined()
        } else {
            Style::default()
        };

        Paragraph::new(Span::styled(label, style))
    }

    fn draw_buttons(&self, f: &mut Frame<'_>, area: Rect) {
        let yes_width = Self::button_width(YES);
        let no_width = Self::button_width(NO);
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

        f.render_widget(self.button(YES, self.yes_selected), chunks[1]);
        f.render_widget(self.button(NO, !self.yes_selected), chunks[2]);
    }
}

impl Component for ConfirmPopup {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let width = self
            .question
            .lines
            .iter()
            .map(|line| line.width() as u16 + PADDING * 2)
            .max()
            .unwrap_or(0)
            .max(MIN_WIDTH);
        let height = self.question.lines.len() as u16 + PADDING * 2 + BUTTONS_HEIGHT;
        let area = centered_rect_fixed(area, width, height);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Max(BUTTONS_HEIGHT)])
            .split(area);

        f.render_widget(Clear, area);
        // The question is padded away from the border the block draws
        // over it afterwards.
        f.render_widget(
            Paragraph::new(self.question.clone())
                .block(Block::new().padding(Padding::uniform(PADDING)))
                .wrap(Wrap { trim: true }),
            chunks[0],
        );
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
        let Event::Key(key) = event else {
            return Ok(ComponentInputResult::Handled);
        };
        if key.kind != KeyEventKind::Press {
            return Ok(ComponentInputResult::Handled);
        }

        match self.keybinds.match_event(key) {
            PopupEvent::Accept => return self.answer(self.yes_selected),
            PopupEvent::Cancel => return Self::close(),
            _ => {}
        }

        // The answers are the buttons the question puts up, so they are
        // its own keys rather than any popup's.
        match key.code {
            KeyCode::Left => self.yes_selected = true,
            KeyCode::Right => self.yes_selected = false,
            KeyCode::Char(YES_KEY) => return self.answer(true),
            KeyCode::Char(NO_KEY) => return self.answer(false),
            _ => {}
        }

        Ok(ComponentInputResult::Handled)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::KeyEvent;
    use ratatui::text::Line;

    use super::*;
    use crate::commander::ids::ChangeId;
    use crate::commander::ids::CommitId;
    use crate::commander::log::Head;

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
        assert!(rows[buttons].contains(YES), "{rows:?}");
        assert!(rows[buttons].contains(NO), "{rows:?}");
    }

    #[test]
    fn yes_is_the_button_enter_presses_until_the_other_is_picked() {
        let mut popup = popup();

        assert!(is_selected(&render(&mut popup), YES));
        assert!(!is_selected(&render(&mut popup), NO));

        press(&mut popup, KeyCode::Right);

        assert!(is_selected(&render(&mut popup), NO));
        assert!(!is_selected(&render(&mut popup), YES));

        press(&mut popup, KeyCode::Left);

        assert!(is_selected(&render(&mut popup), YES));
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
            closed_with(press(&mut popup(), KeyCode::Char(YES_KEY))),
            Some(Some("confirmed".into()))
        );
        assert_eq!(
            closed_with(press(&mut popup(), KeyCode::Char(NO_KEY))),
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
