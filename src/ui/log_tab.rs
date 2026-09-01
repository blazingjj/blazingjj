use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui_textarea::CursorMove;
use ratatui_textarea::TextArea;
use tracing::instrument;

use crate::app::TabId;
use crate::app::command;
use crate::app::command::Command;
use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskResult;
use crate::background_tasks::TaskSlot;
use crate::commander::ids::CommitId;
use crate::commander::jj::PushTarget;
use crate::commander::log::Head;
use crate::commander::log::LOG_LINES_PER_HEAD;
use crate::commander::new_commander;
use crate::commander::revset::Revset;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::event::Mouse;
use crate::keybinds::DetailsPanelEvent;
use crate::keybinds::DetailsPanelKeybinds;
use crate::keybinds::HelpItem;
use crate::keybinds::LogTabEvent;
use crate::keybinds::LogTabKeybinds;
use crate::keybinds::PopupEvent;
use crate::keybinds::PopupKeybinds;
use crate::keybinds::PushScope;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::MessagePopup;
use crate::ui::dialog::log_context_menu;
use crate::ui::dialog::parent_select;
use crate::ui::panel::CommitShowPanel;
use crate::ui::panel::LogPanel;
use crate::ui::panel::MouseInput;
use crate::ui::panel::copy_marked;
use crate::ui::panel::route_mouse;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::centered_rect_line_height;

/// Log tab. Shows `jj log` in main panel and shows selected change details of in details panel.
pub struct LogTab<'a> {
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

    config: JjConfig,
    pane_divider: PaneDivider,
    keybinds: LogTabKeybinds,
    details_keybinds: DetailsPanelKeybinds,
    revset_keybinds: PopupKeybinds,

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
        let keybinds = LogTabKeybinds::new();
        let details_keybinds = DetailsPanelKeybinds::new();

        let config = get_env().jj_config.clone();
        let pane_divider = PaneDivider::new(config.layout_percent());

        Self {
            log_revset: get_env().default_revset.clone(),
            log_revset_textarea: None,

            log_panel: LogPanel::new(head.clone(), LOG_LINES_PER_HEAD),

            head,
            head_panel: CommitShowPanel::new(TabId::Log, background_tasks),

            config,
            pane_divider,
            keybinds,
            details_keybinds,
            revset_keybinds: PopupKeybinds::text(),

            stale: true,
        }
    }

    /// Stop marking the changes that were marked, whatever they were
    /// marked for having been done to them.
    pub fn clear_marks(&mut self) {
        self.log_panel.marked_heads.clear();
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
}

/**
# Event handling
[`LogTab::handle_event`] turns an event into the request it stands for,
taking whatever that request acts on off the selection. The app runs it
from there.
*/
impl<'a> LogTab<'a> {
    /// What the operations in a menu or behind a key would act on.
    fn marked(&self) -> Vec<CommitId> {
        self.log_panel.marked_heads.iter().cloned().collect()
    }

    /// The menu of what can be done to the selected change, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> Result<Option<AppAction>> {
        Ok(Some(AppAction::SetPopup(Box::new(log_context_menu(
            self.config.clone(),
            anchor,
            &self.head,
            &self.marked(),
        )?))))
    }

    /// Move the selection to the parent of the current head, asking which
    /// one unless there is a single parent to go to.
    fn handle_goto_parent(&mut self) -> Result<Option<AppAction>> {
        let message = |text: &str| {
            Ok(Some(AppAction::SetPopup(Box::new(MessagePopup::new(
                "Go to parent",
                text,
            )))))
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
                Ok(None)
            }
            _ => Ok(Some(AppAction::SetPopup(Box::new(parent_select(
                self.config.clone(),
                &parents,
                &out_of_view,
            ))))),
        }
    }

    /// Push what `scope` says. Pushing the new bookmarks of a change
    /// means naming them, as jj only tracks a bookmark the remote does
    /// not have yet when it is asked for by name.
    fn handle_push(&self, scope: PushScope) -> Result<Option<AppAction>> {
        let target = match scope {
            PushScope::Selected => PushTarget::Revision(Revset::from(&self.head.commit_id)),
            PushScope::SelectedWithNew => {
                let message = |text: String| {
                    Ok(Some(AppAction::SetPopup(Box::new(
                        MessagePopup::new("Push", text).text_align(Alignment::Left),
                    ))))
                };

                let bookmarks = match new_commander()
                    .get_local_bookmarks(&Revset::from(&self.head.commit_id))
                {
                    Ok(bookmarks) => bookmarks,
                    Err(err) => return message(format!("{err:#}")),
                };
                if bookmarks.is_empty() {
                    return message("This change has no bookmark to push".to_owned());
                }

                PushTarget::Bookmarks(bookmarks.into_iter().map(|it| it.name).collect())
            }
            PushScope::Tracked => PushTarget::Tracked,
            PushScope::All => PushTarget::All,
        };

        Ok(Some(AppAction::Run(Command::Push(target))))
    }

    fn handle_event(&mut self, log_tab_event: LogTabEvent) -> Result<Option<AppAction>> {
        match log_tab_event {
            LogTabEvent::ToggleHeadMark => {
                self.log_panel.toggle_head_mark();
                self.sync_head_output();
            }
            LogTabEvent::Duplicate => {
                return Ok(Some(AppAction::Run(Command::Duplicate(Revset::from(
                    &self.head.change_id,
                )))));
            }

            LogTabEvent::CreateNew { describe } => {
                return Ok(Some(command::ask_new_change_from_selection(
                    self.config.clone(),
                    &self.head,
                    &self.marked(),
                    describe,
                )));
            }
            LogTabEvent::Rebase => {
                return Ok(Some(command::rebase(&self.head)?));
            }
            LogTabEvent::Squash { ignore_immutable } => {
                return Ok(Some(command::ask_squash(
                    self.config.clone(),
                    &self.head,
                    ignore_immutable,
                )?));
            }
            LogTabEvent::EditChange { ignore_immutable } => {
                return Ok(Some(command::ask_edit(
                    self.config.clone(),
                    &self.head,
                    format!("Change: {}", self.head.change_id.as_str()),
                    ignore_immutable,
                )));
            }
            LogTabEvent::Abandon => {
                return Ok(Some(command::ask_abandon(
                    self.config.clone(),
                    &self.head,
                    self.marked(),
                )));
            }
            LogTabEvent::Absorb => {
                return Ok(Some(AppAction::Run(Command::Absorb(self.head.clone()))));
            }
            LogTabEvent::Describe => {
                return Ok(Some(command::describe(&self.head)?));
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
                return Ok(None);
            }
            LogTabEvent::SetBookmark => {
                return Ok(Some(command::set_bookmark(self.config.clone(), &self.head)));
            }
            LogTabEvent::OpenFiles => {
                return Ok(Some(AppAction::ViewFiles(self.head.clone())));
            }
            LogTabEvent::OpenEvolog => {
                return Ok(Some(AppAction::ViewEvolog(self.head.clone())));
            }
            LogTabEvent::CopyChangeId => {
                return Ok(Some(AppAction::Run(Command::Copy(
                    self.head.change_id.as_string(),
                ))));
            }
            LogTabEvent::CopyRev => {
                return Ok(Some(AppAction::Run(Command::Copy(
                    self.head.commit_id.as_str().to_owned(),
                ))));
            }
            LogTabEvent::Push(scope) => {
                return self.handle_push(scope);
            }
            LogTabEvent::Fetch { all_remotes } => {
                return Ok(Some(AppAction::Run(Command::Fetch { all_remotes })));
            }
            LogTabEvent::GotoParent => {
                return self.handle_goto_parent();
            }

            LogTabEvent::Unbound => {}
        };
        Ok(None)
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
        self.log_panel
            .scroll_relative(scroll.distance(self.log_panel.visible_heads()));
        self.sync_head_output();
        Ok(())
    }

    fn focus_current(&mut self) -> Result<()> {
        self.set_head(new_commander().get_current_head()?);
        Ok(())
    }

    fn open_context_menu(&self) -> Result<Option<AppAction>> {
        self.context_menu(self.log_panel.selected_position())
    }

    fn make_main_panel_help(&self) -> Vec<HelpItem> {
        self.keybinds.make_main_panel_help()
    }

    fn make_details_panel_help(&self) -> Vec<HelpItem> {
        self.details_keybinds.make_help()
    }
}

impl Component for LogTab<'_> {
    fn update(&mut self) -> Result<Option<AppAction>> {
        self.head_panel.update();

        Ok(None)
    }

    fn task_done(&mut self, result: TaskResult) -> Result<Option<AppAction>> {
        if let TaskSlot::CommitShow(_, request) = result.slot {
            self.head_panel.task_done(request, result.output);
        }
        Ok(None)
    }

    fn needs_periodic_redraw(&self) -> bool {
        self.head_panel.needs_periodic_redraw()
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

                let help = Paragraph::new(vec![self.revset_keybinds.hint("accept").into()])
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
                match self.revset_keybinds.match_event(key) {
                    PopupEvent::Accept => {
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
                    PopupEvent::Cancel => {
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

            match self.details_keybinds.match_event(key) {
                DetailsPanelEvent::Unbound => {}
                ev => {
                    self.head_panel.handle_event(ev);
                    return Ok(ComponentInputResult::Handled);
                }
            }

            return match self.keybinds.match_event(key) {
                // Not something the tab acts on here, so whoever else
                // wants the key is welcome to it.
                LogTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event)?.into()),
            };
        }

        Ok(ComponentInputResult::Handled)
    }

    fn input_mouse(&mut self, mouse: Mouse) -> Result<ComponentInputResult> {
        if self.pane_divider.handle_mouse(mouse, self.config.layout()) {
            return Ok(ComponentInputResult::Handled);
        }
        match route_mouse(mouse, &mut [&mut self.log_panel, &mut self.head_panel]) {
            MouseInput::Scroll(delta) => self.log_panel.scroll_relative(delta),
            MouseInput::Select(index) => {
                if let Some(head) = self.log_panel.head_at_log_line(index) {
                    self.log_panel.set_head_in_place(head);
                }
            }
            // The press before this one selected the change, so all that
            // is left to do is mark it.
            MouseInput::Activate => self.log_panel.toggle_head_mark(),
            // The graph takes lines of its own, which name no change
            // for a menu to act on.
            MouseInput::Context(index) => {
                if let Some(head) = self.log_panel.head_at_log_line(index) {
                    self.log_panel.set_head_in_place(head);
                    self.sync_head_output();
                    return Ok(self.context_menu(Some(mouse.position()))?.into());
                }
            }
            MouseInput::Copy(text) => return Ok(copy_marked(text)),
            MouseInput::Handled => {}
            MouseInput::NotHandled => return Ok(ComponentInputResult::NotHandled),
        }
        self.sync_head_output();
        Ok(ComponentInputResult::Handled)
    }
}
