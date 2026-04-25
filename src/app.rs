use core::fmt;
use std::time::Instant;

use anyhow::Result;
use anyhow::anyhow;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyModifiers;
use ratatui::crossterm::event::{self};
use tracing::info;
use tracing::instrument;

use crate::ComponentInputResult;
use crate::commander::new_commander;
use crate::ui::Component;
use crate::ui::ComponentAction;
use crate::ui::bookmarks_tab::BookmarksTab;
use crate::ui::command_popup::CommandPopup;
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
    pub current_tab: Tab,
    pub log: Option<LogTab<'a>>,
    pub files: Option<FilesTab>,
    pub bookmarks: Option<BookmarksTab<'a>>,
    pub popup: Option<Box<dyn Component>>,
    pub stats: Stats,
}

impl<'a> App<'a> {
    pub fn new() -> Result<App<'a>> {
        Ok(App {
            current_tab: Tab::Log,
            log: None,
            files: None,
            bookmarks: None,
            popup: None,
            stats: Stats {
                start_time: Instant::now(),
            },
        })
    }

    pub fn get_or_init_current_tab(&mut self) -> Result<&mut dyn Component> {
        self.get_or_init_tab(self.current_tab)
    }
    pub fn get_current_tab(&mut self) -> Option<&mut dyn Component> {
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

    pub fn set_tab(&mut self, tab: Tab) -> Result<()> {
        info!("Setting tab to {}", tab);
        self.current_tab = tab;
        self.get_or_init_current_tab()?.focus()?;
        Ok(())
    }

    pub fn get_log_tab(&mut self) -> Result<&mut LogTab<'a>> {
        if self.log.is_none() {
            self.log = Some(LogTab::new()?);
        }

        self.log
            .as_mut()
            .ok_or_else(|| anyhow!("Failed to get mutable reference to LogTab"))
    }

    pub fn get_files_tab(&mut self) -> Result<&mut FilesTab> {
        if self.files.is_none() {
            let current_head = new_commander().get_current_head()?;
            self.files = Some(FilesTab::new(&current_head)?);
        }

        self.files
            .as_mut()
            .ok_or_else(|| anyhow!("Failed to get mutable reference to FilesTab"))
    }

    pub fn get_bookmarks_tab(&mut self) -> Result<&mut BookmarksTab<'a>> {
        if self.bookmarks.is_none() {
            self.bookmarks = Some(BookmarksTab::new()?);
        }

        self.bookmarks
            .as_mut()
            .ok_or_else(|| anyhow!("Failed to get mutable reference to BookmarksTab"))
    }

    pub fn get_or_init_tab(&mut self, tab: Tab) -> Result<&mut dyn Component> {
        Ok(match tab {
            Tab::Log => self.get_log_tab()?,
            Tab::Files => self.get_files_tab()?,
            Tab::Bookmarks => self.get_bookmarks_tab()?,
        })
    }

    pub fn get_tab(&mut self, tab: Tab) -> Option<&mut dyn Component> {
        match tab {
            Tab::Log => self
                .log
                .as_mut()
                .map(|log_tab| log_tab as &mut dyn Component),
            Tab::Files => self
                .files
                .as_mut()
                .map(|files_tab| files_tab as &mut dyn Component),
            Tab::Bookmarks => self
                .bookmarks
                .as_mut()
                .map(|bookmarks_tab| bookmarks_tab as &mut dyn Component),
        }
    }

    pub fn handle_action(&mut self, component_action: ComponentAction) -> Result<()> {
        match component_action {
            ComponentAction::ViewFiles(head) => {
                self.set_tab(Tab::Files)?;
                self.get_files_tab()?.set_head(&head)?;
            }
            ComponentAction::ViewLog(head) => {
                self.get_log_tab()?.set_head(head);
                self.set_tab(Tab::Log)?;
            }
            ComponentAction::ChangeHead(head) => {
                self.get_files_tab()?.set_head(&head)?;
            }
            ComponentAction::SetPopup(popup) => {
                self.popup = popup;
            }
            ComponentAction::Multiple(component_actions) => {
                for component_action in component_actions.into_iter() {
                    self.handle_action(component_action)?;
                }
            }
            ComponentAction::RefreshTab() => {
                self.set_tab(self.current_tab)?;
                if self.current_tab == Tab::Log {
                    let head = new_commander().get_current_head()?.clone();
                    self.get_log_tab()?.set_head(head);
                };
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

        if let Some(component_action) = self.get_or_init_current_tab()?.update()? {
            self.handle_action(component_action)?;
        }

        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    pub fn input(&mut self, event: Event) -> Result<bool> {
        if let Some(popup) = self.popup.as_mut() {
            match popup.input(event.clone())? {
                ComponentInputResult::HandledAction(component_action) => {
                    self.handle_action(component_action)?
                }
                ComponentInputResult::Handled => {}
                ComponentInputResult::NotHandled => {
                    if let Event::Key(key) = event
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
            self.get_or_init_current_tab()?.focus()?;
        } else {
            match self.get_or_init_current_tab()?.input(event.clone())? {
                ComponentInputResult::HandledAction(component_action) => {
                    self.handle_action(component_action)?
                }
                ComponentInputResult::Handled => {}
                ComponentInputResult::NotHandled => {
                    if let Event::Key(key) = event
                        && key.kind == event::KeyEventKind::Press
                    {
                        // Close
                        if key.code == KeyCode::Char('q')
                            || (key.modifiers.contains(KeyModifiers::CONTROL)
                                && (key.code == KeyCode::Char('c')))
                            || key.code == KeyCode::Esc
                        {
                            return Ok(true);
                        }
                        //
                        // Tab switching
                        else if key.code == KeyCode::Char('l') {
                            self.set_next_tab_with_offset(1)?;
                        } else if key.code == KeyCode::Char('h') {
                            self.set_next_tab_with_offset(-1)?;
                        } else if let Some((_, tab)) =
                            Tab::VALUES.iter().enumerate().find(|(i, _)| {
                                key.code
                                    == KeyCode::Char(
                                        char::from_digit((*i as u32) + 1u32, 10)
                                            .expect("Tab index could not be converted to digit"),
                                    )
                            })
                        {
                            self.set_tab(*tab)?;
                        }
                        // General jj command runner
                        else if key.code == KeyCode::Char(':') {
                            self.popup = Some(Box::new(CommandPopup::new()));
                        }
                    }
                }
            };
        }

        Ok(false)
    }
}
