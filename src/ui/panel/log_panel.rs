/*! The log panel shows a graph on the left side of a tab: the changes of
the log, or the entries of the operation log. */

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
use crate::commander::log::LogItem;
use crate::commander::log::LogOutput;
use crate::env::get_env;
use crate::event::Mouse;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::utils::error_text;

/**
    A panel that displays a graph of items. This panel is used on the
    left side of a tab. It shows a selected item, which is expanded on
    the right side of the tab.

    The log operates with two index:
    - line index (into self.log_output.text)
    - item index (into self.log_output.items)

    The line index is used for scrolling at the display level.

    The item index is used for scrolling at the user level
    as well as for selecting which lines to highlight.
*/
pub struct LogPanel<'a, T: LogItem> {
    /// The log to show, or what went wrong reading it
    log_output: Result<LogOutput<T>, CommandError>,

    /// The log converted to Ratatui Text
    log_output_text: Text<'a>,

    /// The title the log is shown under
    title: String,

    /// How many lines of the graph one item takes
    lines_per_item: usize,

    /// Scroll offset and cursor position
    log_list_state: ListState,

    /// Currently selected item
    pub selected: T,

    /// Currently marked items
    pub marked: HashSet<T::Mark>,

    /// Whether the next operation acts on the marked commits rather
    /// than on the selected one
    pub use_marks: bool,

    list_pane: ListPane,

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

fn get_item_index<T: LogItem>(
    item: &T,
    log_output: &Result<LogOutput<T>, CommandError>,
) -> Option<usize> {
    match log_output {
        Ok(log_output) => log_output
            .items
            .iter()
            .position(|items| items == item)
            .or_else(|| {
                log_output
                    .items
                    .iter()
                    .position(|other| other.same_subject(item))
            }),
        Err(_) => None,
    }
}

impl<'a, T: LogItem> LogPanel<'a, T> {
    /// A panel showing `selected` in an empty log. The graphs it is later
    /// given take `lines_per_item` lines per item.
    pub fn new(selected: T, lines_per_item: usize) -> Self {
        Self {
            log_output_text: Text::default(),
            log_output: Ok(LogOutput::default()),
            title: String::new(),
            lines_per_item,
            log_list_state: ListState::default(),

            selected,
            marked: HashSet::new(),
            use_marks: false,

            list_pane: ListPane::default(),

            scroll_padding_active: true,
        }
    }

    //
    //  Handle jj log output
    //

    /// Replace what the panel shows. An error takes the place of the
    /// graph.
    pub fn show(&mut self, log_output: Result<LogOutput<T>, CommandError>, title: String) {
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
    fn output_to_lines(&self, log_output: &LogOutput<T>) -> Vec<Line<'a>> {
        // Add commit mark
        let add_mark = |line: &mut Line, i: usize| {
            let at_marked_commit = log_output
                .item_at(i)
                .is_some_and(|item| self.is_item_marked(item));

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

        let highlight = get_env().jj_config.highlight_color();

        self.log_output_text
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let mut line = line.to_owned();

                // Add padding at start
                add_mark(&mut line, i);

                // Highlight lines that correspond to self.selected
                if log_output.item_at(i) == Some(&self.selected) {
                    set_bg(&mut line, highlight);
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

    /// Get a list of all items in log list
    pub fn items(&self) -> Vec<T> {
        match self.log_output.as_ref() {
            Ok(log_output) => log_output.items.clone(),
            Err(_) => vec![],
        }
    }

    //
    //  Selected item and the special item index
    //

    /// Find the line in self.log_output that match self.selected
    fn selected_log_line(&self) -> Option<usize> {
        let log_output = self.log_output.as_ref().ok()?;

        log_output
            .graph_items
            .iter()
            .position(|opt_h| opt_h.as_ref().is_some_and(|h| h == &self.selected))
    }

    /// Where to put something that wants to show up next to the selected
    /// change.
    pub fn selected_position(&self) -> Option<Position> {
        self.list_pane
            .item_anchor(self.selected_log_line()?, self.lines_per_item as u16)
    }

    /// Find item of the provided log_output line
    pub fn item_at_log_line(&self, log_line: usize) -> Option<T> {
        self.log_output.as_ref().ok()?.item_at(log_line).cloned()
    }

    // Return the item-index for the selection
    fn current_item_index(&self) -> Option<usize> {
        get_item_index(&self.selected, &self.log_output)
    }

    /// Whether the log holds an item, taking another version of the same
    /// subject as a match.
    pub fn shows_item(&self, item: &T) -> bool {
        get_item_index(item, &self.log_output).is_some()
    }

    /// Number of items that fit on screen. Think of this as in unit
    /// item-index. Moving the item-index this much causes a full page
    /// scroll.
    pub fn visible_items(&self) -> isize {
        // Every entry in the log list is one line, and every item spans as
        // many of them as the graph takes.
        self.list_pane.visible_items() / self.lines_per_item as isize
    }

    /// Move selection to a specific item. This may cause the next draw to
    /// scroll to a different line.
    pub fn set_selected(&mut self, item: T) {
        self.scroll_padding_active = true;
        item.clone_into(&mut self.selected);
    }

    /// Move selection to a specific item, leaving the viewport where it
    /// is. Used when the selection follows the mouse, which is already
    /// pointing at the line it wants to stay on.
    pub fn set_selected_in_place(&mut self, item: T) {
        self.scroll_padding_active = false;
        item.clone_into(&mut self.selected);
    }

    /// Move selection relative to the current position.
    /// The scroll is relative to item-index, not line-index.
    /// This will update self.selected
    pub fn scroll_relative(&mut self, scroll: isize) {
        let log_output = match self.log_output.as_ref() {
            Ok(log_output) => log_output,
            Err(_) => return,
        };

        let items: &Vec<T> = log_output.items.as_ref();

        let current_item_index = self.current_item_index();
        let next_item = match current_item_index {
            Some(current_item_index) => items.get(
                current_item_index
                    .saturating_add_signed(scroll)
                    .min(items.len() - 1),
            ),
            None => items.first(),
        };
        if let Some(next_item) = next_item {
            self.set_selected(next_item.clone());
        }
        // TODO Notify about change of selection
    }

    //
    //  Marked items
    //

    /// Mark or unmark the specified item
    pub fn set_item_mark(&mut self, item: &T, mark: bool) {
        if mark {
            self.marked.insert(item.mark());
        } else {
            self.marked.remove(&item.mark());
        }
    }

    /// Check if an item is marked for batch operation
    pub fn is_item_marked(&self, item: &T) -> bool {
        self.marked.contains(&item.mark())
    }

    /// Toggle the mark on the selected item
    pub fn toggle_item_mark(&mut self) {
        let was_marked = self.is_item_marked(&self.selected);
        self.set_item_mark(&self.selected.clone(), !was_marked);
    }
}

impl<T: LogItem> Component for LogPanel<'_, T> {
    fn update(&mut self) -> Result<Option<AppAction>> {
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let log_lines = self.log_lines();
        let mut log_block = Block::bordered()
            .title(self.title.clone())
            .border_type(BorderType::Rounded);
        if self.use_marks {
            log_block = log_block
                .title_top(Line::styled(" marked ", Style::new().bold().yellow()).right_aligned());
        }
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

impl<T: LogItem> PanelMouseInput for LogPanel<'_, T> {
    fn input_mouse(&mut self, mouse: Mouse) -> MouseInput {
        self.list_pane.input_mouse(mouse)
    }
}
