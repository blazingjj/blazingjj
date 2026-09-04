/*! The workspaces tab shows the working copies attached to the repo: the
workspaces in the main panel, and the change the selected one holds in
the details panel.
*/

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
use crate::commander::new_commander;
use crate::commander::workspace::Workspace;
use crate::commander::workspace::WorkspaceLine;
use crate::event::Mouse;
use crate::keybinds::Binding;
use crate::keybinds::DetailsPanelEvent;
use crate::keybinds::DetailsPanelKeybinds;
use crate::keybinds::WorkspacesTabEvent;
use crate::keybinds::WorkspacesTabKeybinds;
use crate::theme::Role;
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
use crate::ui::styles::AnsiText;
use crate::ui::styles::panel_block;
use crate::ui::styles::panel_title;
use crate::ui::styles::patched;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::error_text;

/// What marks the workspace the app is running in, and what stands in
/// its place on the lines of the others.
const CURRENT_MARK: &str = " * ";
const OTHER_MARK: &str = "   ";

/// The workspace the selection is on, by what identifies it across reads
/// of the listing: its name, or its directory once the name is gone, as
/// after a rename.
#[derive(Clone, Debug)]
struct Selected {
    name: String,
    root: Option<String>,
}

impl Selected {
    fn of(workspace: &Workspace) -> Self {
        Self {
            name: workspace.name.clone(),
            root: workspace.root.clone(),
        }
    }
}

pub struct WorkspacesTab {
    workspaces: anyhow::Result<Vec<WorkspaceLine>>,
    workspaces_pane: ListPane,
    workspaces_list_state: ListState,

    selected: Option<Selected>,

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
        self.follow_selection();

        // Every listed workspace is one we may come to show
        let targets = self
            .listed()
            .map(|line| line.workspace.target.clone())
            .collect();
        self.workspace_panel.set_active(targets);

        self.show_workspace();
    }

    /// Put the selection back on its workspace in a listing read afresh.
    /// A renamed workspace keeps it under its new name. One that has been
    /// forgotten, like the first read of all, leaves it on the workspace
    /// we are running in.
    fn follow_selection(&mut self) {
        self.selected = self
            .selected_workspace()
            .or_else(|| self.current_workspace())
            .or_else(|| self.listed().next().map(|line| &line.workspace))
            .map(Selected::of);
    }

    /// The lines of the listing, of which there are none while we have
    /// failed to read it.
    fn listed(&self) -> impl Iterator<Item = &WorkspaceLine> {
        self.workspaces.iter().flatten()
    }

    /// The workspace the app is running in, if it is listed.
    fn current_workspace(&self) -> Option<&Workspace> {
        self.listed()
            .map(|line| &line.workspace)
            .find(|workspace| workspace.current)
    }

    /// Where in the listing the selection is, if the workspace it is on
    /// is still listed.
    fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;

        self.listed()
            .position(|line| line.workspace.name == selected.name)
            .or_else(|| {
                let root = selected.root.as_deref()?;
                self.listed()
                    .position(|line| line.workspace.root.as_deref() == Some(root))
            })
    }

    /// The workspace the operations would act on, if the selection is on
    /// one still listed.
    fn selected_workspace(&self) -> Option<&Workspace> {
        let index = self.selected_index()?;

        self.listed().nth(index).map(|line| &line.workspace)
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
        let count = self.listed().count();
        let next = match self.selected_index() {
            Some(index) => index
                .saturating_add_signed(scroll)
                .min(count.saturating_sub(1)),
            None => 0,
        };

        self.select_line(next);
    }

    /// Put the selection on the workspace at `index` and report whether
    /// there is one.
    fn select_line(&mut self, index: usize) -> bool {
        let Some(selected) = self
            .listed()
            .nth(index)
            .map(|line| Selected::of(&line.workspace))
        else {
            return false;
        };

        self.selected = Some(selected);
        self.show_workspace();

        true
    }

    /// The menu of what can be done to the selected workspace, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> AppAction {
        AppAction::SetPopup(Box::new(workspaces_context_menu(
            anchor,
            self.selected_workspace(),
        )))
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
                    return Ok(Some(command::ask_forget_workspace(workspace)));
                }
            }
            WorkspacesTabEvent::Switch => {
                if let Some(workspace) = self.selected_workspace() {
                    return Ok(Some(command::switch_workspace(workspace)));
                }
            }
            // Not an operation of its own; the key handler deals with it.
            WorkspacesTabEvent::Unbound => {}
        }

        Ok(None)
    }
}

/// The listing as it is drawn, one line per workspace: what jj wrote about
/// it, with the one we are running in marked and the line at `selected`
/// highlighted.
fn listing_lines(
    workspaces: &anyhow::Result<Vec<WorkspaceLine>>,
    selected: Option<usize>,
) -> Result<Vec<Line<'_>>, ansi_to_tui::Error> {
    let workspaces = match workspaces {
        Ok(workspaces) => workspaces,
        Err(err) => {
            return Ok(error_text("Error getting workspaces", err)?.lines);
        }
    };

    let mut lines = Vec::new();
    for (index, listed) in workspaces.iter().enumerate() {
        let mark = if listed.workspace.current {
            CURRENT_MARK
        } else {
            OTHER_MARK
        };

        // The pane gives each item a single row, so an entry a custom
        // template writes over several lines is put on one.
        let mut line = Line::from(mark);
        for (i, part) in listed.text.to_ansi_text()?.lines.into_iter().enumerate() {
            if i > 0 {
                line.spans.push(Span::raw(" "));
            }
            line.spans.extend(part.spans);
        }

        if selected == Some(index) {
            line = patched(line, Role::Highlight.style());
        }

        lines.push(line);
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
        if let Some(current) = self.current_workspace() {
            self.selected = Some(Selected::of(current));
            self.show_workspace();
        }

        Ok(())
    }

    fn open_context_menu(&mut self) -> Result<Option<AppAction>> {
        Ok(Some(self.context_menu(self.selected_index().and_then(
            |index| self.workspaces_pane.item_anchor(index, 1),
        ))))
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
                vec![
                    Line::from(" No workspaces")
                        .patch_style(Role::Hint.style())
                        .italic(),
                ]
            } else {
                lines
            };

            let block = panel_block().title(panel_title(" Workspaces "));
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
                    return Ok(Some(command::switch_workspace(workspace)).into());
                }
            }
            MouseInput::Copy(text) => return Ok(copy_marked(text)),
            MouseInput::Context(index) => {
                if self.select_line(index) {
                    return Ok(Some(self.context_menu(Some(mouse.position()))).into());
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
        WorkspaceLine {
            text: format!("{name}: .."),
            workspace: Workspace {
                name: name.to_owned(),
                root: Some(format!("/tmp/{name}")),
                target: Head {
                    change_id: ChangeId(name.to_owned()),
                    commit_id: CommitId(name.to_owned()),
                    divergent: false,
                    immutable: false,
                    local_bookmarks: Vec::new(),
                },
                current,
            },
        }
    }

    fn selected_name(tab: &WorkspacesTab) -> Option<&str> {
        tab.selected_workspace()
            .map(|workspace| workspace.name.as_str())
    }

    #[test]
    fn the_selection_stays_on_the_workspace_it_names() {
        let mut tab = tab();
        tab.workspaces = Ok(vec![line("default", true), line("other", false)]);
        tab.select_line(1);

        // The workspaces are listed the other way round after a rename
        // elsewhere, and the selection is still on the same one.
        tab.workspaces = Ok(vec![line("other", false), line("default", true)]);
        assert_eq!(tab.selected_index(), Some(0));
    }

    #[test]
    fn a_renamed_workspace_keeps_the_selection() {
        let mut tab = tab();
        tab.workspaces = Ok(vec![line("default", true), line("other", false)]);
        tab.select_line(1);

        let mut renamed = line("renamed", false);
        renamed.workspace.root = Some("/tmp/other".to_owned());
        tab.workspaces = Ok(vec![line("default", true), renamed]);
        tab.follow_selection();

        assert_eq!(selected_name(&tab), Some("renamed"));
        let Some(selected) = &tab.selected else {
            panic!("the selection is on the renamed workspace");
        };
        assert_eq!(selected.name, "renamed");
    }

    #[test]
    fn a_forgotten_workspace_leaves_the_selection_on_ours() {
        let mut tab = tab();
        tab.workspaces = Ok(vec![line("forgotten", false), line("ours", true)]);
        tab.select_line(0);

        tab.workspaces = Ok(vec![line("ours", true)]);
        tab.follow_selection();

        assert_eq!(selected_name(&tab), Some("ours"));
    }

    #[test]
    fn a_listing_without_ours_selects_its_first_workspace() {
        let mut tab = tab();
        tab.workspaces = Ok(vec![line("one", false), line("two", false)]);

        tab.follow_selection();

        assert_eq!(selected_name(&tab), Some("one"));
    }

    #[test]
    fn an_entry_over_several_lines_takes_a_single_row() {
        let mut entry = line("default", true);
        entry.text = "default:\n  somewhere".to_owned();
        let workspaces = Ok(vec![entry, line("other", false)]);

        let lines = listing_lines(&workspaces, None).expect("the listing is drawn");

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].to_string(), " * default:   somewhere");
    }

    #[test]
    fn scrolling_stops_at_either_end_of_the_listing() {
        let mut tab = tab();
        tab.workspaces = Ok(vec![line("default", true), line("other", false)]);
        tab.select_line(0);

        tab.scroll_workspaces(1);
        assert_eq!(selected_name(&tab), Some("other"));

        tab.scroll_workspaces(1);
        assert_eq!(selected_name(&tab), Some("other"));

        tab.scroll_workspaces(-5);
        assert_eq!(selected_name(&tab), Some("default"));
    }
}
