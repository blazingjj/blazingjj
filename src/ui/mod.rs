/*! All user interface components, such as tabs, panels and dialogs.
*/
pub mod bookmarks_tab;
pub mod commit_show_cache;
pub mod dialog;
pub mod files_tab;
pub mod log_tab;
pub mod panel;
pub mod styles;
pub mod utils;
use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;

use crate::commander::log::Head;

/// Action commmands from component to application
pub enum AppAction {
    ViewFiles(Head),
    ViewLog(Head),
    ChangeHead(Head),
    /// Put this popup up, in place of whatever is up now.
    SetPopup(Box<dyn Component>),
    /// Take the popup down, what it was there to do having been done.
    PopupDone,
    /// Take the popup down, nothing having been done.
    PopupCanceled,
    Multiple(Vec<AppAction>),
    RefreshTab(),
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
}

impl ComponentInputResult {
    pub fn is_handled(&self) -> bool {
        match self {
            Self::Handled => true,
            Self::HandledAction(_) => true,
            Self::NotHandled => false,
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
    /// Read whatever this component shows afresh from the repo.
    fn refresh(&mut self) -> Result<()> {
        Ok(())
    }

    /// Read afresh on the user's request, discarding anything cached.
    fn force_refresh(&mut self) -> Result<()> {
        self.refresh()
    }

    /// Move the selection in the main panel.
    fn scroll_main_panel(&mut self, _scroll: Scroll) -> Result<()> {
        Ok(())
    }

    /// Show the change the working copy is on.
    fn focus_current(&mut self) -> Result<()> {
        Ok(())
    }

    /// Keybindings of the main panel, for the help popup.
    fn make_main_panel_help(&self) -> Vec<(String, String)> {
        vec![]
    }

    /// Keybindings of the details panel, for the help popup.
    fn make_details_panel_help(&self) -> Vec<(String, String)> {
        vec![]
    }

    fn update(&mut self) -> Result<Option<AppAction>> {
        Ok(None)
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()>;

    fn input(&mut self, event: Event) -> Result<ComponentInputResult>;
}
