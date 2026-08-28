use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Alignment;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Clear;
use ratatui::widgets::Padding;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Scrollbar;
use ratatui::widgets::ScrollbarOrientation;
use ratatui::widgets::ScrollbarState;

use crate::env::get_env;
use crate::keybinds::MessagePopupEvent;
use crate::keybinds::MessagePopupKeybinds;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::utils::LargeString;
use crate::ui::utils::centered_rect;
use crate::ui::utils::centered_rect_fixed;
use crate::ui::utils::chrome;

pub struct MessagePopup<'a> {
    title: Line<'a>,
    messages: LargeString,
    text_align: Option<Alignment>,
    scroll: usize,
    lines: usize,
    content_height: u16,
    keybinds: Option<MessagePopupKeybinds>,
}

impl<'a> MessagePopup<'a> {
    pub fn new(title: impl Into<Line<'a>>, messages: impl Into<String>) -> Self {
        let messages = LargeString::new(messages.into());
        let lines = messages.lines();
        Self {
            title: title.into(),
            messages,
            text_align: None,
            scroll: 0,
            lines,
            content_height: 0,
            keybinds: None,
        }
    }

    pub fn text_align(mut self, align: Alignment) -> Self {
        self.text_align = Some(align);
        self
    }

    fn keybinds(&mut self) -> &MessagePopupKeybinds {
        self.keybinds.get_or_insert_with(|| {
            get_env()
                .jj_config
                .keybinds()
                .map(MessagePopupKeybinds::from_config)
                .unwrap_or_default()
        })
    }

    /// Where to put the popup in `area`: centered, and no larger than the
    /// message needs, up to the share of the screen we are willing to take.
    fn popup_rect(&self, area: Rect, block: &Block, title_width: u16) -> Rect {
        let max = centered_rect(area, 80, 80);
        let [extra_width, extra_height] = chrome(block, max);

        // The two rows the scroll indicators live in are there whether or
        // not they are, and the title has to fit on the top border.
        let width = (self.messages.width() as u16 + extra_width)
            .max(title_width + 2)
            .min(max.width);
        let height = (self.lines as u16 + 2 + extra_height).min(max.height);

        centered_rect_fixed(area, width, height)
    }

    fn max_scroll(&self) -> usize {
        self.lines.saturating_sub(self.content_height as usize)
    }

    fn do_scroll(&mut self, delta: isize) {
        let max = self.max_scroll() as isize;
        self.scroll = (self.scroll as isize + delta).clamp(0, max) as usize;
    }
}

impl Component for MessagePopup<'_> {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let mut title = self.title.clone();
        title.spans = [vec![Span::raw(" ")], title.spans, vec![Span::raw(" ")]].concat();
        title = title.fg(Color::Cyan).bold();

        let block = Block::bordered()
            .title(title.clone())
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green))
            .padding(Padding::horizontal(1));

        let popup_rect = self.popup_rect(area, &block, title.width() as u16);
        f.render_widget(Clear, popup_rect);

        let inner = block.inner(popup_rect);
        let content_rect = inner.inner(Margin {
            vertical: 1,
            horizontal: 0,
        });
        self.content_height = content_rect.height;

        let line_count = content_rect.height as usize;
        let text = self.messages.render(self.scroll, line_count);

        let paragraph =
            Paragraph::new(text).alignment(self.text_align.unwrap_or(Alignment::Center));

        f.render_widget(block, popup_rect);
        f.render_widget(paragraph, content_rect);

        let max_scroll = self.max_scroll();
        let indicator_style = Style::default().fg(Color::DarkGray);
        if self.scroll > 0 {
            let top_gap = Rect {
                y: inner.y,
                height: 1,
                ..content_rect
            };
            f.render_widget(
                Paragraph::new(Line::from("▲").centered()).style(indicator_style),
                top_gap,
            );
        }
        if self.scroll < max_scroll {
            let bottom_gap = Rect {
                y: content_rect.y + content_rect.height,
                height: 1,
                ..content_rect
            };
            f.render_widget(
                Paragraph::new(Line::from("▼").centered()).style(indicator_style),
                bottom_gap,
            );
        }

        if max_scroll > 0 {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            let mut scrollbar_state = ScrollbarState::new(max_scroll + 1).position(self.scroll);
            f.render_stateful_widget(
                scrollbar,
                Rect {
                    y: inner.y,
                    height: inner.height,
                    ..popup_rect
                },
                &mut scrollbar_state,
            );
        }

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        let half_page = self.content_height as isize / 2;
        let full_page = self.content_height as isize;
        match &event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let delta = match self.keybinds().match_event(*key) {
                    MessagePopupEvent::ScrollDown => 1,
                    MessagePopupEvent::ScrollUp => -1,
                    MessagePopupEvent::ScrollDownHalf => half_page,
                    MessagePopupEvent::ScrollUpHalf => -half_page,
                    MessagePopupEvent::ScrollDownPage => full_page,
                    MessagePopupEvent::ScrollUpPage => -full_page,
                    MessagePopupEvent::Unbound => return Ok(ComponentInputResult::NotHandled),
                };
                self.do_scroll(delta);
                Ok(ComponentInputResult::Handled)
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => {
                    self.do_scroll(3);
                    Ok(ComponentInputResult::Handled)
                }
                MouseEventKind::ScrollUp => {
                    self.do_scroll(-3);
                    Ok(ComponentInputResult::Handled)
                }
                _ => Ok(ComponentInputResult::NotHandled),
            },
            _ => Ok(ComponentInputResult::NotHandled),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn popup_rect(title: &'static str, message: &str, area: Rect) -> Rect {
        let popup = MessagePopup::new(title, message);
        let block = Block::bordered().padding(Padding::horizontal(1));
        let title_width = title.chars().count() as u16 + 2;

        popup.popup_rect(area, &block, title_width)
    }

    #[test]
    fn a_short_message_gets_a_popup_its_own_size() {
        let rect = popup_rect("New", "one\ntwo", Rect::new(0, 0, 100, 40));

        // The two lines plus the scroll indicator gaps and the border
        assert_eq!(rect.height, 6);
        // The widest line plus the padding and the border
        assert_eq!(rect.width, 7);
    }

    #[test]
    fn the_title_holds_the_popup_open() {
        let rect = popup_rect("A rather long title", "hi", Rect::new(0, 0, 100, 40));

        assert_eq!(rect.width, 23);
    }

    #[test]
    fn a_long_message_stops_at_the_share_of_the_screen_we_take() {
        let long = "x".repeat(200);
        let rect = popup_rect("New", &vec![long; 100].join("\n"), Rect::new(0, 0, 100, 40));

        assert_eq!(rect.width, 80);
        assert_eq!(rect.height, 32);
    }
}
