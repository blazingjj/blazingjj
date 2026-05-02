/*! The log panel shows the list of changes on the left side of the
log tab. */

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
use ratatui::crossterm::event::MouseEvent;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::text::ToText;
use ratatui::widgets::*;

use crate::commander::CommandError;
use crate::commander::ids::CommitId;
use crate::commander::log::Head;
use crate::commander::log::LogOutput;
use crate::commander::new_commander;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::keybinds::LogTabEvent;
use crate::keybinds::LogTabKeybinds;
use crate::ui::Component;
use crate::ui::ComponentAction;
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

    /// Area were log content was drawn. This excludes the border.
    pub log_rect: Rect,

    /// The revision filter used for the log
    pub log_revset: Option<String>,

    /// Currently selected commit
    pub head: Head,

    /// Currently marked commits
    pub marked_heads: HashSet<CommitId>,

    /// In-flight drag, if any
    drag: Option<DragState>,

    /// Drop result, ready for the LogTab to consume
    pending_action: Option<DragAction>,

    /// Area where panel was drawn. This includes the border.
    panel_rect: Rect,

    /// Configuration of colours
    config: JjConfig,
}

/// Tracks an in-flight mouse drag started inside the log panel.
struct DragState {
    /// Source commits resolved at MouseDown.
    source_revs: Vec<CommitId>,
    /// Head where the drag started — kept around so the post-drop
    /// dispatcher can re-select the dragged commit by change_id after a
    /// rebase has rewritten its commit_id.
    source_head: Head,
    /// Display line of the head where the drag started.
    source_line: usize,
    /// Display line currently under the mouse cursor.
    cursor_line: Option<usize>,
    /// Head currently under the cursor (resolved on Drag).
    target_head: Option<Head>,
    /// True once the cursor has crossed onto a different row, so we can
    /// distinguish a click from a real drag on Up.
    has_moved: bool,
    /// Selection at the moment the drag began. Auto-scroll-at-edge moves
    /// `self.head` so the view follows; this snapshot is restored if the
    /// drag is cancelled or drops onto its own source.
    selection_at_start: Head,
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

/// A drop produced one of these actions for the surrounding LogTab to
/// dispatch. Cleared by `take_pending_drag_action`.
///
/// Rebase rewrites the dragged commit's commit_id, so the rebase variants
/// also carry `source_head`: the dispatcher looks the change up by
/// change_id afterwards to re-select it. Squash doesn't need this — the
/// source is left intact (just emptied) and we keep the existing
/// selection behaviour.
pub enum DragAction {
    RebaseOnto {
        source_revs: Vec<CommitId>,
        source_head: Head,
        target: Head,
    },
    Squash {
        source_revs: Vec<CommitId>,
        target: Head,
    },
    RebaseAfter {
        source_revs: Vec<CommitId>,
        source_head: Head,
        target: Head,
    },
    RebaseBefore {
        source_revs: Vec<CommitId>,
        source_head: Head,
        target: Head,
    },
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
    pub fn new() -> Result<Self> {
        let log_revset = new_commander().env.default_revset.clone();
        let log_output = new_commander().get_log(&log_revset);
        let head = new_commander().get_current_head()?;

        let log_list_state = ListState::default().with_selected(get_head_index(&head, &log_output));

        let mut keybinds = LogTabKeybinds::default();
        if let Some(keybinds_config) = new_commander().env.jj_config.keybinds() {
            keybinds.extend_from_config(keybinds_config);
        }

        let log_output_text = match log_output.as_ref() {
            Ok(log_output) => log_output
                .graph
                .into_text()
                .unwrap_or(Text::from("Could not turn text into TUI text (coloring)")),
            Err(_) => Text::default(),
        };

        Ok(Self {
            log_output_text,
            log_output,
            log_list_state,
            log_rect: Rect::ZERO,

            log_revset,

            head,
            marked_heads: HashSet::new(),

            drag: None,
            pending_action: None,

            panel_rect: Rect::ZERO,

            config: get_env().jj_config.clone(),
        })
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

        fn add_underline(line: &mut Line, color: Color, bg: Color) {
            line.style = line
                .style
                .add_modifier(Modifier::UNDERLINED)
                .underline_color(color)
                .bg(bg);
            for span in line.spans.iter_mut() {
                span.style = span
                    .style
                    .add_modifier(Modifier::UNDERLINED)
                    .underline_color(color)
                    .bg(bg);
            }
        }

        let drag_target_head = self.drag.as_ref().and_then(|d| d.target_head.as_ref());
        let drag_source_head =
            log_output.head_at(self.drag.as_ref().map_or(usize::MAX, |d| d.source_line));

        // For Before/After modes, find the head whose last display line gets
        // an underline to show the insertion point:
        //   Before (-B): source becomes parent of target → lands below target
        //                → underline last line of target
        //   After  (-A): source becomes child of target → lands above target
        //                → underline last line of the head above target
        let drag_mode = self
            .drag
            .as_ref()
            .map(|d| decode_drag_modifiers(d.modifiers));
        let underline_head: Option<&Head> = drag_target_head.and_then(|target| match drag_mode? {
            DragMode::Before => Some(target),
            DragMode::After => {
                let idx = log_output.heads.iter().position(|h| h == target)?;
                log_output.heads.get(idx.checked_sub(1)?)
            }
            _ => None,
        });

        self.log_output_text
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let mut line = line.to_owned();
                let head_here = log_output.head_at(i);

                // Add padding at start
                add_mark(&mut line, i);

                // While a drag is in flight, only paint the source/target
                // pair — the regular selection highlight would add a third,
                // unrelated color and only distracts from the drop decision.
                // Target wins over source if a drag is dropped onto its own
                // source row (no-op case).
                // Onto/Squash: target gets a strong "destination" highlight.
                // Before/After: target gets a muted "reference commit" tint;
                // the insertion-line separator carries the directional meaning.
                if drag_target_head.is_some() && head_here == drag_target_head {
                    let color = if underline_head.is_some() {
                        self.config.drag_insert_target_color()
                    } else {
                        self.config.drag_target_color()
                    };
                    set_bg(&mut line, color);
                } else if drag_source_head.is_some() && head_here == drag_source_head {
                    set_bg(&mut line, self.config.drag_source_color());
                } else if self.drag.is_none() && head_here == Some(&self.head) {
                    set_bg(&mut line, self.config.highlight_color());
                }

                // Underline the last display line of the insertion-point head
                // so the separator appears as a bottom border on that commit.
                if underline_head.is_some_and(|ul| head_here == Some(ul))
                    && log_output.head_at(i + 1) != underline_head
                {
                    add_underline(
                        &mut line,
                        self.config.drag_insert_color(),
                        self.config.drag_insert_bg_color(),
                    );
                }

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
    fn head_at_log_line(&mut self, log_line: usize) -> Option<Head> {
        self.log_output.as_ref().ok()?.head_at(log_line).cloned()
    }

    // Return the head-index for the selection
    fn get_current_head_index(&self) -> Option<usize> {
        get_head_index(&self.head, &self.log_output)
    }

    /// Number of log list items that fit on screen. Think of this as
    /// in unit head-index. Moving the head-index this much causes a
    /// full page scroll.
    fn visible_heads(&self) -> u16 {
        // Every item in the log list is 2 lines high, so divide screen rows
        // by 2 to get the number of log items that fit in it.
        self.log_rect.height / 2
    }

    /// Move selection to a specific head. This may cause the next draw to
    /// scroll to a different line.
    pub fn set_head(&mut self, head: Head) {
        head.clone_into(&mut self.head);
    }

    /// Move selection relative to the current position.
    /// The scroll is relative to head-index, not line-index.
    /// This will update self.head
    fn scroll_relative(&mut self, scroll: isize) {
        let log_output = match self.log_output.as_ref() {
            Ok(log_output) => log_output,
            Err(_) => return,
        };

        let heads: &Vec<Head> = log_output.heads.as_ref();

        // Remember the head's current screen row so we can re-anchor the
        // offset after the move. Without this, the List widget's
        // selection-driven offset only shifts once the new selection
        // enters the scroll_padding zone — meaning the first several
        // scroll events appear to do nothing.
        let old_offset = self.log_list_state.offset();
        let old_screen_row = self
            .selected_log_line()
            .map(|l| l.saturating_sub(old_offset));

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

        // Re-anchor the offset so the new head sits at the same screen
        // row as the old one. Each scroll event therefore moves the view
        // by exactly the line-height of one head, instead of waiting
        // for the selection to drift into the padding zone. The List
        // widget still clamps the offset at the actual bounds.
        if let (Some(row), Some(new_line)) = (old_screen_row, self.selected_log_line()) {
            let total_lines = self.log_output_text.lines.len();
            let visible = self.log_rect.height as usize;
            let max_offset = total_lines.saturating_sub(visible);
            let new_offset = new_line.saturating_sub(row).min(max_offset);
            *self.log_list_state.offset_mut() = new_offset;
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
            LogTabEvent::ScrollDown => {
                self.scroll_relative(1);
            }
            LogTabEvent::ScrollUp => {
                self.scroll_relative(-1);
            }
            LogTabEvent::ScrollDownHalf => {
                self.scroll_relative(self.visible_heads() as isize / 2);
            }
            LogTabEvent::ScrollUpHalf => {
                self.scroll_relative((self.visible_heads() as isize / 2).saturating_neg());
            }
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
    // Called when switching to tab
    fn focus(&mut self) -> Result<()> {
        Ok(())
    }

    fn update(&mut self) -> Result<Option<ComponentAction>> {
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        self.panel_rect = area;

        let title = match &self.log_revset {
            Some(log_revset) => &format!(" Log for: {log_revset} "),
            None => " Log ",
        };

        let log_lines = self.log_lines();
        let log_length: usize = log_lines.len();
        let log_block = Block::bordered()
            .title(title)
            .border_type(BorderType::Rounded);
        self.log_rect = log_block.inner(area);
        self.log_list_state.select(self.selected_log_line());
        let log = List::new(log_lines).block(log_block).scroll_padding(7);
        f.render_stateful_widget(log, area, &mut self.log_list_state);

        // Show scrollbar if lines don't fit the screen height
        if log_length > self.log_rect.height.into() {
            let index = self.log_list_state.selected().unwrap_or(0);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            let mut scrollbar_state = ScrollbarState::default()
                .content_length(log_length)
                .position(index);

            f.render_stateful_widget(
                scrollbar,
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Mouse(mouse_event) = event {
            // Determine if mouse event is inside log-view
            let mouse_pos = Position::new(mouse_event.column, mouse_event.row);
            if !self.panel_rect.contains(mouse_pos) {
                // A drag that wandered out of the panel before release is
                // also out-of-bounds for our purposes; abandon it.
                if matches!(mouse_event.kind, MouseEventKind::Up(_))
                    && let Some(drag) = self.drag.take()
                {
                    self.head = drag.selection_at_start;
                }
                return Ok(ComponentInputResult::NotHandled);
            }

            // Execute command dependent on panel and event kind
            match mouse_event.kind {
                MouseEventKind::ScrollUp => {
                    self.handle_event(LogTabEvent::ScrollUp)?;
                    return Ok(ComponentInputResult::Handled);
                }
                MouseEventKind::ScrollDown => {
                    self.handle_event(LogTabEvent::ScrollDown)?;
                    return Ok(ComponentInputResult::Handled);
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(inx) = self.line_under_mouse(&mouse_event)
                        && let Some(head) = self.head_at_log_line(inx)
                    {
                        let source_revs = if self.is_head_marked(&head) {
                            self.marked_heads.iter().cloned().collect()
                        } else {
                            vec![head.commit_id.clone()]
                        };
                        self.drag = Some(DragState {
                            source_revs,
                            source_head: head.clone(),
                            source_line: inx,
                            cursor_line: Some(inx),
                            target_head: Some(head),
                            has_moved: false,
                            selection_at_start: self.head.clone(),
                            last_row: mouse_event.row,
                            last_tick_scroll_at: None,
                            modifiers: mouse_event.modifiers,
                        });
                        return Ok(ComponentInputResult::Handled);
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
                        // the List widget's scroll_padding.
                        let cursor_moved = drag.has_moved
                            || drag.cursor_line.is_some_and(|c| c != drag.source_line);
                        if cursor_moved {
                            let row = mouse_event.row;
                            let log_top = self.log_rect.y;
                            let log_bottom = self.log_rect.bottom();
                            if row <= log_top {
                                self.scroll_relative(-1);
                            } else if row + 1 >= log_bottom {
                                self.scroll_relative(1);
                            }
                        }

                        let inx = self.line_under_mouse(&mouse_event);
                        let new_target = inx.and_then(|i| {
                            self.log_output
                                .as_ref()
                                .ok()
                                .and_then(|out| out.head_at(i).cloned())
                        });
                        let source_head = self.drag.as_ref().and_then(|d| {
                            self.log_output
                                .as_ref()
                                .ok()
                                .and_then(|out| out.head_at(d.source_line).cloned())
                        });
                        let drag = self.drag.as_mut().expect("checked above");
                        // Track modifiers on every drag tick so the footer
                        // can preview the action that release would trigger.
                        drag.modifiers = mouse_event.modifiers;
                        drag.last_row = mouse_event.row;
                        drag.last_tick_scroll_at = None;
                        if inx != drag.cursor_line {
                            drag.cursor_line = inx;
                            drag.target_head = new_target;
                            // The drag only counts as "moved" once the cursor
                            // has crossed onto a different head, not just a
                            // different line within the same head.
                            if drag.target_head != source_head {
                                drag.has_moved = true;
                            }
                        }
                        return Ok(ComponentInputResult::Handled);
                    }
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    if let Some(drag) = self.drag.take() {
                        // Auto-scroll-at-edge moved self.head while the
                        // drag was in flight; reset to the pre-drag
                        // selection. Click-to-select and successful drop
                        // dispatchers will overwrite this if needed.
                        self.head = drag.selection_at_start.clone();

                        // No real movement: behave as a click-to-select on the
                        // up position.
                        if !drag.has_moved {
                            if let Some(inx) = self.line_under_mouse(&mouse_event)
                                && let Some(head) = self.head_at_log_line(inx)
                            {
                                self.set_head(head);
                                return Ok(ComponentInputResult::Handled);
                            }
                            return Ok(ComponentInputResult::Handled);
                        }

                        // Genuine drop: build the action and let LogTab pick it up.
                        if let Some(target) = drag.target_head.clone() {
                            // Dropping a commit on itself (or on a member of
                            // a multi-source set) is a no-op.
                            if drag.source_revs.iter().any(|c| c == &target.commit_id) {
                                return Ok(ComponentInputResult::Handled);
                            }
                            // Some terminals strip modifiers from the Up event
                            // even when they pass them through on Drag events,
                            // so fall back to whatever the drag last saw.
                            let raw = if mouse_event.modifiers.is_empty() {
                                drag.modifiers
                            } else {
                                mouse_event.modifiers
                            };
                            let action = match decode_drag_modifiers(raw) {
                                DragMode::Onto => DragAction::RebaseOnto {
                                    source_revs: drag.source_revs,
                                    source_head: drag.source_head,
                                    target,
                                },
                                DragMode::After => DragAction::RebaseAfter {
                                    source_revs: drag.source_revs,
                                    source_head: drag.source_head,
                                    target,
                                },
                                DragMode::Before => DragAction::RebaseBefore {
                                    source_revs: drag.source_revs,
                                    source_head: drag.source_head,
                                    target,
                                },
                                DragMode::Squash => DragAction::Squash {
                                    source_revs: drag.source_revs,
                                    target,
                                },
                            };
                            self.pending_action = Some(action);
                        }
                        return Ok(ComponentInputResult::Handled);
                    }

                    // Fallback: legacy click-to-select behaviour.
                    if let Some(inx) = self.line_under_mouse(&mouse_event)
                        && let Some(head) = self.head_at_log_line(inx)
                    {
                        self.set_head(head);
                        return Ok(ComponentInputResult::Handled);
                    }
                }
                MouseEventKind::Up(MouseButton::Right) => {
                    // A right-button release shouldn't end an in-flight
                    // left-drag. Otherwise, select the head under the cursor
                    // and report the event handled so callers (e.g. the log
                    // tab opening a context menu) can react against the
                    // updated selection.
                    if self.drag.is_none()
                        && let Some(inx) = self.line_under_mouse(&mouse_event)
                        && let Some(head) = self.head_at_log_line(inx)
                    {
                        self.set_head(head);
                        return Ok(ComponentInputResult::Handled);
                    }
                }
                MouseEventKind::Up(_) => {
                    // Non-left, non-right button release while a left-drag is
                    // in flight shouldn't end the drag. Otherwise nothing to
                    // do.
                }
                _ => {} // Handle other mouse events if necessary
            }
        }

        if let Event::Key(key_event) = event {
            if let Some(drag) = self.drag.as_mut() {
                if let KeyCode::Modifier(mod_code) = key_event.code {
                    let flag = modifier_code_to_flag(mod_code);
                    match key_event.kind {
                        KeyEventKind::Press => drag.modifiers |= flag,
                        KeyEventKind::Release => drag.modifiers &= !flag,
                        KeyEventKind::Repeat => {}
                    }
                    return Ok(ComponentInputResult::Handled);
                }
            }
        }

        Ok(ComponentInputResult::NotHandled)
    }
}

impl LogPanel<'_> {
    /// Map a mouse event to a log line index, if it points inside the list.
    fn line_under_mouse(&self, mouse_event: &MouseEvent) -> Option<usize> {
        let log_lines = self.log_lines();
        let log_items: Vec<ListItem> = log_lines
            .iter()
            .map(|line| ListItem::from(line.to_text()))
            .collect();
        list_item_from_mouse_event(&log_items, self.log_rect, &self.log_list_state, mouse_event)
    }

    /// True when a drag started inside this panel is still in progress.
    pub fn drag_active(&self) -> bool {
        self.drag.is_some()
    }

    /// Source commits for the in-flight drag, if any.
    pub fn drag_source_revs(&self) -> Option<&[CommitId]> {
        self.drag.as_ref().map(|d| d.source_revs.as_slice())
    }

    /// Head currently under the cursor for the in-flight drag, if any.
    pub fn drag_target_head(&self) -> Option<&Head> {
        self.drag.as_ref().and_then(|d| d.target_head.as_ref())
    }

    /// Modifier keys reported on the most recent mouse event of the
    /// in-flight drag, if any. Used to preview the action that release
    /// would trigger.
    pub fn drag_modifiers(&self) -> Option<KeyModifiers> {
        self.drag.as_ref().map(|d| d.modifiers)
    }

    /// Head where the current drag started, if any.
    pub fn drag_source_head(&self) -> Option<Head> {
        let drag = self.drag.as_ref()?;
        self.log_output
            .as_ref()
            .ok()?
            .head_at(drag.source_line)
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
        let log_top = self.log_rect.y;
        let log_bottom = self.log_rect.bottom();
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
            self.head = drag.selection_at_start;
        }
        self.pending_action = None;
    }

    /// Take the pending drop action, if any.
    pub fn take_pending_drag_action(&mut self) -> Option<DragAction> {
        self.pending_action.take()
    }
}

// Determine which list item a mouse event is related to
fn list_item_from_mouse_event(
    list: &[ListItem],
    list_rect: Rect,
    list_state: &ListState,
    mouse_event: &MouseEvent,
) -> Option<usize> {
    let mouse_pos = Position::new(mouse_event.column, mouse_event.row);
    if !list_rect.contains(mouse_pos) {
        return None;
    }

    // Assume that each item is exactly one line.
    // This is not true in the general case, but it is in this module.
    let mouse_offset = mouse_pos.y - list_rect.y;
    let item_index = list_state.offset() + mouse_offset as usize;
    if item_index >= list.len() {
        return None;
    }
    Some(item_index)
}
