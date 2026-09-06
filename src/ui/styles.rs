use std::borrow::Cow;

use ansi_to_tui::IntoText;
use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Padding;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;

use crate::theme::Role;

/// The title a panel of the frame is drawn under, as its block takes
/// one.
pub fn panel_title(title: impl Into<Cow<'static, str>>) -> Span<'static> {
    Span::styled(title, Role::PanelTitle.style())
}

/// The heading a list is divided under, which is underlined rather than
/// indented like the rows beneath it.
pub fn section_heading(heading: impl Into<Cow<'static, str>>) -> Span<'static> {
    Span::styled(heading, Role::Heading.style())
        .bold()
        .underlined()
}

/// The block a panel of the frame is drawn in, which a caller titles
/// with [panel_title] and pads as it needs.
pub fn panel_block() -> Block<'static> {
    Block::bordered()
        .border_style(Role::Border.style())
        .border_type(BorderType::Rounded)
}

/// `line` with `style` patched onto it and onto every span of it.
///
/// A span carries the background of whatever role drew it, so patching
/// only the line would leave the spans painted over: what reaches the
/// cells between them would not reach the text. Patching leaves a
/// channel `style` says nothing about alone, which is what lets the
/// highlight take a row without flattening the colours on it.
pub fn patched(mut line: Line<'_>, style: Style) -> Line<'_> {
    line.style = line.style.patch(style);
    for span in &mut line.spans {
        span.style = span.style.patch(style);
    }

    line
}

/// Paint `area` in the colours the app is drawn in.
///
/// A widget only colours the cells it draws something in, so without
/// this the background would show only behind the text and the
/// terminal's own would show everywhere else.
pub fn paint(f: &mut Frame<'_>, area: Rect) {
    let style = Role::Default.style();
    if style != Style::new() {
        f.buffer_mut().set_style(area, style);
    }
}

/// Reading the ANSI a program wrote as the colours it means.
///
/// Nothing reads it any other way: [ansi_to_tui] on its own takes a
/// program at its word about the terminal's default colours, which the
/// app would then draw over its own background.
pub trait AnsiText {
    /// The text, owning what it is made of.
    fn owned_ansi_text(&self) -> Result<Text<'static>, ansi_to_tui::Error>;

    /// The text, borrowing from what it was read out of.
    fn to_ansi_text(&self) -> Result<Text<'_>, ansi_to_tui::Error>;
}

impl<T: AsRef<[u8]>> AnsiText for T {
    fn owned_ansi_text(&self) -> Result<Text<'static>, ansi_to_tui::Error> {
        Ok(without_terminal_defaults(IntoText::into_text(self)?))
    }

    fn to_ansi_text(&self) -> Result<Text<'_>, ansi_to_tui::Error> {
        Ok(without_terminal_defaults(IntoText::to_text(self)?))
    }
}

/// `text` with every colour that names the terminal's own left unsaid.
///
/// A program ends a colour by naming the terminal's default rather than
/// by saying nothing, and the ANSI reset jj ends a line with names both
/// of them. Those read as colours like any other, so drawn as they come
/// they would be painted over the background the app puts down, leaving
/// the terminal's showing through wherever a program stopped colouring.
/// Left unsaid instead, they fall through to what the app is drawn in.
fn without_terminal_defaults(mut text: Text<'_>) -> Text<'_> {
    fn said(style: Style) -> Style {
        Style {
            fg: style.fg.filter(|color| *color != Color::Reset),
            bg: style.bg.filter(|color| *color != Color::Reset),
            ..style
        }
    }

    text.style = said(text.style);
    for line in &mut text.lines {
        line.style = said(line.style);
        for span in &mut line.spans {
            span.style = said(span.style);
        }
    }

    text
}

/// Blank `area` out for a popup to be drawn over whatever was under it.
///
/// Clearing takes a cell back to the terminal's own colours rather than
/// to the app's, so the app's are painted back on: a popup sits on the
/// same background as the rest of it.
pub fn clear(f: &mut Frame<'_>, area: Rect) {
    f.render_widget(Clear, area);
    paint(f, area);
}

/// The block a popup is drawn in.
pub fn popup_block() -> Block<'static> {
    Block::<'static>::bordered()
        .padding(Padding::horizontal(1))
        .border_type(BorderType::Rounded)
        .border_style(Role::PopupBorder.style())
}

/// What the title of a popup is written in.
pub fn popup_block_title_style() -> Style {
    Role::PopupTitle.style().bold()
}

/// What a popup puts under the field it asks in when what was typed was
/// turned down: the answer, boxed off from the field and wrapped to
/// `width`, and the rows it takes there.
pub fn refusal(answer: &str, width: u16) -> (Paragraph<'static>, u16) {
    let paragraph = Paragraph::new(answer.owned_ansi_text().unwrap())
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_type(BorderType::Rounded)
                .border_style(Role::Separator.style()),
        );
    let height = paragraph.line_count(width) as u16;

    (paragraph, height)
}

pub fn create_popup_block(title: &str) -> Block<'_> {
    popup_block()
        .title(Span::styled(
            format!(" {title} "),
            popup_block_title_style(),
        ))
        .title_alignment(Alignment::Center)
}

/// How much of the width of the screen a popup asking for something
/// takes.
pub const POPUP_WIDTH_PERCENT: u16 = 60;

/// How much of that its border and its padding take.
const POPUP_CHROME_WIDTH: u16 = 4;

/// How wide the text of such a popup is, drawn in `area`.
pub fn popup_text_width(area: Rect) -> u16 {
    (area.width * POPUP_WIDTH_PERCENT / 100)
        .saturating_sub(POPUP_CHROME_WIDTH)
        .max(1)
}

/// How many rows `lines` take once wrapped into `width` columns.
pub fn wrapped_height(lines: &[Line], width: u16) -> u16 {
    lines
        .iter()
        .map(|line| (line.width() as u16).div_ceil(width).max(1))
        .sum()
}

/// What a popup says under a rule at the foot of it, such as what it
/// answers to or what it made of what it was given.
pub fn popup_footer(lines: Vec<Line<'static>>) -> Paragraph<'static> {
    Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::TOP)
            .border_type(BorderType::Rounded)
            .border_style(Role::Separator.style()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_program_resets_to_the_terminals_own_is_left_unsaid() {
        let text = "\x1b[38;5;5mpzzunysw\x1b[0m plain"
            .owned_ansi_text()
            .expect("the escapes read");

        let styles: Vec<Style> = text.lines[0].spans.iter().map(|span| span.style).collect();
        assert!(styles.iter().all(|style| style.bg.is_none()), "{styles:?}");
        // Only what the reset said is dropped; the colour before it is
        // what jj meant and stays.
        assert_eq!(styles[0].fg, Some(Color::Indexed(5)));
        assert!(styles[1..].iter().all(|style| style.fg.is_none()));
    }

    /// A colour of the terminal's palette is not the terminal's default,
    /// so it is left alone: `black` is a colour a program asked for.
    #[test]
    fn a_colour_of_the_palette_is_not_the_terminals_default() {
        let text = "\x1b[40mon black\x1b[49m"
            .owned_ansi_text()
            .expect("the escapes read");

        assert_eq!(text.lines[0].spans[0].style.bg, Some(Color::Black));
    }

    #[test]
    fn a_refusal_is_as_tall_as_it_takes_at_the_width_it_gets() {
        // Three lines of ten, plus the row the border is on.
        let (_, height) = refusal("aaaa bbbb cccc dddd eeee ffff", 10);

        assert_eq!(height, 4);
    }
}
