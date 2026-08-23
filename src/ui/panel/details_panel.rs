/*!
The details_panel module contains the main class [DetailsPanel] which
can show various content with an automatic scroll bar.

There is no content in the DetailsPanel, that is provided every frame
as a struct that implements trait [DetailContent]
and rendered using DetailsPanel.render_context.

To make this efficient there are two implementations of DetailContent.
* [TextContent] - for small texts rendered as a Ratatui Paragraph.
* [LargeStringContent] - for large texts where only the visible part is rendered.

*/

use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEvent;
use ratatui::crossterm::event::KeyModifiers;
use ratatui::crossterm::event::MouseEvent;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Margin;
use ratatui::layout::Position;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::text::Text;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Padding;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Scrollbar;
use ratatui::widgets::ScrollbarOrientation;
use ratatui::widgets::ScrollbarState;
use ratatui::widgets::Wrap;
use tracing::trace;

use crate::ui::utils::LargeString;

/// Details panel used for the right side of each tab.
/// This handles scrolling and wrapping.
pub struct DetailsPanel {
    /// Area for rendering panel, including borders
    panel_rect: Rect,
    /// Area used for rendering content of panel
    content_rect: Rect,
    /// First line of content that is visible
    scroll: u16,
    /// Total number of lines in content, including extra lines for wrapped lines.
    lines: u16,
    /// Wrap long lines of content into multiple lines
    wrap: bool,
}

/// Content of the detail panel must be able to render as a paragraph
pub trait DetailContent<'a> {
    /// Render content as a paragraph, and update panel total lines
    fn render_as_paragraph(&self, panel: &mut DetailsPanel, area: Rect) -> Paragraph<'_>;
}

/// Content is preformatted ratatui Text
pub struct TextContent<'a> {
    text: Text<'a>,
}

/// Content is a large string that can quickly fetch a range of lines
pub struct LargeStringContent<'a> {
    large_string: &'a LargeString,
}

/// Transient object holding render data
pub struct DetailsPanelRenderContext<'a, Content>
where
    Content: DetailContent<'a>,
{
    panel: &'a mut DetailsPanel,
    title: Option<Line<'a>>,
    content: Content,
}

/// Commands that can be handled by the details panel
pub enum DetailsPanelEvent {
    ScrollDown,
    ScrollUp,
    ScrollDownHalfPage,
    ScrollUpHalfPage,
    ScrollDownPage,
    ScrollUpPage,
    ToggleWrap,
}

//
//  implementation
//

impl<'a> From<&'a LargeString> for LargeStringContent<'a> {
    fn from(large_string: &'a LargeString) -> Self {
        Self { large_string }
    }
}

//impl<'a> From<Text<'a>> for TextContent<'a> {
impl<'a, T: Into<Text<'a>>> From<T> for TextContent<'a> {
    fn from(content: T) -> Self {
        let text = content.into();
        Self { text }
    }
}

impl<'a> DetailContent<'a> for LargeStringContent<'a> {
    fn render_as_paragraph(&self, panel: &mut DetailsPanel, area: Rect) -> Paragraph<'_> {
        panel.content_rect = area;
        // Update total length. This is used by the scroll bar
        panel.lines = self.large_string.lines() as u16;
        panel.clamp_scroll();
        // Extract visible part of content
        let top_line = panel.scroll as usize;
        let line_count = area.height as usize;
        let content_text = self.large_string.render(top_line, line_count);
        Paragraph::new(content_text)
    }
}

impl<'a> DetailContent<'a> for TextContent<'a> {
    fn render_as_paragraph(&self, panel: &mut DetailsPanel, area: Rect) -> Paragraph<'_> {
        let content_text = &self.text;
        let mut paragraph = Paragraph::new(content_text.clone());

        panel.content_rect = area;
        panel.lines = paragraph.line_count(area.width) as u16;
        panel.clamp_scroll();

        paragraph = paragraph.scroll((panel.scroll, 0));

        paragraph
    }
}

impl<'a, Content> DetailsPanelRenderContext<'a, Content>
where
    Content: DetailContent<'a>,
{
    pub fn new(panel: &'a mut DetailsPanel, content: Content) -> Self {
        Self {
            panel,
            title: None,
            content,
        }
    }
    /// Set the title on the frame that surrounds the content
    pub fn title<T>(&mut self, title: T) -> &mut Self
    where
        T: Into<Line<'a>>,
    {
        self.title = Some(title.into());
        self
    }

    pub fn draw(&mut self, f: &mut ratatui::prelude::Frame<'_>, area: ratatui::prelude::Rect) {
        // Remember last rendered rect for mouse event handling
        self.panel.panel_rect = area;

        // Define border block
        let mut border = Block::bordered()
            .border_type(BorderType::Rounded)
            .padding(Padding::horizontal(1));
        // Apply title if provided
        if let Some(title) = &self.title {
            border = border.title_top(title.clone());
        }

        // Create content widget that uses border
        let paragraph_area = border.inner(area);
        let content = &self.content;
        let mut paragraph = content
            .render_as_paragraph(self.panel, paragraph_area)
            .block(border);

        if self.panel.wrap {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }

        // render content and border
        f.render_widget(paragraph, area);

        // render scrollbar on top of border
        if self.panel.lines > paragraph_area.height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);

            let mut scrollbar_state =
                ScrollbarState::new(self.panel.lines.into()).position(self.panel.scroll.into());

            f.render_stateful_widget(
                scrollbar,
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }
    }
}

impl DetailsPanel {
    pub fn new() -> Self {
        Self {
            panel_rect: Rect::ZERO,
            content_rect: Rect::ZERO,
            scroll: 0,
            lines: 0,
            wrap: true,
        }
    }

    /// Create a RenderContext that can render the provided content
    /// as a Paragraph into an area.
    pub fn render_context<'a, Content>(
        &'a mut self,
        content: impl Into<Content>,
    ) -> DetailsPanelRenderContext<'a, Content>
    where
        Content: DetailContent<'a>,
    {
        DetailsPanelRenderContext::new(self, content.into())
    }

    /// Return number of columns available for content at last call to render.
    /// Will return 0 if render has not been called.
    pub fn columns(&self) -> u16 {
        self.content_rect.width
    }

    /// Return number of rows available for content at last call to render.
    /// Will return 0 if render has not been called.
    pub fn rows(&self) -> u16 {
        self.content_rect.height
    }

    /// Scroll to a line, at most until the end of the content reaches the
    /// bottom of the panel
    pub fn scroll_to(&mut self, line_no: u16) {
        // Wrapped content takes more rows than it has lines, and we do not
        // know how many, so we can only keep its last line in view
        let keep_visible = if self.wrap { 1 } else { self.rows() };
        self.scroll = line_no.min(self.lines.saturating_sub(keep_visible))
    }

    /// Pull the scroll position back into content that has shrunk
    fn clamp_scroll(&mut self) {
        self.scroll_to(self.scroll);
    }

    pub fn scroll(&mut self, scroll: isize) {
        self.scroll_to(self.scroll.saturating_add_signed(scroll as i16))
    }

    pub fn handle_event(&mut self, details_panel_event: DetailsPanelEvent) {
        match details_panel_event {
            DetailsPanelEvent::ScrollDown => self.scroll(1),
            DetailsPanelEvent::ScrollUp => self.scroll(-1),
            DetailsPanelEvent::ScrollDownHalfPage => self.scroll(self.rows() as isize / 2),
            DetailsPanelEvent::ScrollUpHalfPage => {
                self.scroll((self.rows() as isize / 2).saturating_neg())
            }
            DetailsPanelEvent::ScrollDownPage => self.scroll(self.rows() as isize),
            DetailsPanelEvent::ScrollUpPage => self.scroll((self.rows() as isize).saturating_neg()),
            DetailsPanelEvent::ToggleWrap => self.wrap = !self.wrap,
        }
    }

    /// Handle input. Returns bool of if event was handled
    pub fn input(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_event(DetailsPanelEvent::ScrollDown)
            }
            KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_event(DetailsPanelEvent::ScrollUp)
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_event(DetailsPanelEvent::ScrollDownHalfPage)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_event(DetailsPanelEvent::ScrollUpHalfPage)
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_event(DetailsPanelEvent::ScrollDownPage)
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.handle_event(DetailsPanelEvent::ScrollUpPage)
            }
            KeyCode::Char('W') => self.handle_event(DetailsPanelEvent::ToggleWrap),
            _ => return false,
        };

        true
    }

    /// Handle input. Returns bool of if event was handled
    pub fn input_mouse(&mut self, mouse: MouseEvent) -> bool {
        if !self.panel_rect.contains(Position {
            y: mouse.row,
            x: mouse.column,
        }) {
            trace!("mouse {:?} not in rect {:?}", &mouse, &self.panel_rect);
            return false;
        }
        trace!("mouse {:?} inside  rect {:?}", &mouse, &self.panel_rect);
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.handle_event(DetailsPanelEvent::ScrollUp);
                self.handle_event(DetailsPanelEvent::ScrollUp);
                self.handle_event(DetailsPanelEvent::ScrollUp);
            }
            MouseEventKind::ScrollDown => {
                self.handle_event(DetailsPanelEvent::ScrollDown);
                self.handle_event(DetailsPanelEvent::ScrollDown);
                self.handle_event(DetailsPanelEvent::ScrollDown);
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 10,
    };

    /// A panel that has rendered `lines` of unwrapped content into [AREA]
    fn panel_showing(lines: u16) -> DetailsPanel {
        let mut panel = DetailsPanel::new();
        panel.content_rect = AREA;
        panel.lines = lines;
        panel.wrap = false;
        panel
    }

    fn render(panel: &mut DetailsPanel, content: &LargeString) {
        LargeStringContent::from(content).render_as_paragraph(panel, AREA);
    }

    #[test]
    fn scrolling_stops_with_the_last_line_at_the_bottom() {
        let mut panel = panel_showing(100);

        panel.scroll_to(u16::MAX);

        assert_eq!(panel.scroll, 90);
    }

    #[test]
    fn wrapped_content_scrolls_up_to_its_last_line() {
        let mut panel = panel_showing(100);
        panel.wrap = true;

        panel.scroll_to(u16::MAX);

        assert_eq!(panel.scroll, 99);
    }

    #[test]
    fn content_shorter_than_the_panel_is_not_scrolled() {
        let mut panel = panel_showing(3);

        panel.scroll_to(u16::MAX);

        assert_eq!(panel.scroll, 0);
    }

    #[test]
    fn shrinking_content_pulls_the_scroll_position_back() {
        let mut panel = panel_showing(100);
        panel.scroll_to(90);

        render(&mut panel, &LargeString::new("a\nb\nc\n".to_owned()));

        assert_eq!(panel.scroll, 0);
    }

    #[test]
    fn growing_content_keeps_the_scroll_position() {
        let mut panel = panel_showing(20);
        panel.scroll_to(10);

        let lines = "a\n".repeat(100);
        render(&mut panel, &LargeString::new(lines));

        assert_eq!(panel.scroll, 10);
    }
}
