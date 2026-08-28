/*! All user interface components, such as tabs, panels and dialogs.
*/
pub mod bookmarks_tab;
pub mod dialog;
pub mod evolog_tab;
pub mod files_tab;
pub mod log_tab;
pub mod panel;
pub mod styles;
pub mod utils;
use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;

use crate::app::command::Command;
use crate::background_tasks::TaskResult;
use crate::commander::log::Head;

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
    ChangeHead(Head),
    /// Put this popup up, in place of whatever is up now.
    SetPopup(Box<dyn Component>),
    /// Take the popup down, what it was there to do having been done.
    PopupDone,
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
    /// Run this operation and do whatever it asks for in turn. Whoever
    /// raises one has named it in full, so the app can run it without
    /// asking anything of the component the request came from.
    Run(Command),
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
}

pub trait Component {
    fn update(&mut self) -> Result<Option<AppAction>> {
        Ok(None)
    }

    /// Called with the result of a task this component submitted.
    fn task_done(&mut self, _result: TaskResult) -> Result<Option<AppAction>> {
        Ok(None)
    }

    /// Whether the component is still waiting for a task result it wants.
    /// While it is, the main loop keeps calling [Self::update].
    fn is_waiting(&self) -> bool {
        false
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()>;

    fn input(&mut self, _event: Event) -> Result<ComponentInputResult> {
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

    /// Keybindings of the main panel, for the help popup.
    fn make_main_panel_help(&self) -> Vec<(String, String)>;

    /// Keybindings of the details panel, for the help popup.
    fn make_details_panel_help(&self) -> Vec<(String, String)>;
}
