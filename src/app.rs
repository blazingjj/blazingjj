use core::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use ratatui::crossterm::event::Event as TermEvent;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::{self};
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::prelude::*;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::symbols;
use ratatui::widgets::*;
use tracing::info;
use tracing::instrument;
use tracing::trace;
use tracing::warn;

use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskResult;
use crate::background_tasks::TaskSlot;
use crate::commander::ids::OperationId;
use crate::commander::new_commander;
use crate::env::get_env;
use crate::event::AppEvent;
use crate::event::EventSource;
use crate::keybinds::GlobalEvent;
use crate::keybinds::GlobalKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::bookmarks_tab::BookmarksTab;
use crate::ui::dialog::CommandPopup;
use crate::ui::dialog::HelpPopup;
use crate::ui::files_tab::FilesTab;
use crate::ui::log_tab::LogTab;

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum TabId {
    Log,
    Files,
    Bookmarks,
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TabId::Log => write!(f, "Log"),
            TabId::Files => write!(f, "Files"),
            TabId::Bookmarks => write!(f, "Bookmarks"),
        }
    }
}

impl TabId {
    pub const VALUES: [Self; 3] = [TabId::Log, TabId::Files, TabId::Bookmarks];
}

pub struct Stats {
    pub start_time: Instant,
}

/// How long after a check of what the repo is at the next one is due.
const OP_ID_CHECK_INTERVAL: Duration = Duration::from_secs(1);

pub struct App<'a> {
    // user interface
    pub current_tab: TabId,
    pub log: LogTab<'a>,
    pub files: FilesTab,
    pub bookmarks: BookmarksTab,
    pub popup: Option<Box<dyn Component>>,
    pub stats: Stats,
    global_keybinds: GlobalKeybinds,

    repo_op_id: Option<OperationId>,
    next_op_id_check: Instant,

    /// Whether the next check may leave the working copy alone rather
    /// than snapshotting it.
    ignore_working_copy: bool,

    // event handling
    running: Arc<AtomicBool>,
    event_source: EventSource,
    background_tasks: BackgroundTasks,
}

impl<'a> App<'a> {
    pub fn new() -> Result<App<'a>> {
        let running = Arc::from(AtomicBool::new(true));
        let event_source = EventSource::new(running.clone());
        let background_tasks = BackgroundTasks::new(event_source.clone_event_sender());
        let mut global_keybinds = GlobalKeybinds::default();
        if let Some(keybinds_config) = get_env().jj_config.keybinds() {
            global_keybinds.extend_from_config(keybinds_config);
        }
        let current_head = new_commander().get_current_head()?;

        Ok(App {
            current_tab: TabId::Log,
            log: LogTab::new(background_tasks.clone(), current_head.clone()),
            files: FilesTab::new(&current_head),
            bookmarks: BookmarksTab::new(background_tasks.clone()),
            popup: None,
            stats: Stats {
                start_time: Instant::now(),
            },
            global_keybinds,

            repo_op_id: None,
            next_op_id_check: Instant::now(),
            ignore_working_copy: false,

            running,
            event_source,
            background_tasks,
        })
    }

    pub fn get_current_tab(&mut self) -> &mut dyn Tab {
        self.get_tab(self.current_tab)
    }

    pub fn set_next_tab_with_offset(&mut self, offset: i64) {
        let current_index = TabId::VALUES
            .iter()
            .position(|&t| t == self.current_tab)
            .unwrap();
        let new_index = (current_index as i64 + TabId::VALUES.len() as i64 + offset) as usize
            % TabId::VALUES.len();
        let new_tab: TabId = TabId::VALUES[new_index];
        self.set_tab(new_tab);
    }

    fn open_help(&mut self) -> Result<()> {
        let global_help = self.global_keybinds.make_help();
        let tab = self.get_current_tab();
        let popup = HelpPopup::new(
            tab.make_main_panel_help(),
            tab.make_details_panel_help(),
            global_help,
        );
        self.popup = Some(Box::new(popup));
        Ok(())
    }

    pub fn set_tab(&mut self, tab: TabId) {
        info!("Setting tab to {}", tab);
        self.current_tab = tab;
    }

    /// Mark every tab stale if the repo has moved since the last check,
    /// then read the current tab if it is stale. Returns whether
    /// anything on screen changed.
    pub fn refresh_view(&mut self) -> Result<bool> {
        if self.check_repo_moved() {
            trace!("The repo has moved, so every tab is stale");
            for tab in TabId::VALUES {
                self.get_tab(tab).mark_stale();
            }
        }

        if !self.get_current_tab().is_stale() {
            return Ok(false);
        }

        self.get_current_tab().refresh()?;

        Ok(true)
    }

    /// Ask for the next check to read the repo without waiting out
    /// [OP_ID_CHECK_INTERVAL], and snapshot the working copy while at it.
    fn request_check_with_snapshot(&mut self) {
        self.next_op_id_check = Instant::now();
        self.ignore_working_copy = false;
    }

    /// Read what operation the repo is at once the last check is an
    /// [OP_ID_CHECK_INTERVAL] old, or as soon as one was asked for, and
    /// remember it. Reports whether it differs from what was remembered
    /// before; a check that is not due yet or that fails reports no
    /// movement.
    fn check_repo_moved(&mut self) -> bool {
        if Instant::now() < self.next_op_id_check {
            return false;
        }

        let mut commander = new_commander();
        if self.ignore_working_copy {
            commander.ignore_working_copy();
        }

        let result = commander.get_operation_id();
        self.next_op_id_check = Instant::now() + OP_ID_CHECK_INTERVAL;

        let operation_id = match result {
            Ok(operation_id) => operation_id,
            Err(err) => {
                warn!("Could not read what the repo is at: {err}");
                return false;
            }
        };
        self.ignore_working_copy = true;

        let moved = self.repo_op_id.as_ref() != Some(&operation_id);
        self.repo_op_id = Some(operation_id);
        moved
    }

    pub fn get_tab(&mut self, tab: TabId) -> &mut dyn Tab {
        match tab {
            TabId::Log => &mut self.log,
            TabId::Files => &mut self.files,
            TabId::Bookmarks => &mut self.bookmarks,
        }
    }

    /// When a component wants the app to do something,
    /// it sends a AppAction which the App handles.
    pub fn handle_action(&mut self, app_action: AppAction) -> Result<()> {
        match app_action {
            AppAction::ViewFiles(head) => {
                self.set_tab(TabId::Files);
                self.files.set_head(&head)?;
            }
            AppAction::ViewLog(head) => {
                self.log.set_head(head);
                self.set_tab(TabId::Log);
            }
            AppAction::ChangeHead(head) => {
                self.files.set_head(&head)?;
            }
            AppAction::SetPopup(popup) => {
                self.popup = Some(popup);
            }
            AppAction::PopupDone => {
                self.popup = None;
                self.handle_action(AppAction::RefreshTab)?;
            }
            AppAction::PopupCanceled => {
                self.popup = None;
            }
            AppAction::Multiple(app_actions) => {
                for app_action in app_actions.into_iter() {
                    self.handle_action(app_action)?;
                }
            }
            AppAction::RefreshTab => {
                self.get_current_tab().mark_stale();
                // Whatever asks for this has likely moved the repo, so
                // the other tabs want checking without delay.
                self.next_op_id_check = Instant::now();
            }
        }

        Ok(())
    }

    /// Returns whether anything that shows may have changed.
    #[instrument(level = "trace", skip(self))]
    pub fn update(&mut self) -> Result<bool> {
        let mut changed = self.needs_periodic_redraw();

        if let Some(popup) = self.popup.as_mut()
            && let Some(component_action) = popup.update()?
        {
            self.handle_action(component_action)?;
        }

        if let Some(component_action) = self.get_current_tab().update()? {
            self.handle_action(component_action)?;
            changed = true;
        }

        Ok(changed)
    }

    #[instrument(level = "trace", skip(self, f))]
    pub fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(area);

        let header_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);

        {
            let tabs = Tabs::new(
                TabId::VALUES
                    .iter()
                    .enumerate()
                    .map(|(i, tab)| format!("[{}] {}", i + 1, tab)),
            )
            .block(
                Block::bordered()
                    .title(" Tabs ")
                    .border_type(BorderType::Rounded),
            )
            .highlight_style(Style::default().bg(get_env().jj_config.highlight_color()))
            .select(
                TabId::VALUES
                    .iter()
                    .position(|tab| tab == &self.current_tab)
                    .unwrap_or(0),
            )
            .divider(symbols::line::VERTICAL);

            f.render_widget(tabs, header_chunks[0]);
        }
        {
            let tabs = Paragraph::new("q: quit | ?: help | R: refresh | 1/2/3: change tab")
                .fg(Color::DarkGray)
                .block(
                    Block::bordered()
                        .title(" blazingjj ")
                        .border_type(BorderType::Rounded)
                        .fg(Color::default()),
                );

            f.render_widget(tabs, header_chunks[1]);
        }

        self.get_current_tab().draw(f, chunks[1])?;

        if let Some(popup) = self.popup.as_mut() {
            popup.draw(f, area)?;
        }

        {
            let paragraph =
                Paragraph::new(format!("{}ms", self.stats.start_time.elapsed().as_millis()))
                    .alignment(Alignment::Right);
            let position = Rect {
                x: 0,
                y: 1,
                height: 1,
                width: area.width - 1,
            };
            f.render_widget(paragraph, position);
        }

        Ok(())
    }

    /// Set up threads that capture input and send AppEvents
    pub fn launch_input_channel(&mut self) {
        self.event_source.launch_user_input();
    }

    /// Recieve an AppEvent if one is waiting
    pub fn try_recv_app_event(&self, timeout: Duration) -> Option<AppEvent> {
        self.event_source.try_recv(timeout)
    }

    /// Whether something on screen counts up on its own, so that the main
    /// loop has to come back on a timer rather than only on an event.
    pub fn needs_periodic_redraw(&mut self) -> bool {
        self.popup.is_some() || self.get_current_tab().is_waiting()
    }

    /// Hand the output of a finished task to whoever asked for it
    fn handle_task_result(&mut self, result: TaskResult) -> Result<()> {
        self.background_tasks.finish(&result);

        let consumer: &mut dyn Component = match result.slot {
            TaskSlot::CommitShow(tab, _) => self.get_tab(tab),
        };

        if let Some(app_action) = consumer.task_done(result)? {
            self.handle_action(app_action)?;
        }
        Ok(())
    }

    /// Process an AppEvent
    #[instrument(level = "trace", skip(self))]
    pub fn input(&mut self, event: AppEvent) -> Result<bool> {
        let event = match event {
            AppEvent::UserInput(event) => event,
            AppEvent::TaskDone(result) => {
                trace!("Processing task result");
                self.handle_task_result(result)?;
                return Ok(false); // do not terminate the app
            }
        };
        trace!("Processing user input");

        // Coming back to the window is worth a check, as the repo may
        // well have moved while we were not being watched.
        if event == event::Event::FocusGained {
            self.request_check_with_snapshot();
            return Ok(false);
        }

        if let Some(popup) = self.popup.as_mut() {
            match popup.input(event.clone())? {
                ComponentInputResult::HandledAction(app_action) => {
                    self.handle_action(app_action)?
                }
                ComponentInputResult::Handled => {}
                ComponentInputResult::NotHandled => {
                    if let TermEvent::Key(key) = event
                        && key.kind == event::KeyEventKind::Press
                    {
                        // Close
                        if matches!(
                            key.code,
                            KeyCode::Char('y')
                                | KeyCode::Char('n')
                                | KeyCode::Char('o')
                                | KeyCode::Enter
                                | KeyCode::Char('q')
                                | KeyCode::Esc
                        ) {
                            self.popup = None
                        }
                    }
                }
            };
        } else {
            match self.get_current_tab().input(event.clone())? {
                ComponentInputResult::HandledAction(app_action) => {
                    self.handle_action(app_action)?
                }
                ComponentInputResult::Handled => {}
                ComponentInputResult::NotHandled => {
                    if let TermEvent::Key(key) = event
                        && key.kind == event::KeyEventKind::Press
                    {
                        match self.global_keybinds.match_event(key) {
                            GlobalEvent::ScrollDown => {
                                self.get_current_tab().scroll_main_panel(Scroll::Down)?;
                            }
                            GlobalEvent::ScrollUp => {
                                self.get_current_tab().scroll_main_panel(Scroll::Up)?;
                            }
                            GlobalEvent::ScrollDownHalf => {
                                self.get_current_tab()
                                    .scroll_main_panel(Scroll::DownHalfPage)?;
                            }
                            GlobalEvent::ScrollUpHalf => {
                                self.get_current_tab()
                                    .scroll_main_panel(Scroll::UpHalfPage)?;
                            }
                            GlobalEvent::FocusCurrent => {
                                self.get_current_tab().focus_current()?;
                            }
                            GlobalEvent::Refresh => {
                                self.request_check_with_snapshot();
                                self.get_current_tab().drop_caches();
                                self.handle_action(AppAction::RefreshTab)?;
                            }
                            GlobalEvent::NextTab => self.set_next_tab_with_offset(1),
                            GlobalEvent::PrevTab => self.set_next_tab_with_offset(-1),
                            GlobalEvent::LogTab => self.set_tab(TabId::Log),
                            GlobalEvent::FilesTab => self.set_tab(TabId::Files),
                            GlobalEvent::BookmarksTab => self.set_tab(TabId::Bookmarks),
                            GlobalEvent::CommandPopup => {
                                self.popup = Some(Box::new(CommandPopup::new()));
                            }
                            GlobalEvent::OpenHelp => self.open_help()?,
                            GlobalEvent::Quit => {
                                self.running.store(false, Ordering::Relaxed);
                                return Ok(true);
                            }
                            GlobalEvent::Unbound => {}
                        }
                    }
                }
            };
        }

        Ok(false)
    }
}
