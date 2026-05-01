/*! The log panel shows a graph on the left side of a tab: the changes of
the log, or the entries of the operation log. */

use std::collections::HashSet;
use std::time::Duration;
use std::time::Instant;

use ansi_to_tui::IntoText;
use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::crossterm::event::KeyModifiers;
use ratatui::crossterm::event::ModifierKeyCode;
use ratatui::crossterm::event::MouseButton;
use ratatui::crossterm::event::MouseEventKind;
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
use crate::ui::ComponentInputResult;
use crate::ui::utils::error_text;

/// Heads to keep between the selected one and the edge of the pane.
const SCROLL_MARGIN_HEADS: usize = 3;

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

    /// Whether a press starts a drag that moves items about, rather
    /// than only picking what it hits
    draggable: bool,

    /// In-flight drag, if any
    drag: Option<DragState<T>>,

    /// Drop result, ready for the surrounding tab to consume
    pending_action: Option<DragAction<T>>,

    /// Item the last press hit. A press on another one is a fresh click,
    /// however soon it followed, since selecting can scroll the list.
    pressed: Option<T>,

    list_pane: ListPane,

    /// Whether the next draw scrolls the selection into view.
    /// Disabled after a right-click so the viewport doesn't jump.
    scroll_padding_active: bool,
}
/// Tracks an in-flight mouse drag started inside the log panel.
struct DragState<T: LogItem> {
    /// Marks of the source items, resolved at MouseDown.
    source_marks: Vec<T::Mark>,
    /// Item where the drag started — kept around so the post-drop
    /// dispatcher can re-select the dragged item after the operation has
    /// rewritten it.
    source_item: T,
    /// Display line of the item where the drag started.
    source_line: usize,
    /// Display line currently under the mouse cursor.
    cursor_line: Option<usize>,
    /// Item currently under the cursor (resolved on Drag).
    target_item: Option<T>,
    /// True once the cursor has crossed onto a different row, so we can
    /// distinguish a click from a real drag on Up.
    has_moved: bool,
    /// Selection at the moment the drag began. Auto-scroll-at-edge moves
    /// `self.selected` so the view follows; this snapshot is restored if
    /// the drag is cancelled or drops onto its own source.
    selection_at_start: T,
    /// Screen row of the most recent Drag event. Drives the tick-based
    /// auto-scroll that keeps the view moving while the cursor is held
    /// at the edge without wiggling.
    last_row: u16,
    /// Last tick at which the auto-scroll fired, so we can rate-limit
    /// the tick path independently from event-driven scrolls.
    last_tick_scroll_at: Option<Instant>,
    /// Modifier keys reported on the latest mouse event of this drag.
    /// Updates on every Drag event so the UI can preview the action.
    modifiers: KeyModifiers,
}

/// Operation a drop should perform, derived from the modifiers held at
/// release. Shift wins over Ctrl, which wins over Alt; bare drop is a
/// plain rebase onto. Some terminals report Alt as META, so we accept
/// either.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DragMode {
    /// `jj rebase ... -d <target>` (default)
    Onto,
    /// `jj rebase ... -A <target>`
    After,
    /// `jj rebase ... -B <target>`
    Before,
    /// `jj squash --from ... --into <target>`
    Squash,
}

fn modifier_code_to_flag(code: ModifierKeyCode) -> KeyModifiers {
    match code {
        ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift => KeyModifiers::SHIFT,
        ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl => KeyModifiers::CONTROL,
        ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt => KeyModifiers::ALT,
        ModifierKeyCode::LeftMeta
        | ModifierKeyCode::RightMeta
        | ModifierKeyCode::LeftSuper
        | ModifierKeyCode::RightSuper => KeyModifiers::META,
        _ => KeyModifiers::empty(),
    }
}

pub fn decode_drag_modifiers(modifiers: KeyModifiers) -> DragMode {
    if modifiers.contains(KeyModifiers::SHIFT) {
        DragMode::Squash
    } else if modifiers.contains(KeyModifiers::CONTROL) {
        DragMode::Before
    } else if modifiers.intersects(KeyModifiers::ALT | KeyModifiers::META) {
        DragMode::After
    } else {
        DragMode::Onto
    }
}

/// A drop the surrounding tab has yet to act on, taken with
/// `take_pending_drag_action`.
pub struct DragAction<T: LogItem> {
    /// What the modifiers held at release asked for.
    pub mode: DragMode,
    /// Marks of the items dragged.
    pub source_marks: Vec<T::Mark>,
    /// The item the drag started on, which the tab can look up again
    /// once the operation has rewritten it.
    pub source_item: T,
    /// The item they were dropped on.
    pub target: T,
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
            draggable: false,
            drag: None,
            pending_action: None,
            pressed: None,

            list_pane: ListPane::default(),

            scroll_padding_active: true,
        }
    }

    /// The same panel, with a press on an item starting a drag that
    /// moves it about. The drop it produces is for the surrounding tab
    /// to carry out, through `take_pending_drag_action`.
    pub fn draggable(mut self) -> Self {
        self.draggable = true;
        self
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

        let config = get_env().jj_config.clone();

        let drag_target_item = self.drag.as_ref().and_then(|d| d.target_item.as_ref());
        let drag_source_item =
            log_output.item_at(self.drag.as_ref().map_or(usize::MAX, |d| d.source_line));

        self.log_output_text
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let mut line = line.to_owned();
                let item_here = log_output.item_at(i);

                // Add padding at start
                add_mark(&mut line, i);

                // While a drag is in flight, only paint the source/target
                // pair — the regular selection highlight would add a third,
                // unrelated color and only distracts from the drop decision.
                // Target wins over source if a drag is dropped onto its own
                // source row (no-op case).
                if drag_target_item.is_some() && item_here == drag_target_item {
                    set_bg(&mut line, config.drag_target_color());
                } else if drag_source_item.is_some() && item_here == drag_source_item {
                    set_bg(&mut line, config.drag_source_color());
                } else if self.drag.is_none() && item_here == Some(&self.selected) {
                    set_bg(&mut line, config.highlight_color());
                }

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

    /// Scroll the view so the selected head keeps a margin to both edges.
    ///
    /// The List widget's own `scroll_padding` counts list items, which are
    /// single lines here, so it happily leaves the offset in the middle of a
    /// head and renders every entry split across the pane edge. Owning the
    /// offset lets us keep it on a head boundary.
    fn scroll_selection_into_view(&mut self, visible: usize) {
        let Some(line) = self.selected_log_line() else {
            return;
        };
        let head_lines = self.lines_per_item;
        if visible <= head_lines {
            return;
        }

        // On a short pane the two rules below would fight, so never ask for
        // more margin than half of what is left after the head itself.
        let margin = (SCROLL_MARGIN_HEADS * head_lines).min((visible - head_lines) / 2);

        let mut offset = self.log_list_state.offset();
        if line < offset + margin {
            offset = line.saturating_sub(margin);
        } else if line + head_lines + margin > offset + visible {
            offset = (line + head_lines + margin).saturating_sub(visible);
        }

        let max_offset = self.log_output_text.lines.len().saturating_sub(visible);
        offset = offset.min(max_offset);

        // Align to a head boundary, unless the pane height leaves the last
        // head straddling the bottom edge, where alignment would clip it.
        let aligned = offset - offset % head_lines;
        *self.log_list_state.offset_mut() = if line + head_lines > aligned + visible {
            offset
        } else {
            aligned
        };
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
        if self.scroll_padding_active {
            self.scroll_selection_into_view(log_block.inner(area).height as usize);
        }
        self.log_list_state.select(self.selected_log_line());
        let log = List::new(log_lines);
        self.list_pane
            .render(f, area, log_block, log, &mut self.log_list_state);

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key_event) = event
            && let Some(drag) = self.drag.as_mut()
            && let KeyCode::Modifier(mod_code) = key_event.code
        {
            let flag = modifier_code_to_flag(mod_code);
            match key_event.kind {
                KeyEventKind::Press => drag.modifiers |= flag,
                KeyEventKind::Release => drag.modifiers &= !flag,
                KeyEventKind::Repeat => {}
            }
            return Ok(ComponentInputResult::Handled);
        }

        Ok(ComponentInputResult::NotHandled)
    }
}

impl<T: LogItem> LogPanel<'_, T> {
    /// Map a mouse event to a log line index, if it points inside the list.
    fn line_under_mouse(&self, pos: Position) -> Option<usize> {
        self.list_pane.item_at(pos)
    }

    /// True when a drag started inside this panel is still in progress.
    pub fn drag_active(&self) -> bool {
        self.drag.is_some()
    }

    /// Marks of the items the in-flight drag started on, if any.
    pub fn drag_source_marks(&self) -> Option<&[T::Mark]> {
        self.drag.as_ref().map(|d| d.source_marks.as_slice())
    }

    /// Item currently under the cursor for the in-flight drag, if any.
    pub fn drag_target_item(&self) -> Option<&T> {
        self.drag.as_ref().and_then(|d| d.target_item.as_ref())
    }

    /// Modifier keys reported on the most recent mouse event of the
    /// in-flight drag, if any. Used to preview the action that release
    /// would trigger.
    pub fn drag_modifiers(&self) -> Option<KeyModifiers> {
        self.drag.as_ref().map(|d| d.modifiers)
    }

    /// Item the current drag started on, if any.
    pub fn drag_source_item(&self) -> Option<T> {
        let drag = self.drag.as_ref()?;
        self.log_output
            .as_ref()
            .ok()?
            .item_at(drag.source_line)
            .cloned()
    }

    /// True once the cursor has crossed onto a different head during the
    /// drag (used by callers to decide whether to render drag UI).
    pub fn drag_has_moved(&self) -> bool {
        self.drag.as_ref().is_some_and(|d| d.has_moved)
    }

    /// Drive auto-scroll while the cursor is held at the top/bottom edge
    /// of the log pane without moving. Called on a steady tick from the
    /// main loop so the view keeps advancing when no Drag events arrive.
    pub fn tick_drag_auto_scroll(&mut self) {
        let Some(drag) = self.drag.as_ref() else {
            return;
        };
        // Same gating as the event-driven path: don't auto-scroll until
        // the cursor has actually moved off the source row.
        let cursor_moved =
            drag.has_moved || drag.cursor_line.is_some_and(|c| c != drag.source_line);
        if !cursor_moved {
            return;
        }
        // Rate-limit the tick path so the scroll feels deliberate, not
        // racing. Event-driven scrolls (from cursor movement) bypass
        // this — they reset `last_tick_scroll_at` to None on every Drag.
        const TICK_INTERVAL: Duration = Duration::from_millis(150);
        if let Some(at) = drag.last_tick_scroll_at
            && at.elapsed() < TICK_INTERVAL
        {
            return;
        }
        let row = drag.last_row;
        let content = self.list_pane.content_rect();
        let log_top = content.y;
        let log_bottom = content.bottom();
        let direction = if row <= log_top {
            -1
        } else if row + 1 >= log_bottom {
            1
        } else {
            return;
        };
        self.scroll_relative(direction);
        if let Some(drag) = self.drag.as_mut() {
            drag.last_tick_scroll_at = Some(Instant::now());
        }
    }

    /// Cancel any in-flight drag without producing an action.
    pub fn cancel_drag(&mut self) {
        if let Some(drag) = self.drag.take() {
            // Restore the selection auto-scroll-at-edge moved during the drag.
            self.selected = drag.selection_at_start;
        }
        self.pending_action = None;
    }

    /// Take the pending drop action, if any.
    pub fn take_pending_drag_action(&mut self) -> Option<DragAction<T>> {
        self.pending_action.take()
    }
}

impl<T: LogItem> PanelMouseInput for LogPanel<'_, T> {
    fn input_mouse(&mut self, mouse: Mouse) -> MouseInput {
        if !self.draggable {
            return self.list_pane.input_mouse(mouse);
        }

        let mouse_pos = mouse.position();
        if !self.list_pane.contains(mouse_pos) {
            // A drag that wandered out of the panel before release is
            // also out-of-bounds for our purposes; abandon it.
            if matches!(mouse.kind(), MouseEventKind::Up(_))
                && let Some(drag) = self.drag.take()
            {
                self.selected = drag.selection_at_start;
            }
            return MouseInput::NotHandled;
        }

        // Execute command dependent on panel and event kind
        match mouse.kind() {
            MouseEventKind::ScrollUp => return MouseInput::Scroll(-1),
            MouseEventKind::ScrollDown => return MouseInput::Scroll(1),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(inx) = self.line_under_mouse(mouse_pos)
                    && let Some(item) = self.item_at_log_line(inx)
                {
                    let double = mouse.clicks() == 2 && self.pressed.as_ref() == Some(&item);
                    self.pressed = Some(item.clone());
                    // The second press of a double click acts on the item
                    // the first one selected instead of dragging it.
                    if double {
                        return MouseInput::Activate;
                    }
                    let source_marks = if self.is_item_marked(&item) {
                        self.marked.iter().cloned().collect()
                    } else {
                        vec![item.mark()]
                    };
                    self.drag = Some(DragState {
                        source_marks,
                        source_item: item.clone(),
                        source_line: inx,
                        cursor_line: Some(inx),
                        target_item: Some(item),
                        has_moved: false,
                        selection_at_start: self.selected.clone(),
                        last_row: mouse_pos.y,
                        last_tick_scroll_at: None,
                        modifiers: mouse.modifiers(),
                    });
                    return MouseInput::Handled;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(drag) = self.drag.as_ref() {
                    // Auto-scroll when the cursor reaches the top or
                    // bottom of the log pane. Gated on the cursor
                    // having actually moved off the source row, so
                    // the synthetic Drag some terminals emit at the
                    // click coordinates can't trigger a scroll.
                    // scroll_relative re-anchors the offset so the
                    // selection follows along instead of fighting
                    // the scroll the pane would otherwise keep.
                    let cursor_moved =
                        drag.has_moved || drag.cursor_line.is_some_and(|c| c != drag.source_line);
                    if cursor_moved {
                        let row = mouse_pos.y;
                        let content = self.list_pane.content_rect();
                        if row <= content.y {
                            self.scroll_relative(-1);
                        } else if row + 1 >= content.bottom() {
                            self.scroll_relative(1);
                        }
                    }

                    let inx = self.line_under_mouse(mouse_pos);
                    let new_target = inx.and_then(|i| {
                        self.log_output
                            .as_ref()
                            .ok()
                            .and_then(|out| out.item_at(i).cloned())
                    });
                    let source_item = self.drag.as_ref().and_then(|d| {
                        self.log_output
                            .as_ref()
                            .ok()
                            .and_then(|out| out.item_at(d.source_line).cloned())
                    });
                    let drag = self.drag.as_mut().expect("checked above");
                    // Track modifiers on every drag tick so the footer
                    // can preview the action that release would trigger.
                    drag.modifiers = mouse.modifiers();
                    drag.last_row = mouse_pos.y;
                    drag.last_tick_scroll_at = None;
                    if inx != drag.cursor_line {
                        drag.cursor_line = inx;
                        drag.target_item = new_target;
                        // The drag only counts as "moved" once the cursor
                        // has crossed onto a different head, not just a
                        // different line within the same head.
                        if drag.target_item != source_item {
                            drag.has_moved = true;
                        }
                    }
                    return MouseInput::Handled;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(drag) = self.drag.take() {
                    // Auto-scroll-at-edge moved self.selected while the
                    // drag was in flight; reset to the pre-drag
                    // selection. Click-to-select and successful drop
                    // dispatchers will overwrite this if needed.
                    self.selected = drag.selection_at_start.clone();

                    // No real movement: behave as a click-to-select on the
                    // up position.
                    if !drag.has_moved {
                        return match self.line_under_mouse(mouse_pos) {
                            Some(inx) => MouseInput::Select(inx),
                            None => MouseInput::Handled,
                        };
                    }

                    // Genuine drop: build the action and let LogTab pick it up.
                    if let Some(target) = drag.target_item.clone() {
                        // Dropping an item on itself (or on a member of
                        // a multi-source set) is a no-op.
                        if drag.source_marks.contains(&target.mark()) {
                            return MouseInput::Handled;
                        }
                        // Some terminals strip modifiers from the Up event
                        // even when they pass them through on Drag events,
                        // so fall back to whatever the drag last saw.
                        let raw = if mouse.modifiers().is_empty() {
                            drag.modifiers
                        } else {
                            mouse.modifiers()
                        };
                        let action = DragAction {
                            mode: decode_drag_modifiers(raw),
                            source_marks: drag.source_marks,
                            source_item: drag.source_item,
                            target,
                        };
                        self.pending_action = Some(action);
                    }
                    return MouseInput::Handled;
                }

                // Fallback: legacy click-to-select behaviour.
                if let Some(inx) = self.line_under_mouse(mouse_pos) {
                    return MouseInput::Select(inx);
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                // A right press asks for the context menu, unless a
                // left drag is in flight, which it must not disturb.
                if self.drag.is_none()
                    && let Some(inx) = self.line_under_mouse(mouse_pos)
                {
                    return MouseInput::Context(inx);
                }
            }
            MouseEventKind::Up(_) => {
                // A release of any other button while a left drag is in
                // flight must not end it, and there is nothing else to
                // do for one.
            }
            _ => {}
        }

        MouseInput::NotHandled
    }
}
