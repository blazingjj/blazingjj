/*! The workspaces tab shows the working copies attached to the repo: the
workspaces in the main panel, and the change the selected one holds in
the details panel.

A workspace is worked in by running there, so switching to one restarts
the app in its directory rather than taking the repo over from here.
*/

use ansi_to_tui::IntoText;
use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::prelude::*;
use ratatui::widgets::*;
use tracing::instrument;

use crate::app::TabId;
use crate::app::command;
use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskResult;
use crate::background_tasks::TaskSlot;
use crate::commander::CommandError;
use crate::commander::new_commander;
use crate::commander::workspace::Workspace;
use crate::commander::workspace::WorkspaceLine;
use crate::env::get_env;
use crate::event::Mouse;
use crate::keybinds::Binding;
use crate::keybinds::DetailsPanelEvent;
use crate::keybinds::DetailsPanelKeybinds;
use crate::keybinds::WorkspacesTabEvent;
use crate::keybinds::WorkspacesTabKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::workspaces_context_menu;
use crate::ui::panel::CommitShowPanel;
use crate::ui::panel::ListPane;
use crate::ui::panel::MouseInput;
use crate::ui::panel::copy_marked;
use crate::ui::panel::route_mouse;
use crate::ui::utils::PaneDivider;

/// What marks the workspace the app is running in, and what stands in
/// its place on the lines of the others.
const CURRENT_MARK: &str = " * ";
const OTHER_MARK: &str = "   ";

pub struct WorkspacesTab {
    workspaces: Result<Vec<WorkspaceLine>, CommandError>,
    workspaces_pane: ListPane,
    workspaces_list_state: ListState,

    /// The workspace the selection is on, by name: the list is read
    /// afresh often enough that holding a line of it would be holding
    /// one that is gone.
    selected: Option<String>,

    /// The panel showing the change the selected workspace holds
    workspace_panel: CommitShowPanel,

    keybinds: WorkspacesTabKeybinds,
    details_keybinds: DetailsPanelKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

impl WorkspacesTab {
    /// A stale tab, holding no workspaces yet.
    #[instrument(level = "info", name = "Initializing workspaces tab", parent = None, skip(background_tasks))]
    pub fn new(background_tasks: BackgroundTasks) -> Self {
        Self {
            workspaces: Ok(Vec::new()),
            workspaces_pane: ListPane::default(),
            workspaces_list_state: ListState::default(),

            selected: None,

            workspace_panel: CommitShowPanel::new(TabId::Workspaces, background_tasks),

            keybinds: WorkspacesTabKeybinds::new(),
            details_keybinds: DetailsPanelKeybinds::new(),
            pane_divider: PaneDivider::default(),

            stale: true,
        }
    }

    /// Read the workspaces afresh and update the details panel.
    fn refresh_workspaces(&mut self) {
        self.workspaces = new_commander().get_workspaces();

        // A workspace that has been forgotten or renamed leaves the
        // selection naming none, and so does the first read of all, so
        // it falls back to the workspace we are running in.
        if self.selected_index().is_none() {
            self.selected = self
                .workspace_of(|workspace| workspace.current)
                .or_else(|| self.workspace_of(|_| true))
                .map(|workspace| workspace.name.clone());
        }

        // Every listed workspace is one we may come to show
        let targets = self
            .listed()
            .filter_map(|line| line.workspace())
            .map(|workspace| workspace.target.clone())
            .collect();
        self.workspace_panel.set_active(targets);

        self.show_workspace();
    }

    /// The lines of the listing, of which there are none while we have
    /// failed to read it.
    fn listed(&self) -> impl Iterator<Item = &WorkspaceLine> {
        self.workspaces.iter().flatten()
    }

    /// The first listed workspace `matches` takes.
    fn workspace_of(&self, matches: impl Fn(&Workspace) -> bool) -> Option<&Workspace> {
        self.listed()
            .filter_map(|line| line.workspace())
            .find(|workspace| matches(workspace))
    }

    /// Where in the listing the selection is, if what it names is still
    /// listed.
    fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_deref()?;

        self.listed().position(
            |line| matches!(line.workspace(), Some(workspace) if workspace.name == selected),
        )
    }

    /// The workspace the operations would act on, if the selection is on
    /// one we could make out.
    fn selected_workspace(&self) -> Option<&Workspace> {
        let selected = self.selected.as_deref()?;

        self.workspace_of(|workspace| workspace.name == selected)
    }

    /// Have the details panel show the change the selected workspace
    /// holds.
    fn show_workspace(&mut self) {
        let (target, title) = match self.selected_workspace() {
            Some(workspace) => (
                Some(workspace.target.clone()),
                format!(" Workspace {} ", workspace.name),
            ),
            None => (None, " Workspace ".to_owned()),
        };

        self.workspace_panel.show(target, title);
    }

    fn scroll_workspaces(&mut self, scroll: isize) {
        let listed: Vec<&WorkspaceLine> = self.listed().collect();
        let next = match self.selected_index() {
            Some(index) => listed.get(
                index
                    .saturating_add_signed(scroll)
                    .min(listed.len().saturating_sub(1)),
            ),
            None => listed.first(),
        };

        let Some(name) = next
            .and_then(|line| line.workspace())
            .map(|workspace| workspace.name.clone())
        else {
            return;
        };

        self.selected = Some(name);
        self.show_workspace();
    }

    /// Put the selection on the workspace the line at `index` is and
    /// report whether there was one: a line we cannot make out names no
    /// workspace to select.
    fn select_line(&mut self, index: usize) -> bool {
        let Some(name) = self
            .listed()
            .nth(index)
            .and_then(|line| line.workspace())
            .map(|workspace| workspace.name.clone())
        else {
            return false;
        };

        self.selected = Some(name);
        self.show_workspace();

        true
    }

    /// The menu of what can be done to the selected workspace, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> Option<AppAction> {
        Some(AppAction::SetPopup(Box::new(workspaces_context_menu(
            get_env().jj_config.clone(),
            anchor,
            self.selected_workspace(),
        ))))
    }

    fn handle_event(&mut self, event: WorkspacesTabEvent) -> Result<Option<AppAction>> {
        match event {
            WorkspacesTabEvent::Add => {
                return Ok(Some(command::ask_add_workspace()));
            }
            WorkspacesTabEvent::Rename => {
                if let Some(workspace) = self.selected_workspace() {
                    return Ok(Some(command::ask_rename_workspace(workspace)));
                }
            }
            WorkspacesTabEvent::Forget => {
                if let Some(workspace) = self.selected_workspace() {
                    return Ok(Some(command::ask_forget_workspace(
                        get_env().jj_config.clone(),
                        workspace,
                    )));
                }
            }
            WorkspacesTabEvent::Switch => {
                if let Some(workspace) = self.selected_workspace() {
                    return Ok(Some(command::ask_switch_workspace(
                        get_env().jj_config.clone(),
                        workspace,
                    )));
                }
            }
            // Not an operation of its own; the key handler deals with it.
            WorkspacesTabEvent::Unbound => {}
        }

        Ok(None)
    }
}

/// The listing as it is drawn: what jj wrote about each workspace, with
/// the one we are running in marked and the line at `selected`
/// highlighted.
fn listing_lines(
    workspaces: &Result<Vec<WorkspaceLine>, CommandError>,
    selected: Option<usize>,
) -> Result<Vec<Line<'_>>, ansi_to_tui::Error> {
    let workspaces = match workspaces {
        Ok(workspaces) => workspaces,
        Err(err) => {
            return Ok([
                vec![Line::raw("Error getting workspaces").bold().fg(Color::Red)],
                vec![Line::raw(""), Line::raw("")],
                err.to_string().into_text()?.lines,
            ]
            .concat());
        }
    };

    let mut lines = Vec::new();
    for (index, listed) in workspaces.iter().enumerate() {
        let current = listed
            .workspace()
            .is_some_and(|workspace| workspace.current);
        let mark = if current { CURRENT_MARK } else { OTHER_MARK };

        for line in listed.to_text()?.lines {
            let mut line = line.to_owned();
            line.spans.insert(0, Span::from(mark));

            if selected == Some(index) {
                let highlight = get_env().jj_config.highlight_color();

                line = line.bg(highlight);
                line.spans = line
                    .spans
                    .iter_mut()
                    .map(|span| span.to_owned().bg(highlight))
                    .collect();
            }

            lines.push(line);
        }
    }

    Ok(lines)
}

impl Tab for WorkspacesTab {
    fn refresh(&mut self) -> Result<()> {
        self.refresh_workspaces();
        self.stale = false;

        Ok(())
    }

    fn mark_stale(&mut self) {
        self.stale = true;
    }

    fn config_changed(&mut self) {
        self.workspace_panel.config_changed();
        self.keybinds = WorkspacesTabKeybinds::new();
        self.details_keybinds = DetailsPanelKeybinds::new();
    }

    fn toggle_layout(&mut self) {
        self.pane_divider.toggle_layout();
    }

    fn is_stale(&self) -> bool {
        self.stale
    }

    fn drop_caches(&mut self) {
        self.workspace_panel.mark_dirty();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        self.scroll_workspaces(scroll.distance(self.workspaces_pane.visible_items()));
        Ok(())
    }

    /// Go to the workspace the app is running in, which is the one the
    /// working copy on screen everywhere else belongs to.
    fn focus_current(&mut self) -> Result<()> {
        if let Some(current) = self.workspace_of(|workspace| workspace.current) {
            self.selected = Some(current.name.clone());
            self.show_workspace();
        }

        Ok(())
    }

    fn open_context_menu(&self) -> Result<Option<AppAction>> {
        Ok(self.context_menu(
            self.selected_index()
                .and_then(|index| self.workspaces_pane.item_anchor(index, 1)),
        ))
    }

    fn main_panel_bindings(&self) -> Vec<Binding> {
        self.keybinds.bindings()
    }

    fn details_panel_bindings(&self) -> Vec<Binding> {
        self.details_keybinds.bindings()
    }
}

impl Component for WorkspacesTab {
    fn update(&mut self) -> Result<Option<AppAction>> {
        self.workspace_panel.update();

        Ok(None)
    }

    fn task_done(&mut self, result: TaskResult) -> Result<Option<AppAction>> {
        if let TaskSlot::CommitShow(_, request) = result.slot {
            self.workspace_panel.task_done(request, result.output);
        }
        Ok(None)
    }

    fn needs_periodic_redraw(&self) -> bool {
        self.workspace_panel.needs_periodic_redraw()
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = self.pane_divider.split(area);

        {
            let selected = self.selected_index();
            let lines = listing_lines(&self.workspaces, selected)?;
            let lines = if lines.is_empty() {
                vec![Line::from(" No workspaces").fg(Color::DarkGray).italic()]
            } else {
                lines
            };

            let block = Block::bordered()
                .title(" Workspaces ")
                .border_type(BorderType::Rounded);
            let workspaces = List::new(lines).scroll_padding(3);
            *self.workspaces_list_state.selected_mut() = selected;
            self.workspaces_pane.render(
                f,
                chunks[0],
                block,
                workspaces,
                &mut self.workspaces_list_state,
            );
        }

        self.workspace_panel.draw(f, chunks[1]);

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                return Ok(ComponentInputResult::Handled);
            }

            match self.details_keybinds.match_event(key) {
                DetailsPanelEvent::Unbound => {}
                ev => {
                    self.workspace_panel.handle_event(ev);
                    return Ok(ComponentInputResult::Handled);
                }
            }

            return match self.keybinds.match_event(key) {
                // Not the tab's to act on, so whoever else wants the key
                // is welcome to it.
                WorkspacesTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event)?.into()),
            };
        }

        Ok(ComponentInputResult::Handled)
    }

    fn input_mouse(&mut self, mouse: Mouse) -> Result<ComponentInputResult> {
        if self.pane_divider.handle_mouse(mouse) {
            return Ok(ComponentInputResult::Handled);
        }

        match route_mouse(
            mouse,
            &mut [&mut self.workspaces_pane, &mut self.workspace_panel],
        ) {
            MouseInput::Scroll(delta) => self.scroll_workspaces(delta),
            MouseInput::Select(index) => {
                self.select_line(index);
            }
            // A double click is the second of two presses that selected
            // the workspace, so it only has the switch left to ask for.
            MouseInput::Activate => {
                if let Some(workspace) = self.selected_workspace() {
                    return Ok(Some(command::ask_switch_workspace(
                        get_env().jj_config.clone(),
                        workspace,
                    ))
                    .into());
                }
            }
            MouseInput::Copy(text) => return Ok(copy_marked(text)),
            // A line we cannot make out names no workspace for a menu
            // to act on, so it does not open one.
            MouseInput::Context(index) => {
                if self.select_line(index) {
                    return Ok(self.context_menu(Some(mouse.position())).into());
                }
            }
            MouseInput::Handled => {}
            MouseInput::NotHandled => return Ok(ComponentInputResult::NotHandled),
        }

        Ok(ComponentInputResult::Handled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander::ids::ChangeId;
    use crate::commander::ids::CommitId;
    use crate::commander::log::Head;
    use crate::env::set_test_env;

    fn tab() -> WorkspacesTab {
        set_test_env();
        let (sender, _receiver) = std::sync::mpsc::channel();

        WorkspacesTab::new(BackgroundTasks::new(sender))
    }

    /// A listed workspace of this name, the current one or not.
    fn line(name: &str, current: bool) -> WorkspaceLine {
        WorkspaceLine::Parsed {
            text: format!("{name}: .."),
            workspace: Workspace {
                name: name.to_owned(),
                root: Some(format!("/tmp/{name}")),
                target: Head {
                    change_id: ChangeId(name.to_owned()),
                    commit_id: CommitId(name.to_owned()),
                    divergent: false,
                    immutable: false,
                },
                current,
            },
        }
    }

    #[test]
    fn the_selection_stays_on_the_workspace_it_names() {
        let mut tab = tab();
        tab.workspaces = Ok(vec![line("default", true), line("other", false)]);
        tab.selected = Some("other".to_owned());

        assert_eq!(tab.selected_index(), Some(1));

        // The workspaces are listed the other way round after a rename
        // elsewhere, and the selection is still on the same one.
        tab.workspaces = Ok(vec![line("other", false), line("default", true)]);
        assert_eq!(tab.selected_index(), Some(0));
    }

    #[test]
    fn a_workspace_that_is_gone_leaves_the_selection_naming_none() {
        let mut tab = tab();
        tab.workspaces = Ok(vec![line("default", true)]);
        tab.selected = Some("forgotten".to_owned());

        assert_eq!(tab.selected_index(), None);
        assert!(tab.selected_workspace().is_none());
    }

    /// A line we cannot make out is listed as it is, and names no
    /// workspace, so a click on it leaves the selection where it was.
    #[test]
    fn clicking_a_line_that_names_no_workspace_selects_nothing() {
        let mut tab = tab();
        tab.workspaces = Ok(vec![
            line("default", true),
            WorkspaceLine::Unparsable("what?".to_owned()),
        ]);
        tab.selected = Some("default".to_owned());

        tab.select_line(1);

        assert_eq!(tab.selected.as_deref(), Some("default"));
    }
}
