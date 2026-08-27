/*! The log panel shows the list of changes on the left side of the
log tab. */

use std::collections::HashSet;

use ansi_to_tui::IntoText;
use anyhow::Result;
use ratatui::crossterm::event::MouseEvent;
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
use crate::commander::new_commander;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::keybinds::LogTabEvent;
use crate::keybinds::LogTabKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;

/**
    A panel that displays the output of jj log.
    This panel is used on the left side of the log tab.
    It shows a selected change, which is expanded
    on the right side of the log tab.

    The log operates with two index:
    - line index (into self.log_output.text)
    - head index (into self.log_output.heads)

    The line index is used for scrolling at the display level.

    The head index is used for scrolling at the user level
    as well as for selecting which lines to highlight.
*/
pub struct LogPanel<'a> {
    /// Output from 'jj log' as provided by command::get_show_log
    log_output: Result<LogOutput, CommandError>,

    /// Output from 'jj log' converted to Ratatui Text
    log_output_text: Text<'a>,

    /// Scroll offset and cursor position
    log_list_state: ListState,

    /// The revision filter used for the log
    pub log_revset: Option<String>,

    /// Currently selected commit
    pub head: Head,

    /// Currently marked commits
    pub marked_heads: HashSet<CommitId>,

    list_pane: ListPane,

    /// Configuration of colours
    config: JjConfig,
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
    /// A panel showing `head` selected in an empty log.
    pub fn new(head: Head) -> Self {
        let mut keybinds = LogTabKeybinds::default();
        if let Some(keybinds_config) = new_commander().env.jj_config.keybinds() {
            keybinds.extend_from_config(keybinds_config);
        }

        Self {
            log_output_text: Text::default(),
            log_output: Ok(LogOutput::default()),
            log_list_state: ListState::default(),
            log_revset: new_commander().env.default_revset.clone(),

            head,
            marked_heads: HashSet::new(),

            list_pane: ListPane::default(),

            config: get_env().jj_config.clone(),
        }
    }

    //
    //  Handle jj log output
    //

    /// Run jj log and store output for display
    pub fn refresh_log_output(&mut self) {
        self.log_output = new_commander().get_log(&self.log_revset);
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
            Err(err) => err.into_text("Error getting log").unwrap().lines,
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
        // Every item in the log list is one line and every head spans two
        // of them.
        self.list_pane.visible_items() / 2
    }

    /// Move selection to a specific head. This may cause the next draw to
    /// scroll to a different line.
    pub fn set_head(&mut self, head: Head) {
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

    /// LogTabEvent: Toggle mark on the current head
    pub fn toggle_head_mark(&mut self) {
        let was_marked = self.is_head_marked(&self.head);
        self.set_head_mark(&self.head.clone(), !was_marked);
    }

    /// Extract the list of all marked heads and clear it
    pub fn extract_and_clear_head_marks(&mut self) -> Vec<CommitId> {
        self.marked_heads.drain().collect()
    }

    //
    //  Event handling
    //

    pub fn handle_event(&mut self, log_tab_event: LogTabEvent) -> Result<ComponentInputResult> {
        match log_tab_event {
            LogTabEvent::ScrollToBottom => {
                self.scroll_relative(isize::MAX);
            }
            LogTabEvent::ScrollToTop => {
                self.scroll_relative(-isize::MAX);
            }
            LogTabEvent::ToggleHeadMark => {
                self.toggle_head_mark();
            }
            _ => {
                return Ok(ComponentInputResult::NotHandled);
            }
        }
        Ok(ComponentInputResult::Handled)
    }
}

impl Component for LogPanel<'_> {
    fn update(&mut self) -> Result<Option<AppAction>> {
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let title = match &self.log_revset {
            Some(log_revset) => &format!(" Log for: {log_revset} "),
            None => " Log ",
        };

        let log_lines = self.log_lines();
        let log_block = Block::bordered()
            .title(title)
            .border_type(BorderType::Rounded);
        self.log_list_state.select(self.selected_log_line());
        let log = List::new(log_lines).scroll_padding(7);
        self.list_pane
            .render(f, area, log_block, log, &mut self.log_list_state);

        Ok(())
    }
}

impl PanelMouseInput for LogPanel<'_> {
    fn input_mouse(&mut self, mouse: MouseEvent) -> MouseInput {
        self.list_pane.input_mouse(mouse)
    }
}
