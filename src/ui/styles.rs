use std::sync::LazyLock;

use ansi_to_tui::IntoText;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Padding;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;

pub static POPUP_BLOCK: LazyLock<Block<'static>> = LazyLock::new(|| {
    Block::<'static>::bordered()
        .padding(Padding::horizontal(1))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Green))
});
pub static POPUP_BLOCK_TITLE_STYLE: LazyLock<Style> = LazyLock::new(|| Style::new().bold().cyan());

/// What a popup puts under the field it asks in when what was typed was
/// turned down: the answer, boxed off from the field and wrapped to
/// `width`, and the rows it takes there.
pub fn refusal(answer: &str, width: u16) -> (Paragraph<'static>, u16) {
    let paragraph = Paragraph::new(answer.into_text().unwrap())
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    let height = paragraph.line_count(width) as u16;

    (paragraph, height)
}

pub fn create_popup_block(title: &str) -> Block<'_> {
    POPUP_BLOCK
        .clone()
        .title(Span::styled(format!(" {title} "), *POPUP_BLOCK_TITLE_STYLE))
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
            .border_style(Style::default().fg(Color::DarkGray)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_as_tall_as_it_takes_at_the_width_it_gets() {
        // Three lines of ten, plus the row the border is on.
        let (_, height) = refusal("aaaa bbbb cccc dddd eeee ffff", 10);

        assert_eq!(height, 4);
    }
}
