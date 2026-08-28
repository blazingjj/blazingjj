/*! The evolog tab shows how a change came to be: the versions it has had
in the main panel, and what the rewrite that produced the selected one
changed in the details panel.
*/

use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::prelude::*;
use tracing::instrument;

use crate::app::TabId;
use crate::app::command;
use crate::app::command::Command;
use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskResult;
use crate::background_tasks::TaskSlot;
use crate::commander::log::EVOLOG_LINES_PER_HEAD;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::commander::revset::Revset;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::keybinds::DetailsPanelEvent;
use crate::keybinds::DetailsPanelKeybinds;
use crate::keybinds::EvologTabEvent;
use crate::keybinds::EvologTabKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::evolog_context_menu;
use crate::ui::panel::EvologShowPanel;
use crate::ui::panel::LogPanel;
use crate::ui::panel::MouseInput;
use crate::ui::panel::route_mouse;
use crate::ui::utils::PaneDivider;

pub struct EvologTab<'a> {
    /// The change the evolog is read for, as opposed to the version of it
    /// the panels are on
    change: Head,

    /// The versions of the change, newest first
    entry_panel: LogPanel<'a>,

    /// The panel showing what the selected version changed
    patch_panel: EvologShowPanel,

    config: JjConfig,
    keybinds: EvologTabKeybinds,
    details_keybinds: DetailsPanelKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

impl<'a> EvologTab<'a> {
    /// A stale tab for `current_head`, holding no entries yet.
    #[instrument(level = "info", name = "Initializing evolog tab", parent = None, skip(background_tasks))]
    pub fn new(current_head: &Head, background_tasks: BackgroundTasks) -> Self {
        let config = get_env().jj_config.clone();

        Self {
            change: current_head.clone(),

            entry_panel: LogPanel::new(current_head.clone(), EVOLOG_LINES_PER_HEAD),
            patch_panel: EvologShowPanel::new(TabId::Evolog, background_tasks),

            pane_divider: PaneDivider::new(config.layout_percent()),
            config,
            keybinds: EvologTabKeybinds::default(),
            details_keybinds: DetailsPanelKeybinds::default(),

            stale: true,
        }
    }

    /// Show the evolog of `head`, read the next time the tab is shown.
    pub fn set_head(&mut self, head: &Head) {
        self.change = head.clone();
        self.entry_panel.set_head(head.clone());
        self.stale = true;
    }

    /// Read the evolog afresh and update the details panel.
    fn refresh_evolog(&mut self) {
        let title = format!(" Evolog for {} ", self.change.change_id);
        self.entry_panel
            .show(new_commander().get_evolog(&self.change.commit_id), title);

        // Abandoning the change moves the tab to another one, whose evolog
        // does not list the version that was selected, so the selection
        // falls back to the newest one.
        let entries = self.entry_panel.log_heads();
        if !entries.contains(&self.entry_panel.head)
            && let Some(newest) = entries.first()
        {
            self.entry_panel.set_head(newest.clone());
        }

        self.patch_panel.set_active(entries);
        self.sync_entry_output();
    }

    /// Have the details panel show what the selected version changed.
    fn sync_entry_output(&mut self) {
        let entry = self.entry_panel.head.clone();
        let title = format!(" Version {} ", entry.commit_id.short());
        self.patch_panel.show(Some(entry), title);
    }

    fn scroll_entries(&mut self, scroll: isize) {
        self.entry_panel.scroll_relative(scroll);
        self.sync_entry_output();
    }

    /// The menu of what can be done to the selected version, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn open_context_menu(&self, anchor: Option<Position>) -> Option<AppAction> {
        Some(AppAction::SetPopup(Box::new(evolog_context_menu(
            self.config.clone(),
            anchor,
            &self.entry_panel.head,
            &self.change,
        ))))
    }

    fn handle_event(&mut self, event: EvologTabEvent) -> Result<Option<AppAction>> {
        let entry = &self.entry_panel.head;

        match event {
            EvologTabEvent::OpenFiles => {
                return Ok(Some(command::show_version_files(entry, &self.change)));
            }
            EvologTabEvent::Duplicate => {
                // The duplicate is a change of its own, so it shows up in
                // the log rather than in the evolog we are on
                return Ok(Some(AppAction::Run(Command::Duplicate(Revset::from(
                    &entry.commit_id,
                )))));
            }
            EvologTabEvent::CopyRev => {
                return Ok(Some(AppAction::Run(Command::Copy(
                    entry.commit_id.as_str().to_owned(),
                ))));
            }
            EvologTabEvent::OpenContextMenu => {
                return Ok(self.open_context_menu(self.entry_panel.selected_position()));
            }
            // Not an operation of its own; the key handler deals with it.
            EvologTabEvent::Unbound => {}
        }

        Ok(None)
    }
}

impl Tab for EvologTab<'_> {
    fn refresh(&mut self) -> Result<()> {
        // The evolog we are about to read leaves the working copy alone,
        // so following the change has to as well, or the two disagree.
        let mut commander = new_commander();
        commander.ignore_working_copy();

        self.change = commander.get_head_latest(&self.change)?;
        self.refresh_evolog();
        self.stale = false;

        Ok(())
    }

    fn mark_stale(&mut self) {
        self.stale = true;
    }

    fn is_stale(&self) -> bool {
        self.stale
    }

    fn drop_caches(&mut self) {
        self.patch_panel.mark_dirty();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        let half_page = self.entry_panel.visible_heads() / 2;
        self.scroll_entries(match scroll {
            Scroll::Down => 1,
            Scroll::Up => -1,
            Scroll::DownHalfPage => half_page,
            Scroll::UpHalfPage => half_page.saturating_neg(),
        });
        Ok(())
    }

    fn focus_current(&mut self) -> Result<()> {
        self.set_head(&new_commander().get_current_head()?);
        Ok(())
    }

    fn make_main_panel_help(&self) -> Vec<(String, String)> {
        self.keybinds.make_help()
    }

    fn make_details_panel_help(&self) -> Vec<(String, String)> {
        self.details_keybinds.make_help()
    }
}

impl Component for EvologTab<'_> {
    fn update(&mut self) -> Result<Option<AppAction>> {
        self.patch_panel.update();

        Ok(None)
    }

    fn task_done(&mut self, result: TaskResult) -> Result<Option<AppAction>> {
        if let TaskSlot::EvologShow(_, request) = result.slot {
            self.patch_panel.task_done(request, result.output);
        }
        Ok(None)
    }

    fn is_waiting(&self) -> bool {
        self.patch_panel.is_waiting()
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = self.pane_divider.split(area, self.config.layout());

        self.entry_panel.draw(f, chunks[0])?;
        self.patch_panel.draw(f, chunks[1]);

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
                    self.patch_panel.handle_event(ev);
                    return Ok(ComponentInputResult::Handled);
                }
            }

            return match self.keybinds.match_event(key) {
                // Not the tab's to act on, so whoever else wants the key
                // is welcome to it.
                EvologTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event)?.into()),
            };
        }

        if let Event::Mouse(mouse) = event {
            if self.pane_divider.handle_mouse(mouse, self.config.layout()) {
                return Ok(ComponentInputResult::Handled);
            }
            match route_mouse(mouse, &mut [&mut self.entry_panel, &mut self.patch_panel]) {
                MouseInput::Scroll(delta) => self.scroll_entries(delta),
                MouseInput::Select(index) => {
                    if let Some(entry) = self.entry_panel.head_at_log_line(index) {
                        self.entry_panel.set_head(entry);
                        self.sync_entry_output();
                    }
                }
                // The graph takes lines of its own, which name no version
                // for a menu to act on.
                MouseInput::Context(index) => {
                    if let Some(entry) = self.entry_panel.head_at_log_line(index) {
                        self.entry_panel.set_head_in_place(entry);
                        self.sync_entry_output();
                        let anchor = Position::new(mouse.column, mouse.row);
                        return Ok(self.open_context_menu(Some(anchor)).into());
                    }
                }
                MouseInput::Handled => {}
                MouseInput::NotHandled => return Ok(ComponentInputResult::NotHandled),
            }
            return Ok(ComponentInputResult::Handled);
        }

        Ok(ComponentInputResult::Handled)
    }
}
