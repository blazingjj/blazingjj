/*! All user interface components, such as tabs, panels and dialogs.
*/
pub mod bookmarks_tab;
pub mod dialog;
pub mod evolog_tab;
pub mod files_tab;
pub mod keybindings_tab;
pub mod log_tab;
pub mod op_log_tab;
pub mod panel;
pub mod settings_tab;
pub mod status_bar;
pub mod styles;
pub mod utils;
pub mod workspaces_tab;
use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;

use crate::app::TabId;
use crate::app::command::Command;
use crate::background_tasks::TaskResult;
use crate::commander::log::Head;
use crate::commander::program::Program;
use crate::event::Mouse;
use crate::keybinds::Binding;

/// Action commmands from component to application
pub enum AppAction {
    ViewFiles(Head),
    /// Show the files of one version of a change, as opposed to those of
    /// the change as it stands.
    ViewVersionFiles(Head),
    ViewEvolog(Head),
    ViewLog(Head),
    /// Show the bookmark of this name, which may have just come into
    /// being.
    ViewBookmark(String),
    /// Show this tab, as it stands.
    ViewTab(TabId),
    ChangeHead(Head),
    /// Put this popup up, in place of whatever is up now.
    SetPopup(Box<dyn Component>),
    /// Take the popup down. Whatever it was there to collect is asked
    /// for alongside this, so there is nothing left for the app to do.
    ClosePopup,
    /// The marked changes have been acted on, so the log stops marking
    /// them.
    ClearLogMarks,
    Multiple(Vec<AppAction>),
    /// Have every tab read itself again before it is next drawn, the
    /// operation that has just run having moved the repo.
    MarkTabsStale,
    /// The configuration has changed, so the app reads it again and
    /// everything that goes by it takes up what it now says.
    ConfigChanged,
    /// Work in the workspace at this path from now on: the app comes
    /// down and is started again there.
    RestartIn(String),
    /// Run this operation and do whatever it asks for in turn. Whoever
    /// raises one has named it in full, so the app can run it without
    /// asking anything of the component the request came from.
    Run(Command),
    /// Run a command that takes over the terminal, in place of whatever
    /// other one is queued, and read the repo again once it is done.
    RunInteractive(Interactive),
}

/// A program to run with the terminal handed over to it.
pub struct Interactive {
    pub program: Program,
    /// Whether to wait for the user before taking the screen back. Off for
    /// a command run for its effect, whose output is beside the point
    /// unless it failed.
    pub hold_screen: bool,
}

/// When a Component process an input event, it returns an ComponentInputResult
/// which tells the app what to do.
pub enum ComponentInputResult {
    /// The app should stop processing the event
    Handled,
    /// The app should perform the specified AppAction.
    HandledAction(AppAction),
    /// The app should ask the next component in z-order to handle the event
    NotHandled,
    /// The app should close this popup and then ask the next component
    /// in z-order to handle the event.
    Dismissed,
}

impl From<Option<AppAction>> for ComponentInputResult {
    fn from(app_action: Option<AppAction>) -> Self {
        match app_action {
            Some(app_action) => ComponentInputResult::HandledAction(app_action),
            None => ComponentInputResult::Handled,
        }
    }
}

/// How far to move the selection in a tab's main panel.
#[derive(Debug, Clone, Copy)]
pub enum Scroll {
    Down,
    Up,
    DownHalfPage,
    UpHalfPage,
    ToTop,
    ToBottom,
}

impl Scroll {
    /// How far to move in a list of `page` entries, as far as there is
    /// to go for the ends.
    fn distance(self, page: isize) -> isize {
        match self {
            Scroll::Down => 1,
            Scroll::Up => -1,
            Scroll::DownHalfPage => page / 2,
            Scroll::UpHalfPage => (page / 2).saturating_neg(),
            Scroll::ToBottom => isize::MAX,
            Scroll::ToTop => -isize::MAX,
        }
    }
}

pub trait Component {
    fn update(&mut self) -> Result<Option<AppAction>> {
        Ok(None)
    }

    /// Called with the result of a task this component submitted.
    fn task_done(&mut self, _result: TaskResult) -> Result<Option<AppAction>> {
        Ok(None)
    }

    /// Whether what the component shows changes without anything
    /// happening. While it does, the main loop draws on a timer rather
    /// than only on an event.
    fn needs_periodic_redraw(&self) -> bool {
        false
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()>;

    fn input(&mut self, _event: Event) -> Result<ComponentInputResult> {
        Ok(ComponentInputResult::NotHandled)
    }

    /// Called with what the mouse did, in z-order from the top down,
    /// until a component takes it.
    fn input_mouse(&mut self, _mouse: Mouse) -> Result<ComponentInputResult> {
        Ok(ComponentInputResult::NotHandled)
    }
}

/// A top-level tab, showing what it reads from the repo.
pub trait Tab: Component {
    /// Read whatever this tab shows afresh from the repo, leaving it no
    /// longer stale.
    fn refresh(&mut self) -> Result<()>;

    /// Have the next [refresh](Tab::refresh) read the repo.
    fn mark_stale(&mut self);

    /// Take up the configuration as it now reads, of which the tab and
    /// its panels hold what they go by.
    fn config_changed(&mut self);

    /// Turn the tab's panes the other way round.
    fn toggle_layout(&mut self);

    /// Whether the next [refresh](Tab::refresh) will read the repo.
    fn is_stale(&self) -> bool;

    /// Discard whatever this tab has cached, so that the next read goes
    /// to the repo.
    fn drop_caches(&mut self) {}

    /// Move the selection in the main panel.
    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()>;

    /// Show the change the working copy is on, if the tab has one.
    fn focus_current(&mut self) -> Result<()> {
        Ok(())
    }

    /// The menu of what can be done to what the tab has selected, put
    /// where the selection is.
    fn open_context_menu(&self) -> Result<Option<AppAction>>;

    /// Keybindings of the main panel, for the help popup.
    fn main_panel_bindings(&self) -> Vec<Binding>;

    /// Keybindings of the details panel, for the help popup.
    fn details_panel_bindings(&self) -> Vec<Binding>;
}
