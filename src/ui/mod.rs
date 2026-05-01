pub mod bookmark_set_popup;
pub mod bookmarks_tab;
pub mod command_popup;
pub mod commit_show_cache;
pub mod context_menu_popup;
pub mod files_tab;
pub mod help_popup;
pub mod loader_popup;
pub mod log_tab;
pub mod message_popup;
pub mod panel;
pub mod rebase_popup;
pub mod styles;
pub mod utils;

use std::process::Command;

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::symbols;
use ratatui::widgets::*;
use tracing::instrument;

use crate::ComponentInputResult;
use crate::app::App;
use crate::app::Tab;
use crate::commander::log::Head;
use crate::env::get_env;

pub enum ComponentAction {
    ViewFiles(Head),
    ViewLog(Head),
    ChangeHead(Head),
    SetPopup(Option<Box<dyn Component>>),
    Multiple(Vec<ComponentAction>),
    RefreshTab(),
    /// Run a command that takes over the terminal. The main loop drains
    /// these and refreshes the current view once the command exits.
    RunInteractive(Command),
}

pub trait Component {
    // Called when switching to tab
    fn focus(&mut self) -> Result<()> {
        Ok(())
    }

    fn update(&mut self) -> Result<Option<ComponentAction>> {
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()>;

    fn input(&mut self, event: Event) -> Result<ComponentInputResult>;

    /// True if the component needs `update()` calls on a steady tick
    /// (e.g. an in-flight drag auto-scroll). Drives the input poll
    /// timeout in the main loop.
    fn wants_tick(&self) -> bool {
        false
    }
}

#[instrument(level = "trace", name = "draw", skip(f, app))]
pub fn ui(f: &mut Frame, app: &mut App) -> Result<()> {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(f.area());

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
                .position(|tab| tab == &app.current_tab)
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

    if let Some(current_tab) = app.get_current_tab() {
        current_tab.draw(f, chunks[1])?;
    }

    if let Some(popup) = app.popup.as_mut() {
        popup.draw(f, f.area())?;
    }

    {
        let paragraph = Paragraph::new(format!("{}ms", app.stats.start_time.elapsed().as_millis()))
            .alignment(Alignment::Right);
        let position = Rect {
            x: 0,
            y: 1,
            height: 1,
            width: f.area().width - 1,
        };
        f.render_widget(paragraph, position);
    }

    Ok(())
}
