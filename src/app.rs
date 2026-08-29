pub mod command;
mod repo_watch;

use core::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use ratatui::crossterm::event::Event as TermEvent;
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

use crate::app::repo_watch::Check;
use crate::app::repo_watch::Moment;
use crate::app::repo_watch::RepoWatch;
use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskOutput;
use crate::background_tasks::TaskResult;
use crate::background_tasks::TaskSlot;
use crate::commander::ids::OperationId;
use crate::commander::new_commander;
use crate::env::get_env;
use crate::event::AppEvent;
use crate::event::EventSource;
use crate::keybinds::GlobalEvent;
use crate::keybinds::GlobalKeybinds;
use crate::keybinds::PopupEvent;
use crate::keybinds::PopupKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::bookmarks_tab::BookmarksTab;
use crate::ui::dialog::CommandPopup;
use crate::ui::dialog::HelpPopup;
use crate::ui::evolog_tab::EvologTab;
use crate::ui::files_tab::FilesTab;
use crate::ui::log_tab::LogTab;

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum TabId {
    Log,
    Files,
    Bookmarks,
    Evolog,
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TabId::Log => write!(f, "Log"),
            TabId::Files => write!(f, "Files"),
            TabId::Bookmarks => write!(f, "Bookmarks"),
            TabId::Evolog => write!(f, "Evolog"),
        }
    }
}

impl TabId {
    pub const VALUES: [Self; 4] = [TabId::Log, TabId::Files, TabId::Bookmarks, TabId::Evolog];
}

pub struct Stats {
    pub start_time: Instant,
}

/// What handling an event leaves for the main loop to do.
pub enum Handled {
    /// Nothing the app shows can have changed.
    Nothing,
    /// What the app shows may have changed.
    Redraw,
    /// The app was asked to stop.
    Stop,
}

pub struct App<'a> {
    // user interface
    pub current_tab: TabId,
    pub log: LogTab<'a>,
    pub files: FilesTab,
    pub bookmarks: BookmarksTab,
    pub evolog: EvologTab<'a>,
    pub popup: Option<Box<dyn Component>>,
    pub stats: Stats,
    global_keybinds: GlobalKeybinds,
    /// The keys a popup that does not answer to them itself is taken
    /// down by
    popup_keybinds: PopupKeybinds,

    repo_watch: RepoWatch,

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
            files: FilesTab::new(&current_head, background_tasks.clone()),
            bookmarks: BookmarksTab::new(background_tasks.clone()),
            evolog: EvologTab::new(&current_head, background_tasks.clone()),
            popup: None,
            stats: Stats {
                start_time: Instant::now(),
            },
            global_keybinds,
            popup_keybinds: PopupKeybinds::dialog(),

            repo_watch: RepoWatch::new(get_env().jj_config.poll_interval(), Instant::now()),

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
        // Asking for the tab already on screen is not asking for it to
        // move.
        if tab == self.current_tab {
            return;
        }

        info!("Setting tab to {}", tab);
        self.current_tab = tab;
        // The user is not reading the tab they are switching to yet, so
        // nothing moves under them if we bring it up to date.
        self.repo_watch.catching_up();
    }

    /// How long until the app next checks for work done outside it, or
    /// None if there is nothing to wake up for.
    pub fn time_until_poll(&self) -> Option<Duration> {
        self.repo_watch.time_until_poll(self.moment())
    }

    fn moment(&self) -> Moment {
        Moment {
            at: Instant::now(),
            checking: self.background_tasks.is_running(&TaskSlot::RepoOpId),
            room: self.background_tasks.has_room(),
        }
    }

    /// Start a check of what the repo is at if one is called for, and
    /// catch the current tab up unless refreshing it now would move what
    /// the user is reading. Returns whether anything on screen changed.
    pub fn refresh_view(&mut self) -> Result<bool> {
        // A popup covers the tab a check would read for, and keeps the
        // loop running for its own sake.
        if self.popup.is_some() {
            return Ok(false);
        }

        if let Some(check) = self.repo_watch.check_to_start(self.moment()) {
            self.submit_repo_check(check);
        }

        let stale = self.get_current_tab().is_stale();
        let hint_changed = self.repo_watch.leave_stale(stale);
        if self.repo_watch.waiting_for_refresh() || !stale {
            return Ok(hint_changed);
        }

        self.get_current_tab().refresh()?;

        Ok(true)
    }

    /// Read what operation the repo is at, keeping the slot until the
    /// check is done rather than letting newer work kill it.
    fn submit_repo_check(&mut self, check: Check) {
        self.background_tasks
            .submit_uninterruptible(TaskSlot::RepoOpId, move || {
                let mut commander = new_commander();
                if !check.snapshot {
                    commander.ignore_working_copy();
                }
                Ok(commander.get_operation_id()?.0)
            });
    }

    /// Take what a check found and mark every tab stale if the repo has
    /// moved since the last one.
    fn repo_checked(&mut self, output: TaskOutput) {
        let op_id = match output {
            Ok(op_id) => Some(OperationId(op_id)),
            Err(err) => {
                warn!("Could not read what the repo is at: {err}");
                None
            }
        };

        if self.repo_watch.checked(Instant::now(), op_id) {
            trace!("The repo has moved, so every tab is stale");
            self.mark_all_stale();
        }
    }

    /// Every tab is behind on what it shows, whoever moved the repo.
    fn mark_all_stale(&mut self) {
        for tab in TabId::VALUES {
            self.get_tab(tab).mark_stale();
        }
    }

    pub fn get_tab(&mut self, tab: TabId) -> &mut dyn Tab {
        match tab {
            TabId::Log => &mut self.log,
            TabId::Files => &mut self.files,
            TabId::Bookmarks => &mut self.bookmarks,
            TabId::Evolog => &mut self.evolog,
        }
    }

    /// When a component wants the app to do something,
    /// it sends a AppAction which the App handles.
    pub fn handle_action(&mut self, app_action: AppAction) -> Result<()> {
        match app_action {
            AppAction::ViewFiles(head) => {
                self.set_tab(TabId::Files);
                self.files.set_head(&head);
            }
            AppAction::ViewVersionFiles(version) => {
                self.set_tab(TabId::Files);
                self.files.set_version(&version);
            }
            AppAction::ViewEvolog(head) => {
                self.set_tab(TabId::Evolog);
                self.evolog.set_head(&head);
            }
            AppAction::ViewLog(head) => {
                self.log.set_head(head);
                self.set_tab(TabId::Log);
            }
            AppAction::ViewBookmark(name) => {
                self.set_tab(TabId::Bookmarks);
                self.bookmarks.select_bookmark(&name);
            }
            AppAction::ChangeHead(head) => {
                self.files.set_head(&head);
                self.evolog.set_head(&head);
            }
            AppAction::SetPopup(popup) => {
                self.popup = Some(popup);
            }
            AppAction::ClosePopup => {
                self.popup = None;
            }
            AppAction::Multiple(app_actions) => {
                for app_action in app_actions.into_iter() {
                    self.handle_action(app_action)?;
                }
            }
            AppAction::ClearLogMarks => {
                self.log.clear_marks();
            }
            AppAction::Run(command) => {
                if let Some(app_action) = command.run(&self.background_tasks)? {
                    self.handle_action(app_action)?;
                }
            }
            AppAction::MarkTabsStale => {
                self.mark_all_stale();
                // We moved the repo ourselves and snapshotted while at
                // it, so the check is only there to keep the operation
                // id we compare against up to date.
                self.repo_watch.ask_check(Check {
                    snapshot: false,
                    ours: true,
                });
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
            // The app is not going to pick it up, so light up the key
            // that does.
            let refresh_style = if self.repo_watch.waiting_for_refresh() {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };

            let hints = Paragraph::new(Line::from(vec![
                Span::raw("q: quit | ?: help | "),
                Span::styled("R: refresh", refresh_style),
                Span::raw(" | 1-4: change tab"),
            ]))
            .fg(Color::DarkGray)
            .block(
                Block::bordered()
                    .title(" blazingjj ")
                    .border_type(BorderType::Rounded)
                    .fg(Color::default()),
            );

            f.render_widget(hints, header_chunks[1]);
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
    fn handle_task_result(&mut self, result: TaskResult) -> Result<Handled> {
        self.background_tasks.finish(&result);

        let consumer: Option<&mut dyn Component> = match result.slot {
            // The check is the app's own, and puts nothing on screen
            // itself.
            TaskSlot::RepoOpId => {
                self.repo_checked(result.output);
                return Ok(Handled::Nothing);
            }
            TaskSlot::CommitShow(tab, _)
            | TaskSlot::FileDiff(tab, _)
            | TaskSlot::EvologShow(tab, _) => Some(self.get_tab(tab)),
            // The cast reborrows the popup for the body rather than for
            // the lifetime the app is tied to.
            TaskSlot::GitPush | TaskSlot::GitFetch => self
                .popup
                .as_deref_mut()
                .map(|popup| popup as &mut dyn Component),
        };
        let Some(consumer) = consumer else {
            trace!("Dropping task result, its consumer is gone");
            return Ok(Handled::Nothing);
        };

        if let Some(app_action) = consumer.task_done(result)? {
            self.handle_action(app_action)?;
        }
        Ok(Handled::Redraw)
    }

    /// Process an AppEvent
    #[instrument(level = "trace", skip(self))]
    pub fn input(&mut self, event: AppEvent) -> Result<Handled> {
        let event = match event {
            AppEvent::UserInput(event) => event,
            AppEvent::TaskDone(result) => {
                trace!("Processing task result");
                return self.handle_task_result(result);
            }
        };
        trace!("Processing user input");

        match event {
            // Coming back to the window is worth a check, as the repo
            // may well have moved while we were not being watched.
            event::Event::FocusGained => {
                self.repo_watch.set_focus(true);
                self.repo_watch.ask_check(Check {
                    snapshot: true,
                    ours: false,
                });
                return Ok(Handled::Nothing);
            }
            event::Event::FocusLost => {
                self.repo_watch.set_focus(false);
                return Ok(Handled::Nothing);
            }
            _ => {}
        }

        let mut to_tab = self.popup.is_none();
        if let Some(popup) = self.popup.as_mut() {
            match popup.input(event.clone())? {
                ComponentInputResult::HandledAction(app_action) => {
                    self.handle_action(app_action)?
                }
                ComponentInputResult::Handled => {}
                ComponentInputResult::Dismissed => {
                    self.popup = None;
                    to_tab = true;
                }
                ComponentInputResult::NotHandled => {
                    if let TermEvent::Key(key) = event
                        && key.kind == event::KeyEventKind::Press
                        && matches!(
                            self.popup_keybinds.match_event(key),
                            PopupEvent::Accept | PopupEvent::Cancel
                        )
                    {
                        self.popup = None
                    }
                }
            };
        }

        if to_tab {
            match self.get_current_tab().input(event.clone())? {
                ComponentInputResult::HandledAction(app_action) => {
                    self.handle_action(app_action)?
                }
                // A tab is never on top of anything, so it has nothing
                // to dismiss itself in favour of.
                ComponentInputResult::Handled | ComponentInputResult::Dismissed => {}
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
                                // The tabs that read what they show when
                                // they show it are now out of date at our
                                // asking, not at the repo's.
                                self.repo_watch.catching_up();
                            }
                            GlobalEvent::Refresh => {
                                self.repo_watch.ask_check(Check {
                                    snapshot: true,
                                    ours: true,
                                });
                                self.get_current_tab().drop_caches();
                                // The check above is the one we want, so
                                // there is nothing left but to have every
                                // tab read itself again.
                                self.mark_all_stale();
                            }
                            GlobalEvent::NextTab => self.set_next_tab_with_offset(1),
                            GlobalEvent::PrevTab => self.set_next_tab_with_offset(-1),
                            GlobalEvent::LogTab => self.set_tab(TabId::Log),
                            GlobalEvent::FilesTab => self.set_tab(TabId::Files),
                            GlobalEvent::BookmarksTab => self.set_tab(TabId::Bookmarks),
                            GlobalEvent::EvologTab => self.set_tab(TabId::Evolog),
                            GlobalEvent::CommandPopup => {
                                self.popup = Some(Box::new(CommandPopup::new()));
                            }
                            GlobalEvent::OpenHelp => self.open_help()?,
                            GlobalEvent::Quit => {
                                self.running.store(false, Ordering::Relaxed);
                                return Ok(Handled::Stop);
                            }
                            GlobalEvent::Unbound => {}
                        }
                    }
                }
            };
        }

        Ok(Handled::Redraw)
    }
}
