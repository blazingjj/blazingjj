/*! The log panel shows a list of changes on the left side of a tab. */

use std::collections::HashSet;

use ansi_to_tui::IntoText;
use anyhow::Result;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::*;

use super::ListPane;
use super::MouseInput;
use super::PanelMouseInput;
use crate::commander::CommandError;
use crate::commander::ids::CommitId;
use crate::commander::log::Head;
use crate::commander::log::LogOutput;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::event::Mouse;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::utils::error_text;

/**
    A panel that displays a graph of changes. This panel is used on the
    left side of a tab. It shows a selected change, which is expanded on
    the right side of the tab.

    The log operates with two index:
    - line index (into self.log_output.text)
    - head index (into self.log_output.heads)

    The line index is used for scrolling at the display level.

    The head index is used for scrolling at the user level
    as well as for selecting which lines to highlight.
*/
pub struct LogPanel<'a> {
    /// The log to show, or what went wrong reading it
    log_output: Result<LogOutput, CommandError>,

    /// The log converted to Ratatui Text
    log_output_text: Text<'a>,

    /// The title the log is shown under
    title: String,

    /// How many lines of the graph one head takes
    lines_per_head: usize,

    /// Scroll offset and cursor position
    log_list_state: ListState,

    /// Currently selected commit
    pub head: Head,

    /// Currently marked commits
    pub marked_heads: HashSet<CommitId>,

    list_pane: ListPane,

    /// Configuration of colours
    config: JjConfig,

    /// Whether to apply scroll_padding on the next draw.
    /// Disabled after a right-click so the viewport doesn't jump.
    scroll_padding_active: bool,
}

const LEFT_MARGIN_BLANK: char = ' ';
const LEFT_MARGIN_MARKED: char = '>';

/*
pub enum LogPanelEvent {
    /* Commands to LogPanel */

    /// Refresh current state
    Refresh,
    /// Move selection down the given number of changes
    MoveRelative(isize),

    /* Notifications from LogPanel */

    /// Emitted when selection was changed
    SetHead(Head),
}
*/

fn get_head_index(head: &Head, log_output: &Result<LogOutput, CommandError>) -> Option<usize> {
    match log_output {
        Ok(log_output) => log_output
            .heads
            .iter()
            .position(|heads| heads == head)
            .or_else(|| {
                log_output
                    .heads
                    .iter()
                    .position(|commit| commit.change_id == head.change_id)
            }),
        Err(_) => None,
    }
}

impl<'a> LogPanel<'a> {
    /// A panel showing `head` selected in an empty log. The graphs it is
    /// later given take `lines_per_head` lines per head.
    pub fn new(head: Head, lines_per_head: usize) -> Self {
        Self {
            log_output_text: Text::default(),
            log_output: Ok(LogOutput::default()),
            title: String::new(),
            lines_per_head,
            log_list_state: ListState::default(),

            head,
            marked_heads: HashSet::new(),

            list_pane: ListPane::default(),

            config: get_env().jj_config.clone(),
            scroll_padding_active: true,
        }
    }

    //
    //  Handle jj log output
    //

    /// Replace what the panel shows. An error takes the place of the
    /// graph.
    pub fn show(&mut self, log_output: Result<LogOutput, CommandError>, title: String) {
        self.log_output = log_output;
        self.title = title;
        self.log_output_text = match self.log_output.as_ref() {
            Ok(log_output) => log_output
                .graph
                .into_text()
                .unwrap_or(Text::from("Could not turn text into TUI text (coloring)")),
            Err(_) => Text::default(),
        };
    }

    /// Convert log output to a list of formatted lines
    fn output_to_lines(&self, log_output: &LogOutput) -> Vec<Line<'a>> {
        // Add commit mark
        let add_mark = |line: &mut Line, i: usize| {
            let at_marked_commit = log_output
                .head_at(i)
                .is_some_and(|head| self.is_head_marked(head));

            let symbol = if at_marked_commit {
                LEFT_MARGIN_MARKED
            } else {
                LEFT_MARGIN_BLANK
            };
            let span = Span::from(symbol.to_string());
            line.spans.insert(0, span);
        };

        // Set the background color of the line
        fn set_bg(line: &mut Line, bg_color: Color) {
            // Set background to use when no Span is present
            // This makes the highlight continue beyond the last Span
            line.style = line.style.patch(Style::default().bg(bg_color));

            for span in line.spans.iter_mut() {
                span.style = span.style.bg(bg_color)
            }
        }

        self.log_output_text
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let mut line = line.to_owned();

                // Add padding at start
                add_mark(&mut line, i);

                // Highlight lines that correspond to self.head
                if log_output.head_at(i) == Some(&self.head) {
                    set_bg(&mut line, self.config.highlight_color());
                };

                line
            })
            .collect()
    }

    /// Get lines to show in log list
    fn log_lines(&self) -> Vec<Line<'a>> {
        match self.log_output.as_ref() {
            Ok(log_output) => self.output_to_lines(log_output),
            Err(err) => error_text("Error getting log", err).unwrap().lines,
        }
    }

    /// Get a list of all heads in log list
    pub fn log_heads(&self) -> Vec<Head> {
        match self.log_output.as_ref() {
            Ok(log_output) => log_output.heads.clone(),
            Err(_) => vec![],
        }
    }

    //
    //  Selected head and the special head index
    //

    /// Find the line in self.log_output that match self.head
    fn selected_log_line(&self) -> Option<usize> {
        let log_output = self.log_output.as_ref().ok()?;

        log_output
            .graph_heads
            .iter()
            .position(|opt_h| opt_h.as_ref().is_some_and(|h| h == &self.head))
    }

    /// Where to put something that wants to show up next to the selected
    /// change.
    pub fn selected_position(&self) -> Option<Position> {
        self.list_pane
            .item_anchor(self.selected_log_line()?, self.lines_per_head as u16)
    }

    /// Find head of the provided log_output line
    pub fn head_at_log_line(&self, log_line: usize) -> Option<Head> {
        self.log_output.as_ref().ok()?.head_at(log_line).cloned()
    }

    // Return the head-index for the selection
    fn get_current_head_index(&self) -> Option<usize> {
        get_head_index(&self.head, &self.log_output)
    }

    /// Whether the log holds a head, taking a different commit for the
    /// same change as a match.
    pub fn shows_head(&self, head: &Head) -> bool {
        get_head_index(head, &self.log_output).is_some()
    }

    /// Number of heads that fit on screen. Think of this as in unit
    /// head-index. Moving the head-index this much causes a full page
    /// scroll.
    pub fn visible_heads(&self) -> isize {
        // Every item in the log list is one line, and every head spans as
        // many of them as the graph takes.
        self.list_pane.visible_items() / self.lines_per_head as isize
    }

    /// Move selection to a specific head. This may cause the next draw to
    /// scroll to a different line.
    pub fn set_head(&mut self, head: Head) {
        self.scroll_padding_active = true;
        head.clone_into(&mut self.head);
    }

    /// Move selection to a specific head, leaving the viewport where it
    /// is. Used when the selection follows the mouse, which is already
    /// pointing at the line it wants to stay on.
    pub fn set_head_in_place(&mut self, head: Head) {
        self.scroll_padding_active = false;
        head.clone_into(&mut self.head);
    }

    /// Move selection relative to the current position.
    /// The scroll is relative to head-index, not line-index.
    /// This will update self.head
    pub fn scroll_relative(&mut self, scroll: isize) {
        let log_output = match self.log_output.as_ref() {
            Ok(log_output) => log_output,
            Err(_) => return,
        };

        let heads: &Vec<Head> = log_output.heads.as_ref();

        let current_head_index = self.get_current_head_index();
        let next_head = match current_head_index {
            Some(current_head_index) => heads.get(
                current_head_index
                    .saturating_add_signed(scroll)
                    .min(heads.len() - 1),
            ),
            None => heads.first(),
        };
        if let Some(next_head) = next_head {
            self.set_head(next_head.clone());
        }
        // TODO Notify about change of head
    }

    //
    //  Marked heads
    //

    /// Mark or unmark the specified head
    pub fn set_head_mark(&mut self, head: &Head, mark: bool) {
        if mark {
            self.marked_heads.insert(head.commit_id.clone());
        } else {
            self.marked_heads.remove(&head.commit_id);
        }
    }

    /// Check if a head is marked for batch operation
    pub fn is_head_marked(&self, head: &Head) -> bool {
        self.marked_heads.contains(&head.commit_id)
    }

    /// Toggle the mark on the current head
    pub fn toggle_head_mark(&mut self) {
        let was_marked = self.is_head_marked(&self.head);
        self.set_head_mark(&self.head.clone(), !was_marked);
    }
}

impl Component for LogPanel<'_> {
    fn update(&mut self) -> Result<Option<AppAction>> {
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let log_lines = self.log_lines();
        let log_block = Block::bordered()
            .title(self.title.clone())
            .border_type(BorderType::Rounded);
        self.log_list_state.select(self.selected_log_line());
        let log = List::new(log_lines);
        let log = if self.scroll_padding_active {
            log.scroll_padding(7)
        } else {
            log
        };
        self.list_pane
            .render(f, area, log_block, log, &mut self.log_list_state);

        Ok(())
    }
}

impl PanelMouseInput for LogPanel<'_> {
    fn input_mouse(&mut self, mouse: Mouse) -> MouseInput {
        self.list_pane.input_mouse(mouse)
    }
}
