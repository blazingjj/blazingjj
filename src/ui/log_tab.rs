#![expect(clippy::borrow_interior_mutable_const)]

use anyhow::Result;
use ratatui::crossterm::clipboard::CopyToClipboard;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::crossterm::execute;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui_textarea::CursorMove;
use ratatui_textarea::TextArea;
use tracing::instrument;
use tui_confirm_dialog::ButtonLabel;
use tui_confirm_dialog::ConfirmDialog;
use tui_confirm_dialog::ConfirmDialogState;
use tui_confirm_dialog::Listener;

use crate::app::TabId;
use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskOutput;
use crate::background_tasks::TaskResult;
use crate::background_tasks::TaskSlot;
use crate::commander::jj::NewInsertMode;
use crate::commander::log::Head;
use crate::commander::log::LOG_LINES_PER_HEAD;
use crate::commander::new_commander;
use crate::commander::revset::Revset;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::keybinds::DetailsPanelEvent;
use crate::keybinds::DetailsPanelKeybinds;
use crate::keybinds::LogTabEvent;
use crate::keybinds::LogTabKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::BookmarkSetPopup;
use crate::ui::dialog::ChoicePopup;
use crate::ui::dialog::DescribePopup;
use crate::ui::dialog::LoaderPopup;
use crate::ui::dialog::MessagePopup;
use crate::ui::dialog::RebasePopup;
use crate::ui::dialog::parent_select;
use crate::ui::panel::CommitShowPanel;
use crate::ui::panel::LogPanel;
use crate::ui::panel::MouseInput;
use crate::ui::panel::route_mouse;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::centered_rect_line_height;

const EDIT_POPUP_ID: u16 = 2;
const ABANDON_POPUP_ID: u16 = 3;
const SQUASH_POPUP_ID: u16 = 4;

/// Log tab. Shows `jj log` in main panel and shows selected change details of in details panel.
pub struct LogTab<'a> {
    /// Where the commands the tab runs itself go, so they do not block
    /// the UI thread
    background_tasks: BackgroundTasks,

    /// The revset filter to apply to jj log
    log_revset: Option<String>,

    /// Editor for the filter, up while it is being changed
    log_revset_textarea: Option<TextArea<'a>>,

    /// The list of changes shown to the left
    log_panel: LogPanel<'a>,

    /// The panel showing change content to the right
    head_panel: CommitShowPanel,

    /// The currently selected change. It is a copy of `self.log_panel.head`,
    /// so if these differ, we need to update `self.head`
    head: Head,

    popup: ConfirmDialogState,
    popup_tx: std::sync::mpsc::Sender<Listener>,
    popup_rx: std::sync::mpsc::Receiver<Listener>,

    goto_parent_tx: std::sync::mpsc::Sender<Head>,
    goto_parent_rx: std::sync::mpsc::Receiver<Head>,

    new_insert_tx: std::sync::mpsc::Sender<NewInsertMode>,
    new_insert_rx: std::sync::mpsc::Receiver<NewInsertMode>,

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
*/
impl<'a> LogTab<'a> {
    #[instrument(level = "info", name = "Initializing log tab", parent = None, skip(background_tasks))]
    pub fn new(background_tasks: BackgroundTasks, head: Head) -> Self {
        let (popup_tx, popup_rx) = std::sync::mpsc::channel();
        let (goto_parent_tx, goto_parent_rx) = std::sync::mpsc::channel();
        let (new_insert_tx, new_insert_rx) = std::sync::mpsc::channel();

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
            log_revset: get_env().default_revset.clone(),
            log_revset_textarea: None,

            log_panel: LogPanel::new(head.clone(), LOG_LINES_PER_HEAD),

            head,
            head_panel: CommitShowPanel::new(TabId::Log, background_tasks.clone()),

            background_tasks,

            popup: ConfirmDialogState::default(),
            popup_tx,
            popup_rx,
            goto_parent_tx,
            goto_parent_rx,

            new_insert_tx,
            new_insert_rx,

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
        let title = match &self.log_revset {
            Some(log_revset) => format!(" Log for: {log_revset} "),
            None => " Log ".to_owned(),
        };
        self.log_panel
            .show(new_commander().get_log(&self.log_revset), title);
        self.head_panel.set_active(self.log_panel.log_heads());
        self.sync_head_output();
    }

    /// Extract selection from log panel and update change details panel
    fn sync_head_output(&mut self) {
        self.head = self.log_panel.head.clone();
        let title = format!(" Details for {} ", self.head.change_id);
        self.head_panel.show(Some(self.head.clone()), title);
    }

    /// Run `operation` in `slot` and put up a loader popup for it, which
    /// stays until that slot's result arrives. The popup swallows all
    /// input, so the slot it waits for has to be the one submitted here.
    fn run_with_loader<F>(&self, operation_name: &str, slot: TaskSlot, operation: F) -> AppAction
    where
        F: FnOnce() -> TaskOutput + Send + 'static,
    {
        self.background_tasks
            .submit_uninterruptible(slot.clone(), operation);

        AppAction::SetPopup(Box::new(LoaderPopup::new(operation_name.to_owned(), slot)))
    }
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
        let target: String = if mark_count > 0 {
            format!("the {mark_count} marked changes")
        } else {
            self.head.change_id.as_str().chars().take(8).collect()
        };
        let items = vec![
            (
                Line::raw(format!("New child of {target}")),
                NewInsertMode::Child,
            ),
            (
                Line::raw(format!("Insert after {target}")),
                NewInsertMode::After,
            ),
            (
                Line::raw(format!("Insert before {target}")),
                NewInsertMode::Before,
            ),
        ];
        self.describe_after_new = describe;
        Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
            Box::new(ChoicePopup::new(
                self.config.clone(),
                self.new_insert_tx.clone(),
                "New",
                items,
            )),
        )))
    }

    // Execute new command, after the insertion point has been picked
    fn execute_new(&mut self, insert: NewInsertMode) -> Result<Option<AppAction>> {
        let describe = std::mem::take(&mut self.describe_after_new);
        let commit_ids: Vec<_> = self.log_panel.marked_heads.iter().cloned().collect();
        let revset =
            Revset::union(&commit_ids).unwrap_or_else(|| Revset::from(&self.head.commit_id));
        // Inserting can hit immutable changes, so report the refusal and
        // keep the marks for another attempt.
        if let Err(err) = new_commander().run_new_with_insert(revset, insert) {
            return Ok(Some(AppAction::SetPopup(Box::new(
                MessagePopup::new("New", format!("{err:#}")).text_align(Alignment::Left),
            ))));
        }
        self.log_panel.marked_heads.clear();
        self.set_head(new_commander().get_current_head()?);
        if describe {
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

    /// Move the selection to the parent of the current head, asking which
    /// one unless there is a single parent to go to.
    fn handle_goto_parent(&mut self) -> Result<ComponentInputResult> {
        let message = |text: &str| {
            Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                Box::new(MessagePopup::new("Go to parent", text)),
            )))
        };

        let parents = match new_commander().get_commit_parents(&self.head.commit_id) {
            Ok(parents) => parents,
            Err(err) => return message(&err.to_string()),
        };

        if parents.is_empty() {
            return message("The root commit has no parent");
        }

        // Selecting a change the log does not hold would leave the list
        // without a highlight, and the next scroll would start over at its
        // first change.
        let (mut parents, out_of_view): (Vec<_>, Vec<_>) = parents
            .into_iter()
            .partition(|parent| self.log_panel.shows_head(&parent.head));

        match parents.len() {
            0 => message("The log holds no parent of this change"),
            1 => {
                self.set_head(parents.remove(0).head);
                Ok(ComponentInputResult::Handled)
            }
            _ => Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                Box::new(parent_select(
                    self.config.clone(),
                    self.goto_parent_tx.clone(),
                    &parents,
                    &out_of_view,
                )),
            ))),
        }
    }

    fn handle_event(&mut self, log_tab_event: LogTabEvent) -> Result<ComponentInputResult> {
        match log_tab_event {
            LogTabEvent::ScrollToBottom => {
                self.log_panel.scroll_relative(isize::MAX);
                self.sync_head_output();
            }
            LogTabEvent::ScrollToTop => {
                self.log_panel.scroll_relative(-isize::MAX);
                self.sync_head_output();
            }
            LogTabEvent::ToggleHeadMark => {
                self.log_panel.toggle_head_mark();
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
                    self.log_revset
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
            LogTabEvent::OpenEvolog => {
                return Ok(ComponentInputResult::HandledAction(AppAction::ViewEvolog(
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

                return Ok(ComponentInputResult::HandledAction(self.run_with_loader(
                    "Pushing",
                    TaskSlot::GitPush,
                    move || Ok(new_commander().git_push(all_bookmarks, allow_new, &commit_id)?),
                )));
            }
            LogTabEvent::Fetch { all_remotes } => {
                return Ok(ComponentInputResult::HandledAction(self.run_with_loader(
                    "Fetching",
                    TaskSlot::GitFetch,
                    move || Ok(new_commander().git_fetch(all_remotes)?),
                )));
            }
            LogTabEvent::GotoParent => {
                return self.handle_goto_parent();
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
        self.head_panel.mark_dirty();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        let half_page = self.log_panel.visible_heads() / 2;
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
        self.head_panel.update();

        // Check for popup action
        if let Ok(res) = self.popup_rx.try_recv()
            && res.1.unwrap_or(false)
        {
            match res.0 {
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

        // Moving the selection leaves the repo as it is, so we do not
        // ask for a refresh.
        if let Ok(head) = self.goto_parent_rx.try_recv() {
            return Ok(Some(AppAction::ViewLog(head)));
        }

        if let Ok(insert) = self.new_insert_rx.try_recv() {
            return self.execute_new(insert);
        }

        Ok(None)
    }

    fn task_done(&mut self, result: TaskResult) -> Result<Option<AppAction>> {
        if let TaskSlot::CommitShow(_, request) = result.slot {
            self.head_panel.task_done(request, result.output);
        }
        Ok(None)
    }

    fn is_waiting(&self) -> bool {
        self.head_panel.is_waiting()
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
        self.head_panel.draw(f, chunks[1]);

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
                        self.log_revset = if log_revset.trim().is_empty() {
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
                ev => {
                    self.head_panel.handle_event(ev);
                    return Ok(ComponentInputResult::Handled);
                }
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
            match route_mouse(
                mouse_event,
                &mut [&mut self.log_panel, &mut self.head_panel],
            ) {
                MouseInput::Scroll(delta) => self.log_panel.scroll_relative(delta),
                MouseInput::Select(index) => {
                    if let Some(head) = self.log_panel.head_at_log_line(index) {
                        self.log_panel.set_head(head);
                    }
                }
                MouseInput::Handled => {}
                MouseInput::NotHandled => return Ok(ComponentInputResult::NotHandled),
            }
            self.sync_head_output();
            return Ok(ComponentInputResult::Handled);
        }

        Ok(ComponentInputResult::Handled)
    }
}
