mod large_string;
pub use large_string::LargeString;
use ratatui::crossterm::event::MouseButton;
use ratatui::crossterm::event::MouseEvent;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;

use crate::env::JJLayout;

/// Tracks the split position between two panes and handles drag-to-resize mouse events.
pub struct PaneDivider {
    init_percent: u16,
    size: Option<u16>,
    dragging: bool,
    rects: [Rect; 2],
}

impl PaneDivider {
    pub fn new(percent: u16) -> Self {
        Self {
            init_percent: percent.min(100),
            size: None,
            dragging: false,
            rects: [Rect::ZERO, Rect::ZERO],
        }
    }

    /// Split `area` into two panes at the current divider position and remember
    /// the resulting rects for hit-testing in `handle_mouse`.
    pub fn split(&mut self, area: Rect, layout: JJLayout) -> [Rect; 2] {
        let total = match layout {
            JJLayout::Horizontal => area.width,
            JJLayout::Vertical => area.height,
        };
        let size = match self.size {
            None => {
                let s = ((total as u32 * self.init_percent as u32) / 100) as u16;
                self.size = Some(s);
                s
            }
            Some(s) => s,
        };
        let size = size.min(total);

        let chunks = Layout::default()
            .direction(layout.into())
            .constraints([Constraint::Length(size), Constraint::Fill(1)])
            .split(area);
        self.rects = [chunks[0], chunks[1]];
        self.rects
    }

    /// Handle a mouse event. Returns true if the event was consumed.
    pub fn handle_mouse(&mut self, mouse: MouseEvent, layout: JJLayout) -> bool {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.dragging = false;
                if self.on_border(mouse.column, mouse.row, layout) {
                    self.dragging = true;
                    self.update_size(mouse.column, mouse.row, layout);
                    true
                } else {
                    false
                }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => {
                self.update_size(mouse.column, mouse.row, layout);
                true
            }
            MouseEventKind::Up(MouseButton::Left) if self.dragging => {
                self.dragging = false;
                true
            }
            _ => false,
        }
    }

    fn on_border(&self, col: u16, row: u16, layout: JJLayout) -> bool {
        let [r0, r1] = self.rects;
        match layout {
            JJLayout::Horizontal => {
                let in_row = row >= r0.top() && row < r0.bottom();
                // Right border of r0 and left border of r1 are adjacent columns.
                let on_col = col == r0.right().saturating_sub(1) || col == r1.left();
                in_row && on_col
            }
            JJLayout::Vertical => {
                let in_col = col >= r0.left() && col < r0.right();
                let on_row = row == r0.bottom().saturating_sub(1) || row == r1.top();
                in_col && on_row
            }
        }
    }

    /// Reset the cached split size so the next `split` call recalculates from
    /// `init_percent`. Call this when the layout direction changes.
    pub fn reset(&mut self) {
        self.size = None;
    }

    fn update_size(&mut self, col: u16, row: u16, layout: JJLayout) {
        let [r0, r1] = self.rects;
        let (pos, total) = match layout {
            JJLayout::Horizontal => (
                col.saturating_sub(r0.left()),
                r1.right().saturating_sub(r0.left()),
            ),
            JJLayout::Vertical => (
                row.saturating_sub(r0.top()),
                r1.bottom().saturating_sub(r0.top()),
            ),
        };
        // pos is a 0-based cell index, so it tops out at total-1; snap to
        // total when the mouse reaches the far edge so the first pane can
        // expand to full size. Enforce a minimum of 1 so the pane stays visible.
        let size = if pos >= total.saturating_sub(1) {
            total
        } else {
            pos.max(1)
        };
        self.size = Some(size);
    }
}

pub fn centered_rect(r: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn centered_rect_line_height(r: Rect, percent_x: u16, lines_y: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(lines_y),
            Constraint::Fill(1),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Center a rect of fixed width and height within an outside rect
pub fn centered_rect_fixed(area: Rect, width: u16, height: u16) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;

    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

/// replaces tabs in a string by spaces
///
/// ratatui doesn't work well displaying tabs, so any
/// string that is rendered and might contain tabs
/// needs to have the tabs converted to spaces.
///
/// this function aligns tabs in the input string to
/// virtual tab stops 4 spaces apart, taking care
/// to count ansi control sequences as zero width.
pub fn tabs_to_spaces(line: &str) -> String {
    const TAB_WIDTH: usize = 4;

    enum AnsiState {
        Neutral,
        Escape,
        Csi,
    }

    let mut out = String::new();
    let mut x = 0;
    let mut ansi_state = AnsiState::Neutral;
    for c in line.chars() {
        match ansi_state {
            AnsiState::Neutral => {
                if c == '\t' {
                    loop {
                        out.push(' ');
                        x += 1;
                        if x % TAB_WIDTH == 0 {
                            break;
                        }
                    }
                } else {
                    out.push(c);
                    if c == '\x1b' {
                        ansi_state = AnsiState::Escape;
                    } else {
                        x += 1;
                    }
                }
                if c == '\r' || c == '\n' {
                    x = 0;
                }
            }
            AnsiState::Escape => {
                out.push(c);
                ansi_state = if c == '[' {
                    AnsiState::Csi
                } else {
                    AnsiState::Neutral
                };
            }
            AnsiState::Csi => {
                out.push(c);
                if ('\x40'..='\x7f').contains(&c) {
                    ansi_state = AnsiState::Neutral;
                }
            }
        }
    }
    out
}
