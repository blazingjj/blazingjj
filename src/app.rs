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
use crate::ui::bookmarks_tab::BookmarksTab;
use crate::ui::dialog::CommandPopup;
use crate::ui::dialog::HelpPopup;
use crate::ui::files_tab::FilesTab;
use crate::ui::log_tab::LogTab;

#[derive(PartialEq, Copy, Clone)]
pub enum Tab {
    Log,
    Files,
    Bookmarks,
}

impl fmt::Display for Tab {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Tab::Log => write!(f, "Log"),
            Tab::Files => write!(f, "Files"),
            Tab::Bookmarks => write!(f, "Bookmarks"),
        }
    }
}

impl Tab {
    pub const VALUES: [Self; 3] = [Tab::Log, Tab::Files, Tab::Bookmarks];
}

pub struct Stats {
    pub start_time: Instant,
}

pub struct App<'a> {
    // user interface
    pub current_tab: Tab,
    pub log: LogTab<'a>,
    pub files: FilesTab,
    pub bookmarks: BookmarksTab<'a>,
    pub popup: Option<Box<dyn Component>>,
    pub stats: Stats,
    global_keybinds: GlobalKeybinds,

    // event handling
    running: Arc<AtomicBool>,
    event_source: EventSource,
}

impl<'a> App<'a> {
    pub fn new() -> Result<App<'a>> {
        let running = Arc::from(AtomicBool::new(true));
        let event_source = EventSource::new(running.clone());
        let mut global_keybinds = GlobalKeybinds::default();
        if let Some(keybinds_config) = get_env().jj_config.keybinds() {
            global_keybinds.extend_from_config(keybinds_config);
        }
        let current_head = new_commander().get_current_head()?;

        Ok(App {
            current_tab: Tab::Log,
            log: LogTab::new(event_source.clone_event_sender())?,
            files: FilesTab::new(&current_head)?,
            bookmarks: BookmarksTab::new()?,
            popup: None,
            stats: Stats {
                start_time: Instant::now(),
            },
            global_keybinds,

            running,
            event_source,
        })
    }

    pub fn get_current_tab(&mut self) -> &mut dyn Component {
        self.get_tab(self.current_tab)
    }

    pub fn set_next_tab_with_offset(&mut self, offset: i64) -> Result<()> {
        let current_index = Tab::VALUES
            .iter()
            .position(|&t| t == self.current_tab)
            .unwrap();
        let new_index =
            (current_index as i64 + Tab::VALUES.len() as i64 + offset) as usize % Tab::VALUES.len();
        let new_tab: Tab = Tab::VALUES[new_index];
        self.set_tab(new_tab)
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

    pub fn set_tab(&mut self, tab: Tab) -> Result<()> {
        info!("Setting tab to {}", tab);
        self.current_tab = tab;
        self.get_current_tab().refresh()?;
        Ok(())
    }

    pub fn get_tab(&mut self, tab: Tab) -> &mut dyn Component {
        match tab {
            Tab::Log => &mut self.log,
            Tab::Files => &mut self.files,
            Tab::Bookmarks => &mut self.bookmarks,
        }
    }

    /// When a component wants the app to do something,
    /// it sends a AppAction which the App handles.
    pub fn handle_action(&mut self, app_action: AppAction) -> Result<()> {
        match app_action {
            AppAction::ViewFiles(head) => {
                self.set_tab(Tab::Files)?;
                self.files.set_head(&head)?;
            }
            AppAction::ViewLog(head) => {
                self.log.set_head(head);
                self.set_tab(Tab::Log)?;
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
                self.set_tab(self.current_tab)?;
            }
        }

        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    pub fn update(&mut self) -> Result<()> {
        if let Some(popup) = self.popup.as_mut()
            && let Some(component_action) = popup.update()?
        {
            self.handle_action(component_action)?;
        }

        if let Some(component_action) = self.get_current_tab().update()? {
            self.handle_action(component_action)?;
        }

        Ok(())
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
                Tab::VALUES
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
                Tab::VALUES
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
    pub fn try_recv_app_event(&mut self, timeout: Duration) -> Option<AppEvent> {
        self.event_source.try_recv(timeout)
    }

    /// Process an AppEvent
    #[instrument(level = "trace", skip(self))]
    pub fn input(&mut self, event: AppEvent) -> Result<bool> {
        let ev_name = match event {
            AppEvent::UserInput(_) => "AppEvent",
            AppEvent::Refresh => "Refresh",
        };
        trace!("Processing event {}", ev_name);

        let AppEvent::UserInput(event) = event else {
            trace!("an event that was not a UserInput was ignored");
            return Ok(false); // do not terminate the app
        };
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
        } else if event == event::Event::FocusGained {
            self.get_current_tab().refresh()?;
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
                                self.get_current_tab().drop_caches();
                                self.handle_action(AppAction::RefreshTab)?;
                            }
                            GlobalEvent::NextTab => self.set_next_tab_with_offset(1)?,
                            GlobalEvent::PrevTab => self.set_next_tab_with_offset(-1)?,
                            GlobalEvent::LogTab => self.set_tab(Tab::Log)?,
                            GlobalEvent::FilesTab => self.set_tab(Tab::Files)?,
                            GlobalEvent::BookmarksTab => self.set_tab(Tab::Bookmarks)?,
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
