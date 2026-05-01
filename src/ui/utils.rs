mod large_string;
pub use large_string::LargeString;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;

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

/// Place a rect of fixed width and height with its top-left at `anchor`,
/// clamped so it stays within `area`.
pub fn anchored_rect_fixed(
    area: Rect,
    anchor: ratatui::layout::Position,
    width: u16,
    height: u16,
) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let max_x = area.x + area.width.saturating_sub(width);
    let max_y = area.y + area.height.saturating_sub(height);
    Rect {
        x: anchor.x.clamp(area.x, max_x),
        y: anchor.y.clamp(area.y, max_y),
        width,
        height,
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
