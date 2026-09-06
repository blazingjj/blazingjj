use std::borrow::Cow;

use ansi_to_tui::IntoText;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
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
    let paragraph = Paragraph::new(answer.into_text().unwrap())
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
    fn a_refusal_is_as_tall_as_it_takes_at_the_width_it_gets() {
        // Three lines of ten, plus the row the border is on.
        let (_, height) = refusal("aaaa bbbb cccc dddd eeee ffff", 10);

        assert_eq!(height, 4);
    }
}
