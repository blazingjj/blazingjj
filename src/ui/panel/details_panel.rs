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

use std::time::Duration;
use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::MouseButton;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Margin;
use ratatui::layout::Position;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
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

use super::MouseInput;
use super::PanelMouseInput;
use crate::event::CLICK_PAUSE;
use crate::event::Mouse;
use crate::keybinds::DetailsPanelEvent;
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
    /// Total number of lines in content, however many rows they take to
    /// show.
    lines: u16,
    /// Wrap long lines of content into multiple lines
    wrap: bool,
    /// What the mouse has marked to be copied
    selection: Option<Selection>,
    /// The symbol of each cell of `content_rect` in the last frame, by row
    /// and column, which is where a marked column falls in its line
    screen: Vec<Vec<String>>,
    /// The lines of content the last frame showed, from the top of the
    /// panel, as marking them copies them
    source: Vec<String>,
    /// What each row of the last frame shows of `source`
    origins: Vec<RowOrigin>,
    /// When text was last copied out of the panel, which it says for a
    /// moment
    copied: Option<Instant>,
    /// When a click marked the cell it landed on, which stands only as
    /// long as a further click could still widen it
    marked: Option<Instant>,
}

/// What a row on screen shows of the line of content it is part of. A
/// line the panel wraps takes several rows, and one it cuts off has more
/// to it than the row shows.
struct RowOrigin {
    /// Index into the content on screen
    line: usize,
    /// Byte offset in that line where the row picks it up
    start: usize,
    /// Byte offset in that line where the row leaves off
    end: usize,
}

/// A place in the content on screen.
struct Spot {
    /// Index into the content on screen
    line: usize,
    /// Byte offset in that line
    at: usize,
}

/// Text marked with the mouse, to be copied when the button comes up.
///
/// What is marked are the cells on screen, so that marking follows the
/// mouse. What is copied is the content behind them, so that a line the
/// panel wrapped or cut off to show is not copied the way it looks.
struct Selection {
    /// Cell the button went down on
    press: Position,
    /// How much of what it is on a cell stands for
    mode: Mode,
    /// End the mark is anchored at, which is the press taken as `mode`
    /// has it
    anchor: Position,
    /// End the drag has reached, taken as `mode` has it
    cursor: Position,
    /// Whether the mouse moved with the button down, so that no further
    /// click can be part of this mark
    dragged: bool,
    /// Whether the button that made the mark is still down, so that the
    /// drag and the release still to come belong to it
    held: bool,
}

/// How much of the content a cell of a mark stands for, which is what
/// the number of presses that began the mark asked for.
#[derive(Clone, Copy)]
enum Mode {
    /// The cell itself
    Cell,
    /// The word it is on
    Word,
    /// The line it is on
    Line,
}

/// How marked text is set apart from the rest.
const SELECTED: Style = Style::new().add_modifier(Modifier::REVERSED);

/// How long the panel says that it has copied something.
const COPIED_SHOWN: Duration = Duration::from_millis(1500);

/// What a double click takes to be part of a word. Change ids, revsets
/// and paths are worth taking whole, so the punctuation inside them
/// counts.
fn is_word(symbol: &str) -> bool {
    !symbol.is_empty()
        && symbol
            .chars()
            .all(|c| c.is_alphanumeric() || "_-./".contains(c))
}

/// Content of the detail panel must be able to render as a paragraph
pub trait DetailContent<'a> {
    /// Render content as a paragraph, and update panel total lines
    fn render_as_paragraph(&self, panel: &mut DetailsPanel, area: Rect) -> Paragraph<'_>;

    /// `count` lines of the content from `top`, as the plain text they
    /// read as. This is what marking them copies, so that a line the
    /// panel has to wrap or cut off to show is still copied whole.
    fn source_lines(&self, top: usize, count: usize) -> Vec<String>;
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
    title_right: Option<Line<'a>>,
    content: Content,
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

    fn source_lines(&self, top: usize, count: usize) -> Vec<String> {
        self.large_string.plain_lines(top, count)
    }
}

impl<'a> DetailContent<'a> for TextContent<'a> {
    fn render_as_paragraph(&self, panel: &mut DetailsPanel, area: Rect) -> Paragraph<'_> {
        panel.content_rect = area;
        panel.lines = self.text.lines.len() as u16;
        panel.clamp_scroll();

        // Cut the visible lines out rather than letting the paragraph
        // scroll, which counts the rows a wrapped line takes and would
        // leave the scroll position naming another line than it does for
        // the scrollbar and for what marking copies
        Paragraph::new(self.visible_lines(panel.scroll as usize, area.height as usize))
    }

    fn source_lines(&self, top: usize, count: usize) -> Vec<String> {
        self.text
            .lines
            .iter()
            .skip(top)
            .take(count)
            .map(ToString::to_string)
            .collect()
    }
}

impl TextContent<'_> {
    /// `count` lines of the content from `top`.
    fn visible_lines(&self, top: usize, count: usize) -> Text<'_> {
        Text::from(
            self.text
                .lines
                .iter()
                .skip(top)
                .take(count)
                .cloned()
                .collect::<Vec<_>>(),
        )
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
            title_right: None,
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

    /// Set a second title, in the top right corner of the frame
    pub fn title_right<T>(&mut self, title: T) -> &mut Self
    where
        T: Into<Line<'a>>,
    {
        self.title_right = Some(title.into());
        self
    }

    pub fn draw(&mut self, f: &mut ratatui::prelude::Frame<'_>, area: ratatui::prelude::Rect) {
        // Remember last rendered rect for mouse event handling
        self.panel.panel_rect = area;
        self.panel.fade();

        // Define border block
        let mut border = Block::bordered()
            .border_type(BorderType::Rounded)
            .padding(Padding::horizontal(1));
        // Apply title if provided
        if let Some(title) = &self.title {
            border = border.title_top(title.clone());
        }
        // What was just copied is worth saying more than whatever the
        // corner says the rest of the time
        let title_right = self
            .panel
            .copied_note()
            .or_else(|| self.title_right.clone());
        if let Some(title) = title_right {
            border = border.title_top(title.right_aligned());
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

        let source = self
            .content
            .source_lines(self.panel.scroll as usize, paragraph_area.height as usize);
        self.panel.paint_selection(f.buffer_mut(), source);
    }
}

impl Selection {
    /// The mark a press on `at` begins, before any drag widens it.
    fn new(at: Position, mode: Mode, ends: Option<(Position, Position)>) -> Self {
        Self {
            press: at,
            mode,
            anchor: ends.map_or(at, |(anchor, _)| anchor),
            cursor: ends.map_or(at, |(_, cursor)| cursor),
            dragged: false,
            held: true,
        }
    }

    /// Whether the mark stands for text rather than for the cell a press
    /// landed on, which marks where a mark may come to be.
    fn marks_text(&self) -> bool {
        self.dragged || !matches!(self.mode, Mode::Cell)
    }

    /// The marked cells of each row, from the first to the last one, as
    /// the row and the columns it is marked from and up to, both ends
    /// included. Rows between the ends of the selection are marked all the
    /// way across `area`, so that marking reads like it does in a
    /// terminal rather than cutting out a rectangle.
    fn rows(&self, area: Rect) -> impl Iterator<Item = (u16, u16, u16)> {
        let cursor = self.cursor;
        let (start, end) = if (self.anchor.y, self.anchor.x) <= (cursor.y, cursor.x) {
            (self.anchor, cursor)
        } else {
            (cursor, self.anchor)
        };

        (start.y..=end.y).map(move |y| {
            let from = if y == start.y { start.x } else { area.left() };
            let to = if y == end.y {
                end.x
            } else {
                area.right().saturating_sub(1)
            };
            (y, from, to)
        })
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
            selection: None,
            screen: Vec::new(),
            source: Vec::new(),
            origins: Vec::new(),
            copied: None,
            marked: None,
        }
    }

    /// Whether the panel has something on screen that goes away on its
    /// own.
    pub fn is_flashing(&self) -> bool {
        self.marked.is_some() || self.copied.is_some()
    }

    /// Take what has been on screen long enough off it. Only drawing may
    /// do so, as being asked whether anything fades must not be what
    /// makes it fade.
    fn fade(&mut self) {
        if self
            .marked
            .is_some_and(|marked| marked.elapsed() >= CLICK_PAUSE)
        {
            self.marked = None;
            self.selection = None;
        }
        if self
            .copied
            .is_some_and(|copied| copied.elapsed() >= COPIED_SHOWN)
        {
            self.copied = None;
        }
    }

    /// What the panel says in its corner just after copying, until the
    /// news is stale.
    fn copied_note(&self) -> Option<Line<'static>> {
        self.copied
            .is_some()
            .then(|| Line::styled(" Copied ", Style::new().fg(Color::Green)))
    }

    /// Take down where each row on screen picks up the content it shows,
    /// so that marking it copies the content rather than the display, and
    /// set the marked cells apart from the rest.
    fn paint_selection(&mut self, buffer: &mut Buffer, source: Vec<String>) {
        let area = self.content_rect;
        // Taken down cell by cell into what the last frame left behind
        self.screen.resize_with(area.height as usize, Vec::new);
        for (row, y) in self.screen.iter_mut().zip(area.top()..area.bottom()) {
            row.resize_with(area.width as usize, String::new);
            for (cell, x) in row.iter_mut().zip(area.left()..area.right()) {
                cell.clear();
                if let Some(drawn) = buffer.cell(Position::new(x, y)) {
                    cell.push_str(drawn.symbol());
                }
            }
        }
        // Where the rows pick up the content is worked out only once the
        // mouse asks for it
        self.origins.clear();
        self.source = source;

        let Some(selection) = self.mark() else {
            return;
        };
        for (y, from, to) in selection.rows(area) {
            for x in from..=to {
                if let Some(cell) = buffer.cell_mut(Position::new(x, y)) {
                    cell.set_style(SELECTED);
                }
            }
        }
    }

    /// What is marked on screen, which a press that has yet to be
    /// widened or dragged into a mark is not.
    fn mark(&self) -> Option<&Selection> {
        self.selection.as_ref().filter(|mark| mark.marks_text())
    }

    /// Work out where the rows of the last frame pick up the content, if
    /// nothing has asked since it was drawn.
    fn locate_rows(&mut self) {
        if self.origins.is_empty() {
            self.origins = self.map_rows();
        }
    }

    /// Where each row on screen picks up the line of the content it
    /// shows. The row a wrapped line goes on with reads on from where the
    /// row before it left off, which is where that row's text is found
    /// again in the line.
    fn map_rows(&self) -> Vec<RowOrigin> {
        let mut rows: Vec<RowOrigin> = Vec::new();
        for (line, text) in self.source.iter().enumerate() {
            let mut start = 0;
            for row in 0..self.rows_of(text) {
                if rows.len() >= self.screen.len() {
                    return rows;
                }
                let shown = self.row_text(rows.len());
                if row > 0 {
                    // A row we cannot find again in its line leaves us
                    // nowhere to pick it up from, so it stands for
                    // whatever the line has left and the rows after it
                    // for nothing.
                    let Some(found) = text[start..].find(&shown) else {
                        rows.push(RowOrigin {
                            line,
                            start,
                            end: text.len(),
                        });
                        start = text.len();
                        continue;
                    };
                    start += found;
                }
                let end = (start + shown.len()).min(text.len());
                rows.push(RowOrigin { line, start, end });
                start = end;
            }
        }
        rows
    }

    /// How many rows a line of content takes on screen.
    fn rows_of(&self, line: &str) -> usize {
        if !self.wrap {
            return 1;
        }
        Paragraph::new(line)
            .wrap(Wrap { trim: false })
            .line_count(self.content_rect.width)
            .max(1)
    }

    /// What the row at `index` of the panel showed in the last frame,
    /// without the blanks it was padded with.
    fn row_text(&self, index: usize) -> String {
        let Some(row) = self.screen.get(index) else {
            return String::new();
        };
        row.concat().trim_end().to_owned()
    }

    /// What a press on `position` marks: the word under it when it is the
    /// second in a row, the line it is on when it is the third, and
    /// nothing yet when it stands on its own, as the mark is then the
    /// drag that may follow.
    fn press(&self, position: Position, clicks: u8) -> Selection {
        let mode = match clicks {
            2 => Mode::Word,
            3 => Mode::Line,
            _ => Mode::Cell,
        };
        match self.extent(position, mode) {
            // Nothing to widen the press to leaves the cell it landed on
            // standing for itself
            None => Selection::new(position, Mode::Cell, None),
            ends => Selection::new(position, mode, ends),
        }
    }

    /// Widen the mark to where the drag has taken it, both of its ends
    /// standing for as much as the presses that began it asked for.
    fn extend(&mut self, to: Position) {
        let Some(selection) = self.selection.take() else {
            return;
        };
        let cell = |at: Position| (at, at);
        let from = self
            .extent(selection.press, selection.mode)
            .unwrap_or_else(|| cell(selection.press));
        let onto = self.extent(to, selection.mode).unwrap_or_else(|| cell(to));

        // Which end of each is the outer one depends on which way the
        // drag went
        let (anchor, cursor) = if (onto.0.y, onto.0.x) >= (from.0.y, from.0.x) {
            (from.0, onto.1)
        } else {
            (from.1, onto.0)
        };

        self.selection = Some(Selection {
            anchor,
            cursor,
            dragged: true,
            ..selection
        });
    }

    /// The cells the mark takes `position` to stand for. A cell stands
    /// for itself, so a mark of cells has nothing to widen it to.
    fn extent(&self, position: Position, mode: Mode) -> Option<(Position, Position)> {
        match mode {
            Mode::Cell => None,
            Mode::Word => self.word_at(position),
            Mode::Line => self.line_at(position),
        }
    }

    /// The cells the word at `position` covers, if there is one there.
    fn word_at(&self, position: Position) -> Option<(Position, Position)> {
        let row = self.screen_row(position.y)?;
        let column = position.x.checked_sub(self.content_rect.left())? as usize;
        if !is_word(row.get(column)?) {
            return None;
        }

        let from = row[..column]
            .iter()
            .rposition(|symbol| !is_word(symbol))
            .map_or(0, |before| before + 1);
        let to = row[column..]
            .iter()
            .position(|symbol| !is_word(symbol))
            .map_or(row.len() - 1, |behind| column + behind - 1);

        let left = self.content_rect.left();
        Some((
            Position::new(left + from as u16, position.y),
            Position::new(left + to as u16, position.y),
        ))
    }

    /// The cells the line at `position` covers, which are all of the rows
    /// it took to show it.
    fn line_at(&self, position: Position) -> Option<(Position, Position)> {
        let top = self.content_rect.top();
        let row = position.y.checked_sub(top)? as usize;
        let line = self.origins.get(row)?.line;
        let same_line = |row: &usize| self.origins.get(*row).is_some_and(|at| at.line == line);

        let first = (0..=row).rev().take_while(same_line).last()?;
        let last = (row..self.origins.len()).take_while(same_line).last()?;

        Some((
            Position::new(self.content_rect.left(), top + first as u16),
            Position::new(
                self.content_rect.right().saturating_sub(1),
                top + last as u16,
            ),
        ))
    }

    /// The cells of the row at `y` as the last frame drew them.
    fn screen_row(&self, y: u16) -> Option<&Vec<String>> {
        let row = y.checked_sub(self.content_rect.top())?;
        self.screen.get(row as usize)
    }

    /// The content the marked cells of the last frame show. A line the
    /// panel wrapped comes out as the one line it is, and one it cut off
    /// comes out whole.
    fn marked_text(&self, selection: &Selection) -> String {
        let marked: Vec<_> = selection.rows(self.content_rect).collect();
        let (Some(&(first, from, _)), Some(&(last, _, to))) = (marked.first(), marked.last())
        else {
            return String::new();
        };
        // A word is marked as far as it goes, while a mark that is not
        // bounded by what it is on takes what its last row leaves off
        // showing of the line it ends in
        let whole_line = !matches!(selection.mode, Mode::Word);
        let (Some(start), Some(end)) = (
            self.mark_start(first, from),
            self.mark_end(last, to, whole_line),
        ) else {
            return String::new();
        };

        if start.line == end.line {
            return self.source[start.line][start.at..end.at].to_owned();
        }

        let mut text = self.source[start.line][start.at..].to_owned();
        for line in &self.source[start.line + 1..end.line] {
            text.push('\n');
            text.push_str(line);
        }
        text.push('\n');
        text.push_str(&self.source[end.line][..end.at]);
        text
    }

    /// Where the content picks up what the mark begins at, which is the
    /// column it was drawn from on the row it is on.
    fn mark_start(&self, y: u16, column: u16) -> Option<Spot> {
        let origin = self.origin_of(y)?;
        let line = self.source.get(origin.line)?;
        let at = origin.start + self.row_prefix(y, column).len();
        Some(Spot {
            line: origin.line,
            at: char_boundary(line, at.min(origin.end)),
        })
    }

    /// Where the content leaves off what the mark ends at. With
    /// `whole_line`, a mark that reaches the end of a row takes the rest
    /// of the line the row shows, which is what a line the panel wrapped
    /// or cut off has more of.
    fn mark_end(&self, y: u16, column: u16, whole_line: bool) -> Option<Spot> {
        let origin = self.origin_of(y)?;
        let line = self.source.get(origin.line)?;
        let at = origin.start + self.row_prefix(y, column.saturating_add(1)).len();
        let at = if whole_line && at >= origin.end && self.ends_its_line(y) {
            line.len()
        } else {
            at.min(origin.end)
        };
        Some(Spot {
            line: origin.line,
            at: char_boundary(line, at),
        })
    }

    /// Where the row at `y` picks up the content it shows. Rows past the
    /// content show nothing, and end up at the last one that does.
    fn origin_of(&self, y: u16) -> Option<&RowOrigin> {
        let row = y.checked_sub(self.content_rect.top())? as usize;
        self.origins.get(row).or_else(|| self.origins.last())
    }

    /// Whether the row at `y` is the last one on screen showing its line,
    /// so that whatever the line has past it is not marked elsewhere.
    fn ends_its_line(&self, y: u16) -> bool {
        let Some(origin) = self.origin_of(y) else {
            return false;
        };
        let Some(row) = y.checked_sub(self.content_rect.top()) else {
            return false;
        };
        self.origins
            .get(row as usize + 1)
            .is_none_or(|next| next.line != origin.line)
    }

    /// What the row at `y` shows left of `column`.
    fn row_prefix(&self, y: u16, column: u16) -> String {
        let Some(row) = y
            .checked_sub(self.content_rect.top())
            .and_then(|row| self.screen.get(row as usize))
        else {
            return String::new();
        };
        let columns = column.saturating_sub(self.content_rect.left()) as usize;
        row.iter().take(columns).map(String::as_str).collect()
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
        // What was marked is gone from where it was marked.
        self.selection = None;
        self.marked = None;
        self.scroll_to(self.scroll.saturating_add_signed(scroll as i16))
    }

    pub fn handle_event(&mut self, event: DetailsPanelEvent) {
        match event {
            DetailsPanelEvent::ScrollDown => self.scroll(1),
            DetailsPanelEvent::ScrollUp => self.scroll(-1),
            DetailsPanelEvent::ScrollDownHalfPage => self.scroll(self.rows() as isize / 2),
            DetailsPanelEvent::ScrollUpHalfPage => {
                self.scroll((self.rows() as isize / 2).saturating_neg())
            }
            DetailsPanelEvent::ScrollDownPage => self.scroll(self.rows() as isize),
            DetailsPanelEvent::ScrollUpPage => self.scroll((self.rows() as isize).saturating_neg()),
            DetailsPanelEvent::ToggleWrap => {
                // Every row comes to show something else, so the marked
                // cells no longer stand for what was marked
                self.selection = None;
                self.marked = None;
                self.wrap = !self.wrap;
            }
            DetailsPanelEvent::ToggleDiffFormat | DetailsPanelEvent::Unbound => {}
        }
    }
}

impl PanelMouseInput for DetailsPanel {
    fn input_mouse(&mut self, mouse: Mouse) -> MouseInput {
        let position = mouse.position();

        // A drag that started in the panel goes on wherever it leads, so
        // marking text does not end at the edge of the panel. A mark the
        // button is no longer down on is done with, so what the mouse
        // does next is nothing to do with it.
        match mouse.kind() {
            MouseEventKind::Drag(MouseButton::Left)
                if self.selection.as_ref().is_some_and(|mark| mark.held) =>
            {
                self.locate_rows();
                self.extend(clamp(position, self.content_rect));
                return MouseInput::Handled;
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(mut selection) = self.selection.take_if(|mark| mark.held) {
                    self.locate_rows();
                    selection.held = false;
                    // A press of its own is only where a drag or a
                    // further click may come to mark something, so it
                    // leaves nothing behind.
                    if !selection.marks_text() {
                        return MouseInput::Handled;
                    }
                    let text = self.marked_text(&selection);
                    if text.is_empty() {
                        return MouseInput::Handled;
                    }
                    // A drag is over once the button comes up, while a
                    // click may still be widened by the next one, so its
                    // mark stands until no further click can come.
                    if !selection.dragged {
                        self.selection = Some(selection);
                        self.marked = Some(Instant::now());
                    }
                    self.copied = Some(Instant::now());
                    return MouseInput::Copy(text);
                }
            }
            _ => {}
        }

        if !self.panel_rect.contains(position) {
            trace!("mouse {:?} not in rect {:?}", &mouse, &self.panel_rect);
            return MouseInput::NotHandled;
        }
        trace!("mouse {:?} inside  rect {:?}", &mouse, &self.panel_rect);
        match mouse.kind() {
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
            MouseEventKind::Down(MouseButton::Left) => {
                // The mark this press makes stands on its own terms,
                // however the one it replaces was to go
                self.marked = None;
                self.selection = if self.content_rect.contains(position) {
                    self.locate_rows();
                    Some(self.press(position, mouse.clicks()))
                } else {
                    None
                };
            }
            _ => return MouseInput::NotHandled,
        }
        MouseInput::Handled
    }
}

/// `at` pulled back to where a character of `line` begins, so that a row
/// whose text is not found where it was looked for cannot cut one in two.
fn char_boundary(line: &str, at: usize) -> usize {
    let mut at = at.min(line.len());
    while !line.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The cell of `area` closest to `position`.
fn clamp(position: Position, area: Rect) -> Position {
    Position::new(
        position
            .x
            .clamp(area.left(), area.right().saturating_sub(1).max(area.left())),
        position
            .y
            .clamp(area.top(), area.bottom().saturating_sub(1).max(area.top())),
    )
}

#[cfg(test)]
mod tests {
    use ratatui::widgets::Widget;

    use super::*;

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 40,
        height: 10,
    };

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Mouse {
        Mouse::new(kind, Position::new(column, row), 1)
    }

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

    /// A panel of [AREA] that has drawn `content` the way a frame does,
    /// and the frame it drew it on.
    fn drawn(content: &str, wrap: bool) -> (DetailsPanel, Buffer) {
        let content = LargeString::new(content.to_owned());
        let content = LargeStringContent::from(&content);
        let mut panel = DetailsPanel::new();
        panel.panel_rect = AREA;
        panel.wrap = wrap;

        let mut paragraph = content.render_as_paragraph(&mut panel, AREA);
        if wrap {
            paragraph = paragraph.wrap(Wrap { trim: false });
        }
        let mut buffer = Buffer::empty(AREA);
        paragraph.render(AREA, &mut buffer);
        let source = content.source_lines(panel.scroll as usize, AREA.height as usize);
        panel.paint_selection(&mut buffer, source);

        (panel, buffer)
    }

    /// A panel of [AREA] that has drawn `text` wrapped, scrolled down by
    /// `scroll` lines, the way a frame does.
    fn drawn_text(text: &str, scroll: u16) -> DetailsPanel {
        let content = TextContent::from(text.to_owned());
        let mut panel = DetailsPanel::new();
        panel.panel_rect = AREA;
        panel.wrap = true;
        panel.scroll = scroll;

        let paragraph = content
            .render_as_paragraph(&mut panel, AREA)
            .wrap(Wrap { trim: false });
        let mut buffer = Buffer::empty(AREA);
        paragraph.render(AREA, &mut buffer);
        let source = content.source_lines(panel.scroll as usize, AREA.height as usize);
        panel.paint_selection(&mut buffer, source);

        panel
    }

    /// A paragraph of its own scrolls by the rows a wrapped line takes,
    /// while the scroll position names a line of content everywhere else,
    /// so what is on the top row is the line it names.
    #[test]
    fn scrolled_wrapped_text_is_read_from_the_lines_it_shows() {
        let long = "one two three four five six seven eight nine ten eleven twelve";
        let lines: Vec<String> = (1..20).map(|n| format!("line {n}")).collect();
        let mut panel = drawn_text(&format!("{long}\n{}\n", lines.join("\n")), 1);

        assert_eq!(marked(&mut panel, (0, 0), (39, 0)), "line 1");
    }

    /// What `panel` reads with `anchor` up to `cursor` dragged over in it.
    fn marked(panel: &mut DetailsPanel, anchor: (u16, u16), cursor: (u16, u16)) -> String {
        panel.locate_rows();
        let anchor = Position::new(anchor.0, anchor.1);
        panel.selection = Some(Selection {
            dragged: true,
            ..Selection::new(
                anchor,
                Mode::Cell,
                Some((anchor, Position::new(cursor.0, cursor.1))),
            )
        });
        panel.marked_text(panel.selection.as_ref().unwrap())
    }

    #[test]
    fn marking_within_a_line_reads_what_is_between_its_ends() {
        let (mut panel, _) = drawn("hello world\n", false);

        assert_eq!(marked(&mut panel, (6, 0), (10, 0)), "world");
    }

    /// What a panel shows is coloured by escape sequences that take up
    /// no cell, so where a mark falls in the content is not where it
    /// falls in the line as it is stored.
    #[test]
    fn marking_coloured_content_reads_what_is_on_screen() {
        let (mut panel, _) = drawn("\x1b[1;31mhello\x1b[0m world\n", false);

        assert_eq!(marked(&mut panel, (6, 0), (10, 0)), "world");
        assert_eq!(click(&mut panel, (2, 0), 2).as_deref(), Some("hello"));
    }

    #[test]
    fn marking_backwards_reads_the_same_text() {
        let (mut panel, _) = drawn("hello world\n", false);

        assert_eq!(marked(&mut panel, (10, 0), (6, 0)), "world");
    }

    #[test]
    fn marking_across_lines_takes_the_ones_between_whole() {
        let (mut panel, _) = drawn("first line\nsecond line\nthird line\n", false);

        assert_eq!(
            marked(&mut panel, (6, 0), (4, 2)),
            "line\nsecond line\nthird"
        );
    }

    /// The panel shows what fits, but a mark reaching the edge is for the
    /// line, not for the part of it that got drawn.
    #[test]
    fn marking_a_line_that_is_cut_off_takes_all_of_it() {
        let line = "0123456789".repeat(6);
        let (mut panel, _) = drawn(&format!("{line}\n"), false);

        assert_eq!(marked(&mut panel, (0, 0), (39, 0)), line);
    }

    #[test]
    fn marking_a_wrapped_line_takes_it_as_the_one_line_it_is() {
        let line = "one two three four five six seven eight nine ten eleven";
        let (mut panel, _) = drawn(&format!("{line}\n"), true);

        assert_eq!(marked(&mut panel, (0, 0), (39, 1)), line);
    }

    /// Marking one row of a wrapped line is for that much of the line,
    /// which reads on where the row before it left off.
    #[test]
    fn marking_a_row_of_a_wrapped_line_takes_what_it_shows_of_it() {
        let line = "one two three four five six seven eight nine ten eleven";
        let (mut panel, _) = drawn(&format!("{line}\n"), true);

        assert_eq!(marked(&mut panel, (0, 1), (39, 1)), "nine ten eleven");
    }

    #[test]
    fn releasing_the_button_copies_what_was_marked_and_lets_go_of_it() {
        let (mut panel, mut buffer) = drawn("hello world\n", false);

        panel.input_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 6, 0));
        panel.input_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 10, 0));
        panel.paint_selection(&mut buffer, panel.source.clone());
        let copied = panel.input_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 10, 0));

        assert!(matches!(copied, MouseInput::Copy(text) if text == "world"));
        assert!(panel.selection.is_none());
        assert!(panel.copied_note().is_some());
    }

    /// Press and release the left button `times` in a row on a cell, and
    /// report what each release did.
    fn press_release(panel: &mut DetailsPanel, at: (u16, u16), times: usize) -> Vec<MouseInput> {
        (1..=times)
            .map(|clicks| {
                panel.input_mouse(clicked(MouseButton::Left, at, clicks as u8));
                panel.input_mouse(released(MouseButton::Left, at, clicks as u8))
            })
            .collect()
    }

    /// The press that a click on `at` is the `clicks`th of, as the app
    /// counts them before the panel is told.
    fn clicked(button: MouseButton, at: (u16, u16), clicks: u8) -> Mouse {
        Mouse::new(
            MouseEventKind::Down(button),
            Position::new(at.0, at.1),
            clicks,
        )
    }

    /// The release that ends such a press.
    fn released(button: MouseButton, at: (u16, u16), clicks: u8) -> Mouse {
        Mouse::new(
            MouseEventKind::Up(button),
            Position::new(at.0, at.1),
            clicks,
        )
    }

    /// Click `times` in a row on a cell and report what the last of them
    /// copied.
    fn click(panel: &mut DetailsPanel, at: (u16, u16), times: usize) -> Option<String> {
        match press_release(panel, at, times).pop() {
            Some(MouseInput::Copy(text)) => Some(text),
            _ => None,
        }
    }

    #[test]
    fn a_double_click_copies_the_word_it_is_on() {
        let (mut panel, _) = drawn("show 5b41ab5f in src/ui/mod.rs\n", false);

        assert_eq!(click(&mut panel, (7, 0), 2).as_deref(), Some("5b41ab5f"));
    }

    /// What a change id or a path is made of holds together, or a double
    /// click would leave most of one behind.
    #[test]
    fn a_double_click_takes_a_path_whole() {
        let (mut panel, _) = drawn("show 5b41ab5f in src/ui/mod.rs\n", false);

        assert_eq!(
            click(&mut panel, (22, 0), 2).as_deref(),
            Some("src/ui/mod.rs")
        );
    }

    /// Press `times` in a row on `at`, then drag to `to` and let go.
    fn click_and_drag(
        panel: &mut DetailsPanel,
        at: (u16, u16),
        times: usize,
        to: (u16, u16),
    ) -> MouseInput {
        press_release(panel, at, times - 1);
        let clicks = times as u8;
        panel.input_mouse(clicked(MouseButton::Left, at, clicks));
        panel.input_mouse(Mouse::new(
            MouseEventKind::Drag(MouseButton::Left),
            Position::new(to.0, to.1),
            clicks,
        ));
        panel.input_mouse(released(MouseButton::Left, to, clicks))
    }

    #[test]
    fn dragging_from_a_double_click_takes_whole_words() {
        let (mut panel, _) = drawn("show 5b41ab5f in src/ui/mod.rs\n", false);

        // From the middle of the first word to the middle of the last
        let copied = click_and_drag(&mut panel, (7, 0), 2, (22, 0));

        assert!(matches!(copied, MouseInput::Copy(text) if text == "5b41ab5f in src/ui/mod.rs"));
    }

    #[test]
    fn dragging_from_a_double_click_backwards_takes_whole_words() {
        let (mut panel, _) = drawn("show 5b41ab5f in src/ui/mod.rs\n", false);

        let copied = click_and_drag(&mut panel, (22, 0), 2, (7, 0));

        assert!(matches!(copied, MouseInput::Copy(text) if text == "5b41ab5f in src/ui/mod.rs"));
    }

    #[test]
    fn dragging_from_a_triple_click_takes_whole_lines() {
        let (mut panel, _) = drawn("first line\nsecond line\nthird line\n", false);

        let copied = click_and_drag(&mut panel, (4, 0), 3, (2, 1));

        assert!(matches!(copied, MouseInput::Copy(text) if text == "first line\nsecond line"));
    }

    /// How many cells the panel has marked.
    fn marked_cells(panel: &DetailsPanel) -> usize {
        panel.mark().map_or(0, |selection| {
            selection
                .rows(panel.content_rect)
                .map(|(_, from, to)| (to - from + 1) as usize)
                .sum()
        })
    }

    /// A click of its own picks nothing, so it marks nothing; what the
    /// clicks after it mark grows from the cell it landed on.
    #[test]
    fn the_clicks_that_widen_a_mark_grow_it_from_the_cell_pressed() {
        let (mut panel, _) = drawn("first line\nsecond line\n", false);

        let mut marked = Vec::new();
        for clicks in 1..=3 {
            panel.input_mouse(clicked(MouseButton::Left, (2, 0), clicks));
            marked.push(marked_cells(&panel));
            panel.input_mouse(released(MouseButton::Left, (2, 0), clicks));
            marked.push(marked_cells(&panel));
        }

        // Nothing, then the word the cell is on, then its line
        assert_eq!(marked, [0, 0, 5, 5, 40, 40]);
    }

    /// A click stands for nothing anyone can act on, so the panel is
    /// left as it was and has nothing to take off screen later.
    #[test]
    fn a_click_leaves_no_mark_behind() {
        let (mut panel, _) = drawn("hello world\n", false);
        press_release(&mut panel, (6, 0), 1);

        assert_eq!(marked_cells(&panel), 0);
        assert!(!panel.is_flashing());
    }

    /// What a double click marks is copied, and stands as long as a
    /// third click could still widen it to the line.
    #[test]
    fn the_mark_a_double_click_makes_stays_for_a_further_click() {
        let (mut panel, _) = drawn("hello world\n", false);
        press_release(&mut panel, (6, 0), 2);

        assert_eq!(marked_cells(&panel), 5);

        panel.marked = Some(Instant::now() - CLICK_PAUSE);
        panel.fade();

        assert_eq!(marked_cells(&panel), 0);
    }

    /// The button is up once a click is over, so a drag that begins
    /// elsewhere is somebody else's.
    #[test]
    fn a_drag_that_began_outside_marks_nothing() {
        let (mut panel, _) = drawn("hello world\n", false);
        press_release(&mut panel, (6, 0), 1);

        let dragged = panel.input_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 10, 0));
        let released = panel.input_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 10, 0));

        assert!(matches!(dragged, MouseInput::NotHandled));
        assert!(matches!(released, MouseInput::NotHandled));
        assert_eq!(marked_cells(&panel), 0);
    }

    /// Every row comes to show something else, so keeping the marked
    /// cells would mark text nobody picked.
    #[test]
    fn toggling_the_wrap_drops_the_mark() {
        let (mut panel, _) = drawn("hello world\n", false);
        press_release(&mut panel, (6, 0), 2);

        panel.handle_event(DetailsPanelEvent::ToggleWrap);

        assert_eq!(marked_cells(&panel), 0);
        assert!(panel.marked.is_none());
    }

    #[test]
    fn a_double_click_between_words_marks_nothing() {
        let (mut panel, _) = drawn("hello world\n", false);

        assert_eq!(click(&mut panel, (5, 0), 2), None);
    }

    #[test]
    fn a_triple_click_copies_the_line_it_is_on_however_it_was_drawn() {
        let line = "one two three four five six seven eight nine ten eleven";
        let (mut panel, _) = drawn(&format!("{line}\n"), true);

        assert_eq!(click(&mut panel, (2, 0), 3).as_deref(), Some(line));
    }

    #[test]
    fn a_click_marks_nothing_to_copy() {
        let (mut panel, _) = drawn("hello world\n", false);

        panel.input_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 6, 0));
        let copied = panel.input_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 6, 0));

        assert!(matches!(copied, MouseInput::Handled));
        assert!(panel.copied_note().is_none());
    }

    #[test]
    fn the_word_of_a_copy_goes_off_screen_on_its_own() {
        let mut panel = panel_showing(1);
        panel.copied = Some(Instant::now());

        assert!(panel.is_flashing());
        assert!(panel.copied_note().is_some());

        panel.copied = Some(Instant::now() - COPIED_SHOWN);

        // The word is still on screen, so the frame that takes it off is
        // asked for
        assert!(panel.is_flashing());

        panel.fade();

        assert!(panel.copied_note().is_none());
        assert!(!panel.is_flashing());
    }

    #[test]
    fn marked_cells_are_set_apart() {
        let (mut panel, mut buffer) = drawn("hello world\n", false);
        marked(&mut panel, (6, 0), (10, 0));
        panel.paint_selection(&mut buffer, panel.source.clone());

        assert!(!buffer[(5, 0)].modifier.contains(Modifier::REVERSED));
        assert!(buffer[(6, 0)].modifier.contains(Modifier::REVERSED));
        assert!(buffer[(10, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(11, 0)].modifier.contains(Modifier::REVERSED));
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
