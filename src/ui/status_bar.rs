/*! The bar along the foot of the window, saying where the app is
working and what it is showing there.
*/

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::symbols;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

/// The keys the bar names, in the two pieces the refresh key is lit up
/// on its own in.
const HINTS_BEFORE_REFRESH: &str = "q: quit | ?: help | ";
const HINTS_REFRESH: &str = "R: refresh";

/// How much of the right of the bar the runtime is drawn over. It is a
/// count of milliseconds, which takes a column more every tenfold, so
/// this is room for a session of days rather than a fixed width.
const RUNTIME_WIDTH: usize = 12;

/// How wide the keys and the runtime beside them are.
pub const HINTS_WIDTH: u16 =
    (HINTS_BEFORE_REFRESH.len() + HINTS_REFRESH.len() + RUNTIME_WIDTH) as u16;

/// What the bar says, as the app has it when it draws.
pub struct Status<'a> {
    /// The workspace we are running in, as the repo names it. A repo
    /// that says nothing about where its workspaces are may name none.
    pub workspace: Option<&'a str>,
    /// The directory we are working in, which is that workspace's.
    pub root: &'a str,
    /// The revset the log is showing, where it is showing one of its own
    /// rather than what jj lists by default.
    pub revset: Option<&'a str>,
    /// How many changes the log has marked.
    pub marked: usize,
    /// Whether the repo has moved since what is on screen was read.
    pub stale: bool,
    /// How long the app has been at what it is doing, which is what the
    /// runtime counts.
    pub elapsed: Duration,
}

/// Draw the bar in `area`, which is the single row along the foot of the
/// window.
pub fn draw(f: &mut Frame<'_>, area: Rect, status: &Status) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Max(HINTS_WIDTH)])
        .split(area);

    f.render_widget(Paragraph::new(where_we_are(status)), chunks[0]);
    f.render_widget(
        Paragraph::new(hints(status)).alignment(Alignment::Right),
        chunks[1],
    );
}

/// Where the app is working and what it is up to there.
fn where_we_are(status: &Status) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];

    if let Some(workspace) = status.workspace {
        spans.push(Span::styled(
            workspace.to_owned(),
            Style::default().fg(Color::Cyan),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        in_home(status.root),
        Style::default().fg(Color::DarkGray),
    ));

    if let Some(revset) = status.revset {
        spans.extend(divider());
        spans.push(Span::raw(revset.to_owned()));
    }

    if status.marked > 0 {
        spans.extend(divider());
        spans.push(Span::raw(format!("{} marked", status.marked)));
    }

    Line::from(spans)
}

/// What sits between two things the bar says.
fn divider() -> [Span<'static>; 3] {
    let divider = Span::styled(
        symbols::line::VERTICAL,
        Style::default().fg(Color::DarkGray),
    );

    [Span::raw(" "), divider, Span::raw(" ")]
}

/// The keys the bar names and the runtime, with the refresh key lit up
/// while the app is not going to pick the repo up by itself.
fn hints(status: &Status) -> Line<'static> {
    let refresh_style = if status.stale {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    Line::from(vec![
        Span::styled(HINTS_BEFORE_REFRESH, Style::default().fg(Color::DarkGray)),
        Span::styled(HINTS_REFRESH, refresh_style),
        Span::styled(
            format!(" {}ms ", status.elapsed.as_millis()),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

/// `path` with the home directory it is in written as `~`.
fn in_home(path: &str) -> String {
    let Some(home) = std::env::home_dir().filter(|home| !home.as_os_str().is_empty()) else {
        return path.to_owned();
    };
    let home = home.to_string_lossy();

    match path.strip_prefix(home.as_ref()) {
        // The home directory itself, or a path inside it. Anything else
        // only starts with the same characters, such as a sibling whose
        // name the home directory's is the start of.
        Some("") => "~".to_owned(),
        Some(inside) if inside.starts_with('/') => format!("~{inside}"),
        _ => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> Status<'static> {
        Status {
            workspace: Some("default"),
            root: "/tmp/repo",
            revset: None,
            marked: 0,
            stale: false,
            elapsed: Duration::ZERO,
        }
    }

    /// What the bar says, as it reads without the styling.
    fn text(status: &Status) -> String {
        where_we_are(status).to_string()
    }

    #[test]
    fn the_bar_says_which_workspace_we_are_working_in() {
        assert_eq!(text(&status()), " default /tmp/repo");
    }

    /// The revset and the marks are only worth a word while there are
    /// any, so the bar stays quiet about them until there are.
    #[test]
    fn the_bar_says_what_the_log_is_showing_and_has_marked() {
        let quiet = status();
        assert!(!text(&quiet).contains('│'), "{}", text(&quiet));

        let showing = Status {
            revset: Some("trunk()..@"),
            marked: 2,
            ..status()
        };
        assert_eq!(text(&showing), " default /tmp/repo │ trunk()..@ │ 2 marked");
    }

    #[test]
    fn a_directory_in_the_home_directory_is_written_as_such() {
        let home = std::env::home_dir().expect("a home directory");
        let home = home.to_string_lossy();

        assert_eq!(in_home(&home), "~");
        assert_eq!(in_home(&format!("{home}/src/repo")), "~/src/repo");
        // A directory whose name starts the same way is not inside it.
        assert_eq!(in_home(&format!("{home}x/repo")), format!("{home}x/repo"));
        assert_eq!(in_home("/tmp/repo"), "/tmp/repo");
    }
}
