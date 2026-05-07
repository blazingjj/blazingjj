mod large_string;

use std::fmt;
use std::time::Duration;
use std::time::Instant;

use ansi_to_tui::IntoText;
pub use large_string::LargeString;
use ratatui::crossterm::event::MouseButton;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Position;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Text;
use ratatui::widgets::Block;

use crate::env::JJLayout;
use crate::env::get_env;
use crate::event::Mouse;
use crate::keybinds::Shortcut;

/// Tracks the split position between two panes and handles drag-to-resize mouse events.
#[derive(Default)]
pub struct PaneDivider {
    /// Which way round the panes sit and what share of the area the
    /// first one takes, as the configuration read when the divider was
    /// last placed; it is placed afresh whenever either changes. Its
    /// position counts cells along the axis the panes are divided on,
    /// so it says nothing about them once they turn.
    configured: Option<(JJLayout, u16)>,
    /// The way round the panes have been turned at runtime, which the
    /// configured layout takes back over from when it changes.
    turned_to: Option<JJLayout>,
    size: Option<u16>,
    dragging: bool,
    rects: [Rect; 2],
}

impl PaneDivider {
    /// Split `area` into two panes at the current divider position and remember
    /// the resulting rects for hit-testing in `handle_mouse`.
    pub fn split(&mut self, area: Rect) -> [Rect; 2] {
        let config = &get_env().jj_config;
        let configured = (config.layout(), config.layout_percent());
        // Reading the configuration for the first time is not a change
        // to it, so a tab drawn only now still shows what it was told
        // to while it was not.
        if self
            .configured
            .replace(configured)
            .is_some_and(|was| was != configured)
        {
            self.turned_to = None;
            self.size = None;
        }
        let (_, percent) = configured;

        let layout = self.layout();
        let total = match layout {
            JJLayout::Horizontal => area.width,
            JJLayout::Vertical => area.height,
        };
        let size = match self.size {
            None => {
                let s = ((total as u32 * percent as u32) / 100) as u16;
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
    pub fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        let position = mouse.position();
        let layout = self.layout();
        match mouse.kind() {
            MouseEventKind::Down(MouseButton::Left) => {
                self.dragging = false;
                if self.on_border(position, layout) {
                    self.dragging = true;
                    self.update_size(position, layout);
                    true
                } else {
                    false
                }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => {
                self.update_size(position, layout);
                true
            }
            MouseEventKind::Up(MouseButton::Left) if self.dragging => {
                self.dragging = false;
                true
            }
            _ => false,
        }
    }

    fn on_border(&self, position: Position, layout: JJLayout) -> bool {
        let [r0, r1] = self.rects;
        match layout {
            JJLayout::Horizontal => {
                let in_row = position.y >= r0.top() && position.y < r0.bottom();
                // Right border of r0 and left border of r1 are adjacent columns.
                let on_col = position.x == r0.right().saturating_sub(1) || position.x == r1.left();
                in_row && on_col
            }
            JJLayout::Vertical => {
                let in_col = position.x >= r0.left() && position.x < r0.right();
                let on_row = position.y == r0.bottom().saturating_sub(1) || position.y == r1.top();
                in_col && on_row
            }
        }
    }

    /// Which way round the panes sit, going by what they have been
    /// turned to at runtime over what the configuration says.
    fn layout(&self) -> JJLayout {
        self.turned_to
            .unwrap_or_else(|| get_env().jj_config.layout())
    }

    /// Turn the panes the other way round, until the configuration
    /// says otherwise, placing the divider afresh as it does.
    pub fn toggle_layout(&mut self) {
        self.turned_to = Some(self.layout().toggle());
        self.size = None;
    }

    fn update_size(&mut self, position: Position, layout: JJLayout) {
        let [r0, r1] = self.rects;
        let (pos, total) = match layout {
            JJLayout::Horizontal => (
                position.x.saturating_sub(r0.left()),
                r1.right().saturating_sub(r0.left()),
            ),
            JJLayout::Vertical => (
                position.y.saturating_sub(r0.top()),
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

/// How much wider and taller than its contents a `block` is, in the `area` it
/// has to draw itself in. Its border is not all of it, it also pads.
pub fn chrome(block: &Block, area: Rect) -> [u16; 2] {
    let inner = block.inner(area);

    [
        area.width.saturating_sub(inner.width),
        area.height.saturating_sub(inner.height),
    ]
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

/// `label` with the key that picks it marked: parens around the first
/// character of the label the key is, or the key's name after the label
/// where the label holds no such character.
pub fn mark_key(label: &str, shortcut: Option<Shortcut>) -> String {
    let Some(shortcut) = shortcut else {
        return label.to_owned();
    };

    if let Some(key) = shortcut.as_char()
        && let Some((at, marked)) = label
            .char_indices()
            .find(|(_, c)| c.eq_ignore_ascii_case(&key))
    {
        let (before, rest) = label.split_at(at);
        let after = &rest[marked.len_utf8()..];
        return format!("{before}({marked}){after}");
    }

    format!("{label} ({shortcut})")
}

/// An error under a red title, with any ANSI escape sequences in its
/// message honoured.
pub fn error_text<'a>(
    title: &'a str,
    error: &impl fmt::Display,
) -> Result<Text<'a>, ansi_to_tui::Error> {
    let mut lines = vec![
        Line::raw(title).bold().fg(Color::Red),
        Line::raw(""),
        Line::raw(""),
    ];
    lines.append(&mut error.to_string().into_text()?.lines);

    Ok(Text::from(lines))
}

/// How long a panel keeps quiet about the command it is waiting for.
const LOADING_GRACE: Duration = Duration::from_secs(1);

/// The wait a panel is in while what it shows is being computed
/// elsewhere.
#[derive(Default)]
pub struct PanelWait {
    since: Option<Instant>,
}

impl PanelWait {
    /// Note that the panel wants content it does not have. A wait that is
    /// already going on keeps the start it had, so asking again does not
    /// renew the grace period.
    pub fn begin(&mut self) {
        self.since.get_or_insert_with(Instant::now);
    }

    /// Note that the panel is no longer waiting, because its content
    /// arrived or because it wants something else now.
    pub fn end(&mut self) {
        self.since = None;
    }

    /// Whether the panel is waiting for content at all.
    pub fn is_waiting(&self) -> bool {
        self.since.is_some()
    }

    /// Whether the panel may go on showing content it is about to
    /// replace, rather than saying that it is waiting.
    pub fn within_grace(&self) -> bool {
        self.waited().is_some_and(|waited| waited < LOADING_GRACE)
    }

    /// What the panel shows instead of its content: nothing while the
    /// wait is within its grace, and how long `command` has been running
    /// after that.
    pub fn message(&self, command: &str) -> String {
        let Some(waited) = self.waited() else {
            return String::new();
        };
        // A message that flashes by is only noise.
        if waited < LOADING_GRACE {
            return String::new();
        }

        let seconds = waited.as_secs();
        format!(
            "Waiting for '{command}' .. {:02}:{:02}",
            seconds / 60,
            seconds % 60
        )
    }

    /// How long the panel has been waiting, if it is.
    fn waited(&self) -> Option<Duration> {
        self.since.map(|since| since.elapsed())
    }

    #[cfg(test)]
    fn started_at(since: Instant) -> Self {
        Self { since: Some(since) }
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

/// What `component` draws on a terminal that many columns wide and rows
/// tall, as one string per row of it.
#[cfg(test)]
pub fn drawn(component: &mut impl super::Component, width: u16, height: u16) -> Vec<String> {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("the test backend");
    terminal
        .draw(|f| component.draw(f, f.area()).expect("the component draws"))
        .expect("the frame is drawn");

    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::env::set_test_env;

    /// A divider that was dragged keeps where it was put until the
    /// configuration says something else.
    #[test]
    fn test_a_dragged_divider_goes_back_when_the_configuration_changes() {
        set_test_env();
        let area = Rect::new(0, 0, 100, 10);
        let mut divider = PaneDivider::default();

        let [main, _] = divider.split(area);
        assert_eq!(main.width, 50);

        divider.update_size(Position::new(20, 0), JJLayout::Horizontal);
        let [main, _] = divider.split(area);
        assert_eq!(main.width, 20);

        // What the divider remembers is what the configuration said
        // when it was last placed, so it stands in for a change of it.
        divider.configured = Some((JJLayout::Horizontal, 80));
        let [main, _] = divider.split(area);
        assert_eq!(main.width, 50);
    }

    /// It goes back to it when the panes turn as well.
    #[test]
    fn test_a_dragged_divider_goes_back_when_the_panes_turn() {
        set_test_env();
        let area = Rect::new(0, 0, 100, 10);
        let mut divider = PaneDivider::default();

        divider.split(area);
        divider.update_size(Position::new(20, 0), JJLayout::Horizontal);
        divider.configured = Some((JJLayout::Vertical, 50));

        let [main, _] = divider.split(area);
        assert_eq!(main.width, 50);
    }

    /// A divider that has not been placed yet still turns, so that a
    /// tab drawn only after the panes were turned shows them that way
    /// round as well.
    #[test]
    fn test_a_divider_turns_before_it_is_placed() {
        set_test_env();
        let area = Rect::new(0, 0, 100, 10);
        let mut divider = PaneDivider::default();

        divider.toggle_layout();
        let [main, _] = divider.split(area);

        assert_eq!(main.width, 100);
    }

    /// A label marks the key that picks it in place where the key is one
    /// of its characters, and names it after the label otherwise.
    #[test]
    fn test_mark_key() {
        let bind = |shortcut| Some(Shortcut::from_str(shortcut).expect("shortcut should parse"));

        assert_eq!(mark_key("Yes", bind("y")), "(Y)es");
        assert_eq!(mark_key("No", bind("n")), "(N)o");
        assert_eq!(mark_key("Create bookmark", bind("k")), "Create boo(k)mark");
        // A key the label does not hold, or one no single character
        // stands for, goes after it.
        assert_eq!(mark_key("Yes", bind("q")), "Yes (q)");
        assert_eq!(mark_key("Yes", bind("ctrl+y")), "Yes (Control+y)");
        assert_eq!(mark_key("Yes", bind("enter")), "Yes (Enter)");
        // A binding that has been disabled leaves the label alone.
        assert_eq!(mark_key("Yes", None), "Yes");
    }

    #[test]
    fn an_error_is_shown_under_its_title() -> Result<(), ansi_to_tui::Error> {
        let text = error_text("Error getting diff", &"no such \x1b[31mrevision\x1b[0m")?;

        let lines: Vec<String> = text.lines.iter().map(ToString::to_string).collect();
        assert_eq!(lines, ["Error getting diff", "", "", "no such revision"]);
        assert_eq!(text.lines[0].style.fg, Some(Color::Red));
        // The escape sequence is a style rather than something to read.
        assert_eq!(text.lines[3].spans.len(), 2);

        Ok(())
    }

    #[test]
    fn a_long_wait_names_the_command_and_its_runtime() {
        assert_eq!(
            PanelWait::started_at(Instant::now() - LOADING_GRACE).message("jj show"),
            "Waiting for 'jj show' .. 00:01"
        );
        assert_eq!(
            PanelWait::started_at(Instant::now() - Duration::from_secs(62)).message("jj diff"),
            "Waiting for 'jj diff' .. 01:02"
        );
    }

    #[test]
    fn a_panel_that_waits_for_nothing_says_nothing() {
        let wait = PanelWait::default();

        assert!(!wait.is_waiting());
        assert!(!wait.within_grace());
        assert_eq!(wait.message("jj show"), "");
    }

    #[test]
    fn a_fresh_wait_keeps_the_panel_quiet() {
        let mut wait = PanelWait::default();
        wait.begin();

        assert!(wait.is_waiting());
        assert!(wait.within_grace());
        assert_eq!(wait.message("jj show"), "");
    }

    #[test]
    fn asking_again_does_not_renew_the_grace() {
        let mut wait = PanelWait::started_at(Instant::now() - LOADING_GRACE);
        wait.begin();

        assert!(!wait.within_grace());
        assert_eq!(wait.message("jj show"), "Waiting for 'jj show' .. 00:01");
    }

    #[test]
    fn a_panel_that_is_done_waiting_stops_counting() {
        let mut wait = PanelWait::started_at(Instant::now() - LOADING_GRACE);
        wait.end();

        assert!(!wait.is_waiting());
        assert_eq!(wait.message("jj show"), "");
    }
}
