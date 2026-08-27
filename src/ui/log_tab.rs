#![expect(clippy::borrow_interior_mutable_const)]

use std::io::Read;
use std::process::Child;
use std::sync::mpsc::Sender;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::clipboard::CopyToClipboard;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::crossterm::execute;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui_textarea::CursorMove;
use ratatui_textarea::TextArea;
use tracing::debug;
use tracing::error;
use tracing::instrument;
use tracing::warn;
use tui_confirm_dialog::ButtonLabel;
use tui_confirm_dialog::ConfirmDialog;
use tui_confirm_dialog::ConfirmDialogState;
use tui_confirm_dialog::Listener;

use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::commander::revset::Revset;
use crate::env::DiffFormat;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::event::AppEvent;
use crate::keybinds::DetailsPanelEvent;
use crate::keybinds::DetailsPanelKeybinds;
use crate::keybinds::LogTabEvent;
use crate::keybinds::LogTabKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::commit_show_cache::CommitShowCache;
use crate::ui::commit_show_cache::CommitShowKey;
use crate::ui::commit_show_cache::CommitShowRequest;
use crate::ui::dialog::BookmarkSetPopup;
use crate::ui::dialog::DescribePopup;
use crate::ui::dialog::LoaderPopup;
use crate::ui::dialog::MessagePopup;
use crate::ui::dialog::RebasePopup;
use crate::ui::panel::DetailsPanel;
use crate::ui::panel::LargeStringContent;
use crate::ui::panel::LogPanel;
use crate::ui::panel::TextContent;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::Timer;
use crate::ui::utils::centered_rect_line_height;
use crate::ui::utils::tabs_to_spaces;
use crate::ui::utils::waiting_message;

const NEW_POPUP_ID: u16 = 1;
const EDIT_POPUP_ID: u16 = 2;
const ABANDON_POPUP_ID: u16 = 3;
const SQUASH_POPUP_ID: u16 = 4;

/// How long the details panel hides that 'jj show' is running: it keeps
/// showing the change it already has and, if it has none, stays empty
/// until the request it waits for has taken this long. Each request
/// gets its own grace, so moving on keeps the panel quiet.
const LOADING_GRACE: Duration = Duration::from_secs(1);

/// Log tab. Shows `jj log` in main panel and shows selected change details of in details panel.
pub struct LogTab<'a> {
    /// Channel for app events
    app_event_sender: Sender<AppEvent>,

    /// The revset filter to apply to jj log
    log_revset_textarea: Option<TextArea<'a>>,

    /// The list of changes shown to the left
    log_panel: LogPanel<'a>,

    /// The panel showing change content to the right
    head_panel: DetailsPanel,

    /// The selected change content key in the cache
    head_key: CommitShowKey,

    /// The content key the details panel currently renders. This lags
    /// behind `head_key` while we wait for 'jj show'
    shown_key: Option<CommitShowKey>,

    /// Cached change content
    commit_show_cache: CommitShowCache,

    /// Child process for computing 'jj show'
    pending_jj_show: Option<PendingJjShow>,

    /// The currently selected change. It is a copy of `self.log_panel.head`,
    /// so if these differ, we need to update `self.head`
    head: Head,

    diff_format: DiffFormat,

    popup: ConfirmDialogState,
    popup_tx: std::sync::mpsc::Sender<Listener>,
    popup_rx: std::sync::mpsc::Receiver<Listener>,

    describe_after_new: bool,

    squash_ignore_immutable: bool,
    squash_target: Option<Head>,

    edit_ignore_immutable: bool,

    config: JjConfig,
    pane_divider: PaneDivider,
    keybinds: LogTabKeybinds,
    details_keybinds: DetailsPanelKeybinds,

    stale: bool,
}

/// A background process for fetching 'jj show' and a thread for signalling
/// the UI while waiting
struct PendingJjShow {
    /// The 'jj show' the child was launched for
    request: CommitShowRequest,
    /// The child process executing 'jj show'
    child: Child,
    /// The thread reading stdout from child process
    stdout_reader: JoinHandle<Vec<u8>>,
    /// The thread reading stderr from child process
    stderr_reader: JoinHandle<Vec<u8>>,
    /// A timer used to signal the application while child is running
    timer: Timer<AppEvent>,
}

/**
# Supporting functions
Normally the event handling code would call
member functions on log_panel and head_panel, but some operations
are a little more complex. They get a supporting function.

The main functions are:

* [set_head](LogTab::set_head) - Move the selection to a particular
  commit. Update panels.

* [refresh_log_output](LogTab::refresh_log_output) - Update the log panel
  by running `jj log`, and update the details panel.
  (called by refresh)

* [sync_head_output](LogTab::sync_head_output) - Make right panel show
  what left panel selected.
  (called by refresh_log_output)

* [refresh_head_output](LogTab::refresh_head_output) - Update content of
  right panel
  (called by sync_head_output)

* [request_jj_show](LogTab::request_jj_show) - Spawn `jj show` for a commit,
  replacing any request that is still running
  (called by refresh_head_output)

* [try_read_jj_show_output](LogTab::try_read_jj_show_output) - Move the
  output of a finished `jj show` into the cache
  (called by refresh_head_output)
*/
impl<'a> LogTab<'a> {
    #[instrument(level = "info", name = "Initializing log tab", parent = None, skip())]
    pub fn new(app_event_sender: Sender<AppEvent>, head: Head) -> Self {
        let diff_format = get_env().jj_config.diff_format();

        let head_key = CommitShowKey::new(head.clone(), diff_format.clone());

        let commit_show_cache = CommitShowCache::new();

        let (popup_tx, popup_rx) = std::sync::mpsc::channel();

        let mut keybinds = LogTabKeybinds::default();
        let mut details_keybinds = DetailsPanelKeybinds::default();
        if let Some(keybinds_config) = get_env().jj_config.keybinds() {
            keybinds.extend_from_config(keybinds_config);
            details_keybinds.extend_from_config(
                keybinds_config
                    .log_tab
                    .as_ref()
                    .and_then(|c| c.toggle_diff_format.as_ref()),
            );
        }

        let config = get_env().jj_config.clone();
        let pane_divider = PaneDivider::new(config.layout_percent());

        Self {
            app_event_sender,

            log_revset_textarea: None,

            log_panel: LogPanel::new(head.clone()),

            head,
            head_panel: DetailsPanel::new(),
            head_key,
            shown_key: None,

            commit_show_cache,
            pending_jj_show: None,

            diff_format,

            popup: ConfirmDialogState::default(),
            popup_tx,
            popup_rx,

            describe_after_new: false,

            squash_ignore_immutable: false,
            squash_target: None,

            edit_ignore_immutable: false,

            config,
            pane_divider,
            keybinds,
            details_keybinds,

            stale: true,
        }
    }

    /// Move the cursor, updating the details panel. The log itself is
    /// left as it was.
    pub fn set_head(&mut self, head: Head) {
        self.log_panel.set_head(head);
        self.sync_head_output();
    }

    /// Update the log panel and diff panel. This will also refresh
    /// the diff cache.
    fn refresh_log_output(&mut self) {
        self.log_panel.refresh_log_output();
        self.update_cache_active_commits();
        self.sync_head_output();
    }

    /// Extract selection from log panel and update change details panel
    fn sync_head_output(&mut self) {
        self.head = self.log_panel.head.clone();
        self.refresh_head_output();
    }

    /// Refesh the diff of the currently selected change
    fn refresh_head_output(&mut self) {
        // Check if the child process has new data for the cache
        self.try_read_jj_show_output();

        // Look up selected change via its key
        // If the key matches, then we can use the cached value.
        // This is not entierly true. A reconfiguration of jj could
        // generate different output for some keys. We probably need
        // a forced cache clear function.

        // The next draw requests `jj show` for the new key if it is not
        // already cached, and moves the panel over once it has content.
        self.head_key = CommitShowKey::new(self.head.clone(), self.diff_format.clone());
    }

    /// The 'jj show' the details panel wants for the selected change. The
    /// panel reports the width it got in the last frame, so this is only
    /// meaningful once it has been drawn.
    fn head_show_request(&self) -> CommitShowRequest {
        CommitShowRequest::new(self.head_key.clone(), self.head_panel.columns() as usize)
    }

    /// The content the details panel is to render. This is the selected
    /// change as soon as the cache can serve it, and the change the panel
    /// already shows while we briefly wait for 'jj show'.
    fn key_to_render(&self) -> Option<CommitShowKey> {
        if self.commit_show_cache.get(&self.head_key).is_some() {
            return Some(self.head_key.clone());
        }
        let pending = self.pending_jj_show.as_ref()?;
        if pending.timer.elapsed() < LOADING_GRACE {
            return self.shown_key.clone();
        }
        None
    }

    //
    // Cache related
    //

    /// Mark all active elements as dirty, which will trigger a cache
    /// update next time they are requested.
    fn mark_cache_as_dirty(&mut self) {
        self.commit_show_cache.mark_dirty();
    }

    /// Get the list of active commits from the log panel, and mark
    /// the changes there as active. For non-active changes, keep at most
    /// one commit.
    fn update_cache_active_commits(&mut self) {
        let active_heads = self.log_panel.log_heads();
        self.commit_show_cache
            .set_active(active_heads, &self.diff_format);
    }

    /// Launch of a child process for 'jj show'
    fn request_jj_show(&mut self, request: CommitShowRequest) {
        // Ignore request for already pending key
        if let Some(pjs) = self.pending_jj_show.as_ref()
            && pjs.request == request
        {
            return;
        }
        // Kill old child process
        if let Some(mut pjs) = self.pending_jj_show.take() {
            // TODO implement std::fmt::Display for CommitShowKey
            //debug!("Kill 'jj show' that was too slow. key={}", &key);
            if let Err(err) = pjs.child.kill() {
                error!("Kill failed on 'jj show' child process: {err}");
            }
            let _ = pjs.child.wait();
        }

        // Lanuch new child process that runs 'jj show'
        let launch_key = request.key().clone();
        let mut commander = new_commander();
        commander.limit_width(request.width());
        let launch_child =
            commander.spawn_commit_show(&launch_key.id.commit_id, &launch_key.format, true);
        let mut launch_child = match launch_child {
            Ok(child) => child,
            Err(err) => {
                error!("Unable to spawn 'jj show': {err}");
                self.show_error_in_details_panel(request, err.to_string());
                return;
            }
        };

        let stdout_reader = spawn_pipe_reader("stdout", launch_child.stdout.take().unwrap());
        let stderr_reader = spawn_pipe_reader("stderr", launch_child.stderr.take().unwrap());

        // Check for updates from child
        let launch_timer = Timer::new(self.app_event_sender.clone());
        launch_timer.signal_in(AppEvent::Refresh, Duration::from_millis(100));

        let pjs = PendingJjShow {
            request,
            child: launch_child,
            stdout_reader,
            stderr_reader,
            timer: launch_timer,
        };
        self.pending_jj_show = Some(pjs);
    }

    /// Update the cache with data from the child process, if it is
    /// ready.
    fn try_read_jj_show_output(&mut self) {
        let Some(mut pjs) = self.pending_jj_show.take() else {
            return;
        };

        let wait_result = pjs.child.try_wait();
        if let Err(err) = wait_result {
            // Abort on error, but log what happended
            error!(
                "Unable to get result from 'jj show'. try_wait on child failed with message: {err}"
            );
            // TODO: Maybe we want to kill the child process here?
            self.pending_jj_show = Some(pjs);
            return;
        }

        let Some(status) = wait_result.unwrap() else {
            // Child not done yet
            let next_check_in = if pjs.timer.elapsed() < Duration::from_secs(1) {
                Duration::from_millis(100)
            } else {
                Duration::from_millis(1000)
            };
            pjs.timer.signal_in(AppEvent::Refresh, next_check_in);
            self.pending_jj_show = Some(pjs);
            return;
        };
        debug!("jj show child process exited with status {status}");

        // Read data from child process into cache
        let stdout = pjs.stdout_reader.join().unwrap_or_default();
        let stderr_bytes = pjs.stderr_reader.join().unwrap_or_default();
        let stderr = String::from_utf8_lossy(&stderr_bytes);
        pjs.timer.stop();

        // A failing 'jj show' has nothing useful on stdout, so show the
        // error instead of caching an empty document for the commit.
        if !status.success() {
            error!("'jj show' exited with status {status}:\n{stderr}");
            self.show_error_in_details_panel(pjs.request, format!("jj show failed:\n\n{stderr}"));
            return;
        }
        if !stderr.is_empty() {
            warn!("Ignoring stderr from child process:\n{stderr}");
        }

        let text = tabs_to_spaces(&String::from_utf8_lossy(&stdout));
        let value = pjs.request.into_value(text);
        self.commit_show_cache.insert_document(value);

        // Note: self.pending_jj_show.take() has already cleared the
        // child handle, which indicates room for the next child process
    }

    /// Cache `message` as the content the request asked for, so the
    /// details panel shows it instead of staying blank.
    fn show_error_in_details_panel(&mut self, request: CommitShowRequest, message: String) {
        self.commit_show_cache
            .insert_document(request.into_value(message));
    }
}

/// Drain a child process pipe from its own thread. The pipe buffer would
/// otherwise fill up on large diffs and block the child before it exits.
fn spawn_pipe_reader<R: Read + Send + 'static>(
    name: &'static str,
    mut pipe: R,
) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        if let Err(err) = pipe.read_to_end(&mut output) {
            error!("Failed to read {name} from 'jj show': {err}");
        }
        output
    })
}

/**
# Event handling
Event handling happens in [`LogTab::handle_event`]. Over time, this has
caused it to grow to a very long match with many arms. The size makes it hard
to see what is going on, and the indentation is very deep.

To fix this, we have begun a new code pattern, were the match arm simply
calls a function. Most actions are two step operations, first create a dialog
, then execcute some command. This is reflected in two functions located near
each other in code:
* `handle_<action>` - Set up the dialog and show it.
* `execute_<action>` - Perform some action after the dialog closed.
*/
impl<'a> LogTab<'a> {
    fn handle_new(&mut self, describe: bool) -> Result<ComponentInputResult> {
        let mark_count = self.log_panel.marked_heads.len();
        let text = if mark_count > 0 {
            Text::from(vec![Line::from(format!(
                "Are you sure you want to create a new change with {mark_count} marked parents?"
            ))])
            .fg(Color::default())
        } else {
            Text::from(vec![
                Line::from("Are you sure you want to create a new change?"),
                Line::from(format!("New parent: {}", self.head.change_id.as_str())),
            ])
            .fg(Color::default())
        };
        self.popup = ConfirmDialogState::new(
            NEW_POPUP_ID,
            Span::styled(" New ", Style::new().bold().cyan()),
            text,
        );
        self.popup
            .with_yes_button(ButtonLabel::YES.clone())
            .with_no_button(ButtonLabel::NO.clone())
            .with_listener(Some(self.popup_tx.clone()))
            .open();
        self.describe_after_new = describe;
        Ok(ComponentInputResult::Handled)
    }

    // Execute new command, after self.popup returned
    fn execute_new(&mut self) -> Result<Option<AppAction>> {
        let commit_ids = self.log_panel.extract_and_clear_head_marks();
        let revset =
            Revset::union(&commit_ids).unwrap_or_else(|| Revset::from(&self.head.commit_id));
        new_commander().run_new(revset)?;
        self.set_head(new_commander().get_current_head()?);
        if self.describe_after_new {
            self.describe_after_new = false;
            return Ok(Some(AppAction::Multiple(vec![
                AppAction::ChangeHead(self.head.clone()),
                AppAction::SetPopup(Box::new(DescribePopup::new(self.head.clone(), vec![]))),
            ])));
        }
        Ok(Some(AppAction::Multiple(vec![
            AppAction::ChangeHead(self.head.clone()),
            AppAction::RefreshTab,
        ])))
    }

    fn handle_abandon(&mut self) -> Result<ComponentInputResult> {
        // Cannot abandon immutable changes
        if self.head.immutable {
            return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                Box::new(MessagePopup::new(
                    "Abandon",
                    "The change cannot be abandoned because it is immutable.",
                )),
            )));
        }

        // Ask for confirmation by launching a popup
        let mark_count = self.log_panel.marked_heads.len();
        let text = if mark_count > 0 {
            Text::from(vec![Line::from(format!(
                "Are you sure you want to abandon {} marked changes?",
                mark_count
            ))])
            .fg(Color::default())
        } else {
            Text::from(vec![
                Line::from("Are you sure you want to abandon this change?"),
                Line::from(format!("Change: {}", self.head.change_id.as_str())),
            ])
            .fg(Color::default())
        };
        self.popup = ConfirmDialogState::new(
            ABANDON_POPUP_ID,
            Span::styled(" Abandon ", Style::new().bold().cyan()),
            text,
        );
        self.popup
            .with_yes_button(ButtonLabel::YES.clone())
            .with_no_button(ButtonLabel::NO.clone())
            .with_listener(Some(self.popup_tx.clone()))
            .open();
        Ok(ComponentInputResult::Handled)
    }

    // Execute abandon command, after self.popup returned
    fn execute_abandon(&mut self) -> Result<Option<AppAction>> {
        // If none marked, mark current head
        if self.log_panel.marked_heads.is_empty() {
            self.log_panel.toggle_head_mark();
        }
        // Move selection to parent until it is no longer inside the marked commits
        let old_selection = self.head.clone();
        let mut selection = self.head.clone();
        while self.log_panel.is_head_marked(&selection) {
            selection = new_commander().get_commit_parent(&selection.commit_id)?;
        }
        // Abandon marked commmits
        let commit_id_list = self.log_panel.extract_and_clear_head_marks();
        let revset =
            Revset::union(&commit_id_list).unwrap_or_else(|| Revset::from(&self.head.commit_id));
        new_commander().run_abandon(revset)?;
        // Update selection to latest version, in case abandon triggered a rebase.
        let new_selection = new_commander().get_head_latest(&selection)?;
        // Update log panel and diff panel
        self.set_head(new_selection.clone());
        // If selection was moved, tell the application
        if new_selection != old_selection {
            Ok(Some(AppAction::Multiple(vec![
                AppAction::ChangeHead(self.head.clone()),
                AppAction::RefreshTab,
            ])))
        } else {
            Ok(Some(AppAction::RefreshTab))
        }
    }

    fn handle_event(&mut self, log_tab_event: LogTabEvent) -> Result<ComponentInputResult> {
        match log_tab_event {
            LogTabEvent::ScrollToBottom
            | LogTabEvent::ScrollToTop
            | LogTabEvent::ToggleHeadMark => {
                self.log_panel.handle_event(log_tab_event)?;
                self.sync_head_output();
            }
            LogTabEvent::Duplicate => {
                let _ = new_commander().run_duplicate(&self.head.change_id.to_string());
                return Ok(ComponentInputResult::HandledAction(AppAction::RefreshTab));
            }

            LogTabEvent::CreateNew { describe } => {
                return self.handle_new(describe);
            }
            LogTabEvent::Rebase => {
                let source_change = new_commander().get_current_head()?;
                return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                    Box::new(RebasePopup::new(source_change, self.head.clone())),
                )));
            }
            LogTabEvent::Squash { ignore_immutable } => {
                let current_head = new_commander().get_current_head()?;
                let target = if self.head.change_id == current_head.change_id {
                    match new_commander().get_commit_parent(&current_head.commit_id) {
                        Ok(parent) => {
                            self.squash_target = Some(parent.clone());
                            parent
                        }
                        Err(_) => {
                            return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                                Box::new(MessagePopup::new(
                                    "Squash",
                                    "Cannot squash onto current change",
                                )),
                            )));
                        }
                    }
                } else {
                    self.squash_target = None;
                    self.head.clone()
                };

                if target.immutable && !ignore_immutable {
                    return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                        Box::new(MessagePopup::new(
                            "Squash",
                            "Cannot squash onto immutable change",
                        )),
                    )));
                }

                let description = if self.squash_target.is_some() {
                    "Are you sure you want to squash @ into its parent?"
                } else {
                    "Are you sure you want to squash @ into this change?"
                };
                let mut lines = vec![
                    Line::from(description),
                    Line::from(format!("Squash into {}", target.change_id.as_str())),
                ];
                if ignore_immutable {
                    lines.push(Line::from("This change is immutable."));
                }
                self.popup = ConfirmDialogState::new(
                    SQUASH_POPUP_ID,
                    Span::styled(" Squash ", Style::new().bold().cyan()),
                    Text::from(lines).fg(Color::default()),
                );
                self.popup
                    .with_yes_button(ButtonLabel::YES.clone())
                    .with_no_button(ButtonLabel::NO.clone())
                    .with_listener(Some(self.popup_tx.clone()))
                    .open();
                self.squash_ignore_immutable = ignore_immutable;
            }
            LogTabEvent::EditChange { ignore_immutable } => {
                if self.head.immutable && !ignore_immutable {
                    return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                        Box::new(MessagePopup::new(
                            " Edit ",
                            "The change cannot be edited because it is immutable.",
                        )),
                    )));
                }

                let mut lines = vec![
                    Line::from("Are you sure you want to edit an existing change?"),
                    Line::from(format!("Change: {}", self.head.change_id.as_str())),
                ];
                if ignore_immutable {
                    lines.push(Line::from("This change is immutable."))
                }
                self.popup = ConfirmDialogState::new(
                    EDIT_POPUP_ID,
                    Span::styled(" Edit ", Style::new().bold().cyan()),
                    Text::from(lines).fg(Color::default()),
                );
                self.popup
                    .with_yes_button(ButtonLabel::YES.clone())
                    .with_no_button(ButtonLabel::NO.clone())
                    .with_listener(Some(self.popup_tx.clone()))
                    .open();
                self.edit_ignore_immutable = ignore_immutable;
            }
            LogTabEvent::Abandon => {
                return self.handle_abandon();
            }
            LogTabEvent::Absorb => {
                new_commander().run_absorb(&self.head.commit_id)?;
                self.set_head(new_commander().get_head_latest(&self.head)?);
                return Ok(ComponentInputResult::HandledAction(AppAction::Multiple(
                    vec![
                        AppAction::ChangeHead(self.head.clone()),
                        AppAction::RefreshTab,
                    ],
                )));
            }
            LogTabEvent::Describe => {
                if self.head.immutable {
                    return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                        Box::new(MessagePopup::new(
                            "Describe",
                            "The change cannot be described because it is immutable.",
                        )),
                    )));
                } else {
                    let lines = new_commander()
                        .get_commit_description(&self.head.commit_id)?
                        .split("\n")
                        .map(|line| line.to_string())
                        .collect();
                    return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                        Box::new(DescribePopup::new(self.head.clone(), lines)),
                    )));
                }
            }
            LogTabEvent::EditRevset => {
                let mut textarea = TextArea::new(
                    self.log_panel
                        .log_revset
                        .as_ref()
                        .unwrap_or(&"".to_owned())
                        .lines()
                        .map(String::from)
                        .collect(),
                );
                textarea.move_cursor(CursorMove::End);
                self.log_revset_textarea = Some(textarea);
                return Ok(ComponentInputResult::Handled);
            }
            LogTabEvent::SetBookmark => {
                return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                    Box::new(BookmarkSetPopup::new(
                        self.config.clone(),
                        Some(self.head.change_id.clone()),
                        self.head.commit_id.clone(),
                    )),
                )));
            }
            LogTabEvent::OpenFiles => {
                return Ok(ComponentInputResult::HandledAction(AppAction::ViewFiles(
                    self.head.clone(),
                )));
            }
            LogTabEvent::CopyChangeId => {
                // Copy change ID to clipboard using crossterm
                let change_id = self.head.change_id.as_str();
                let _ = execute!(
                    std::io::stdout(),
                    CopyToClipboard::to_clipboard_from(change_id)
                );
            }
            LogTabEvent::CopyRev => {
                // Copy revision (commit ID) to clipboard using crossterm
                let commit_id = self.head.commit_id.as_str();
                let _ = execute!(
                    std::io::stdout(),
                    CopyToClipboard::to_clipboard_from(commit_id)
                );
            }
            LogTabEvent::Push {
                all_bookmarks,
                allow_new,
            } => {
                let commit_id = self.head.commit_id.clone();

                let loader = LoaderPopup::new("Pushing".to_string(), move || {
                    new_commander().git_push(all_bookmarks, allow_new, &commit_id)
                });

                return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                    Box::new(loader),
                )));
            }
            LogTabEvent::Fetch { all_remotes } => {
                let loader = LoaderPopup::new("Fetching".to_string(), move || {
                    new_commander().git_fetch(all_remotes)
                });

                return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                    Box::new(loader),
                )));
            }
            LogTabEvent::Save
            | LogTabEvent::Cancel
            | LogTabEvent::ClosePopup
            | LogTabEvent::Unbound => return Ok(ComponentInputResult::NotHandled),
        };
        Ok(ComponentInputResult::Handled)
    }
}

impl Tab for LogTab<'_> {
    fn refresh(&mut self) -> Result<()> {
        // The log we are about to read leaves the working copy alone, so
        // following the selection has to as well, or the two disagree.
        let mut commander = new_commander();
        commander.ignore_working_copy();

        let latest_head = commander.get_head_latest(&self.head)?;
        self.log_panel.set_head(latest_head);
        self.refresh_log_output();
        self.stale = false;

        Ok(())
    }

    fn mark_stale(&mut self) {
        self.stale = true;
    }

    fn is_stale(&self) -> bool {
        self.stale
    }

    fn drop_caches(&mut self) {
        self.mark_cache_as_dirty();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        let half_page = self.log_panel.visible_heads() as isize / 2;
        self.log_panel.scroll_relative(match scroll {
            Scroll::Down => 1,
            Scroll::Up => -1,
            Scroll::DownHalfPage => half_page,
            Scroll::UpHalfPage => half_page.saturating_neg(),
        });
        self.sync_head_output();
        Ok(())
    }

    fn focus_current(&mut self) -> Result<()> {
        self.set_head(new_commander().get_current_head()?);
        Ok(())
    }

    fn make_main_panel_help(&self) -> Vec<(String, String)> {
        self.keybinds.make_main_panel_help()
    }

    fn make_details_panel_help(&self) -> Vec<(String, String)> {
        self.details_keybinds.make_help()
    }
}

impl Component for LogTab<'_> {
    fn update(&mut self) -> Result<Option<AppAction>> {
        // Check for popup action
        if let Ok(res) = self.popup_rx.try_recv()
            && res.1.unwrap_or(false)
        {
            match res.0 {
                NEW_POPUP_ID => {
                    return self.execute_new();
                }
                EDIT_POPUP_ID => {
                    new_commander().run_edit(&self.head.commit_id, self.edit_ignore_immutable)?;
                    return Ok(Some(AppAction::Multiple(vec![
                        AppAction::ChangeHead(self.head.clone()),
                        AppAction::RefreshTab,
                    ])));
                }
                ABANDON_POPUP_ID => {
                    return self.execute_abandon();
                }
                SQUASH_POPUP_ID => {
                    let target_id = self
                        .squash_target
                        .take()
                        .unwrap_or_else(|| self.head.clone())
                        .commit_id;
                    new_commander().run_squash(target_id.as_str(), self.squash_ignore_immutable)?;
                    self.set_head(new_commander().get_current_head()?);
                    return Ok(Some(AppAction::Multiple(vec![
                        AppAction::ChangeHead(self.head.clone()),
                        AppAction::RefreshTab,
                    ])));
                }
                _ => {}
            }
        }

        Ok(None)
    }

    fn draw(
        &mut self,
        f: &mut ratatui::prelude::Frame<'_>,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        let chunks = self.pane_divider.split(area, self.config.layout());

        // Draw log
        self.log_panel.draw(f, chunks[0])?;

        // Draw change details
        self.try_read_jj_show_output();
        let head_request = self.head_show_request();
        if !self.commit_show_cache.is_fresh(&head_request) {
            self.request_jj_show(head_request);
        }
        if let Some(key) = self.key_to_render()
            && let Some(content) = self.commit_show_cache.get(&key)
        {
            // Read a change from its top, but stay put while it is only
            // being rewritten under us
            if self.shown_key.as_ref().map(|shown| &shown.id.change_id) != Some(&key.id.change_id) {
                self.head_panel.scroll_to(0);
            }
            self.head_panel
                .render_context::<LargeStringContent>(content.value())
                .title(format!(" Details for {} ", key.id.change_id))
                .draw(f, chunks[1]);
            self.shown_key = Some(key);
        } else if let Some(pjs) = &self.pending_jj_show {
            self.head_panel
                .render_context::<TextContent>(waiting_message(
                    Some(pjs.timer.elapsed()),
                    "jj show",
                    LOADING_GRACE,
                ))
                .title(format!(" Details for {} ", self.head.change_id))
                .draw(f, chunks[1])
        }

        // Draw popup
        if self.popup.is_opened() {
            let popup = ConfirmDialog::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green))
                .selected_button_style(
                    Style::default()
                        .bg(self.config.highlight_color())
                        .underlined(),
                );
            f.render_stateful_widget(popup, area, &mut self.popup);
        }

        // Draw revset textarea
        {
            if let Some(log_revset_textarea) = self.log_revset_textarea.as_mut() {
                let block = Block::bordered()
                    .title(Span::styled(" Revset ", Style::new().bold().cyan()))
                    .title_alignment(Alignment::Center)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Green));
                let area = centered_rect_line_height(area, 30, 7);
                f.render_widget(Clear, area);
                f.render_widget(&block, area);

                let popup_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Fill(1), Constraint::Length(2)])
                    .split(block.inner(area));

                f.render_widget(&*log_revset_textarea, popup_chunks[0]);

                let help = Paragraph::new(vec!["Ctrl+s: save | Escape: cancel".into()])
                    .fg(Color::DarkGray)
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .borders(Borders::TOP)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(Color::DarkGray)),
                    );

                f.render_widget(help, popup_chunks[1]);
            }
        }

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Some(log_revset_textarea) = self.log_revset_textarea.as_mut() {
            if let Event::Key(key) = event {
                match self.keybinds.match_event(key) {
                    LogTabEvent::Save => {
                        let log_revset = log_revset_textarea.lines().join("\n");
                        self.log_panel.log_revset = if log_revset.trim().is_empty() {
                            None
                        } else {
                            Some(log_revset)
                        };
                        self.refresh_log_output();
                        self.log_revset_textarea = None;
                        return Ok(ComponentInputResult::Handled);
                    }
                    LogTabEvent::Cancel => {
                        self.log_revset_textarea = None;
                        return Ok(ComponentInputResult::Handled);
                    }
                    _ => (),
                }
            }
            log_revset_textarea.input(event);
            return Ok(ComponentInputResult::Handled);
        }

        if let Event::Key(key) = &event {
            let key = *key;
            if key.kind != KeyEventKind::Press {
                return Ok(ComponentInputResult::Handled);
            }

            if self.popup.is_opened() {
                if matches!(
                    self.keybinds.match_event(key),
                    LogTabEvent::ClosePopup | LogTabEvent::Cancel
                ) {
                    self.popup = ConfirmDialogState::default();
                } else {
                    self.popup.handle(&key);
                }

                return Ok(ComponentInputResult::Handled);
            }

            match self.details_keybinds.match_event(key) {
                DetailsPanelEvent::Unbound => {}
                DetailsPanelEvent::ToggleDiffFormat => {
                    self.diff_format = self.diff_format.get_next(self.config.diff_tool());
                    self.refresh_head_output();
                    return Ok(ComponentInputResult::Handled);
                }
                ev => {
                    self.head_panel.handle_event(ev);
                    return Ok(ComponentInputResult::Handled);
                }
            }

            let input_result = self.log_panel.input(event)?;
            if input_result.is_handled() {
                self.sync_head_output();
                return Ok(input_result);
            }

            let log_tab_event = self.keybinds.match_event(key);
            return self.handle_event(log_tab_event);
        }

        if let Event::Mouse(mouse_event) = event {
            if self
                .pane_divider
                .handle_mouse(mouse_event, self.config.layout())
            {
                return Ok(ComponentInputResult::Handled);
            }
            let input_result = self.log_panel.input(event.clone())?;
            if input_result.is_handled() {
                self.sync_head_output();
                return Ok(input_result);
            }
            if self.head_panel.input_mouse(mouse_event) {
                return Ok(ComponentInputResult::Handled);
            }
            return Ok(ComponentInputResult::NotHandled);
        }

        Ok(ComponentInputResult::Handled)
    }
}
