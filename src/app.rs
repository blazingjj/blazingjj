use core::fmt;
use std::time::Instant;

use anyhow::Result;
use anyhow::anyhow;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::{self};
use tracing::info;
use tracing::instrument;

use crate::ComponentInputResult;
use crate::commander::new_commander;
use crate::env::JJLayout;
use crate::env::get_env;
use crate::keybinds::GlobalEvent;
use crate::keybinds::GlobalKeybinds;
use crate::ui::Component;
use crate::ui::ComponentAction;
use crate::ui::bookmarks_tab::BookmarksTab;
use crate::ui::command_popup::CommandPopup;
use crate::ui::files_tab::FilesTab;
use crate::ui::help_popup::HelpPopup;
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
    layout: JJLayout,
    global_keybinds: GlobalKeybinds,
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
            layout: get_env().jj_config.layout(),
            global_keybinds: GlobalKeybinds::default(),
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
            let mut tab = LogTab::new()?;
            tab.set_layout(self.layout);
            self.log = Some(tab);
        }

        self.log
            .as_mut()
            .ok_or_else(|| anyhow!("Failed to get mutable reference to LogTab"))
    }

    pub fn get_files_tab(&mut self) -> Result<&mut FilesTab> {
        if self.files.is_none() {
            let current_head = new_commander().get_current_head()?;
            let mut tab = FilesTab::new(&current_head)?;
            tab.set_layout(self.layout);
            self.files = Some(tab);
        }

        self.files
            .as_mut()
            .ok_or_else(|| anyhow!("Failed to get mutable reference to FilesTab"))
    }

    pub fn get_bookmarks_tab(&mut self) -> Result<&mut BookmarksTab<'a>> {
        if self.bookmarks.is_none() {
            let mut tab = BookmarksTab::new()?;
            tab.set_layout(self.layout);
            self.bookmarks = Some(tab);
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
            ComponentAction::ToggleLayout => {
                self.layout = self.layout.toggle();
                if let Some(tab) = self.log.as_mut() {
                    tab.set_layout(self.layout);
                }
                if let Some(tab) = self.files.as_mut() {
                    tab.set_layout(self.layout);
                }
                if let Some(tab) = self.bookmarks.as_mut() {
                    tab.set_layout(self.layout);
                }
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
                        match self.global_keybinds.match_event(key) {
                            GlobalEvent::Quit => return Ok(true),
                            GlobalEvent::NextTab => self.set_next_tab_with_offset(1)?,
                            GlobalEvent::PrevTab => self.set_next_tab_with_offset(-1)?,
                            GlobalEvent::Tab1 => self.set_tab(Tab::Log)?,
                            GlobalEvent::Tab2 => self.set_tab(Tab::Files)?,
                            GlobalEvent::Tab3 => self.set_tab(Tab::Bookmarks)?,
                            GlobalEvent::CommandPopup => {
                                self.popup = Some(Box::new(CommandPopup::new()));
                            }
                            GlobalEvent::OpenHelp => {
                                let (main_help, details_help) = {
                                    let tab = self.get_or_init_current_tab()?;
                                    (tab.make_main_panel_help(), tab.make_details_panel_help())
                                };
                                let global_help = self.global_keybinds.make_help();
                                self.popup = Some(Box::new(HelpPopup::new(
                                    main_help,
                                    details_help,
                                    global_help,
                                )));
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
