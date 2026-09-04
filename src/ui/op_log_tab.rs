/*! The operation log tab shows what the repo has been through: the
operations in the main panel, and what the selected one did to the repo
in the details panel.
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
use crate::commander::new_commander;
use crate::commander::operation::OP_LOG_LINES_PER_ITEM;
use crate::commander::operation::Operation;
use crate::env::get_env;
use crate::event::Mouse;
use crate::keybinds::Binding;
use crate::keybinds::DetailsPanelEvent;
use crate::keybinds::DetailsPanelKeybinds;
use crate::keybinds::OpLogTabEvent;
use crate::keybinds::OpLogTabKeybinds;
use crate::selection::Selection;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::op_log_context_menu;
use crate::ui::panel::LogPanel;
use crate::ui::panel::MouseInput;
use crate::ui::panel::OpShowPanel;
use crate::ui::panel::copy_marked;
use crate::ui::panel::route_mouse;
use crate::ui::utils::PaneDivider;

/// How far back the tab reads before it is asked for more. The operation
/// log of a repo that has been worked in for a while runs to thousands of
/// entries, which take as many jj has to render.
const INITIAL_LIMIT: usize = 200;

pub struct OpLogTab<'a> {
    /// The operations the repo has been through, newest first
    op_panel: LogPanel<'a, Operation>,

    /// The panel showing what the selected operation did
    show_panel: OpShowPanel,

    /// How many operations the tab reads
    limit: usize,

    keybinds: OpLogTabKeybinds,
    details_keybinds: DetailsPanelKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

impl<'a> OpLogTab<'a> {
    /// A stale tab, holding no operations yet.
    #[instrument(level = "info", name = "Initializing operation log tab", parent = None, skip(background_tasks))]
    pub fn new(background_tasks: BackgroundTasks) -> Self {
        Self {
            // The log has yet to be read, and the operation it selects is
            // in it, so the first read takes the newest one.
            op_panel: LogPanel::new(Operation::default(), OP_LOG_LINES_PER_ITEM),
            show_panel: OpShowPanel::new(TabId::OpLog, background_tasks),

            limit: INITIAL_LIMIT,

            pane_divider: PaneDivider::default(),
            keybinds: OpLogTabKeybinds::new(),
            details_keybinds: DetailsPanelKeybinds::new(),

            stale: true,
        }
    }

    /// Read the operation log afresh and update the details panel.
    fn refresh_op_log(&mut self) {
        let op_log = new_commander().get_op_log(self.limit);

        // Reading as many operations as were asked for says that there
        // may well be more, which is when the key that goes further back
        // is worth mentioning.
        let more = op_log
            .as_ref()
            .is_ok_and(|op_log| op_log.items.len() >= self.limit);
        let title = if more {
            format!(" Operations (newest {}, m for more) ", self.limit)
        } else {
            " Operations ".to_owned()
        };

        self.op_panel.show(op_log, title);

        // Reading fewer operations than before leaves the selection out of
        // the log, and so does the first read of all.
        let operations = self.op_panel.items();
        if !operations.contains(&self.op_panel.selected)
            && let Some(newest) = operations.first()
        {
            self.op_panel.set_selected(newest.clone());
        }

        self.show_panel.set_active(operations);
        self.sync_show_output();
    }

    /// Have the details panel show what the selected operation did.
    fn sync_show_output(&mut self) {
        let operation = self.op_panel.selected.clone();

        // There is nothing selected as long as the log holds nothing,
        // which is where a failure to read it leaves us.
        if operation.id.as_str().is_empty() {
            self.show_panel.show(None, " Operation ".to_owned());
            return;
        }

        let title = format!(" Operation {} ", operation.id.short());
        self.show_panel.show(Some(operation), title);
    }

    fn scroll_operations(&mut self, scroll: isize) {
        self.op_panel.scroll_relative(scroll);
        self.sync_show_output();
    }

    /// The menu of what can be done to the selected operation, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> Option<AppAction> {
        Some(AppAction::SetPopup(Box::new(op_log_context_menu(
            get_env().jj_config.clone(),
            anchor,
            &self.selection(),
            &self.op_panel.selected,
        ))))
    }

    fn handle_event(&mut self, event: OpLogTabEvent) -> Result<Option<AppAction>> {
        match event {
            OpLogTabEvent::Restore => {
                return Ok(Some(command::ask_op_restore(
                    get_env().jj_config.clone(),
                    &self.op_panel.selected,
                )));
            }
            OpLogTabEvent::Revert => {
                return Ok(Some(command::ask_op_revert(
                    get_env().jj_config.clone(),
                    &self.op_panel.selected,
                )));
            }
            OpLogTabEvent::CopyId => {
                return Ok(Some(AppAction::Run(Command::Copy(
                    self.op_panel.selected.id.as_str().to_owned(),
                ))));
            }
            OpLogTabEvent::LoadMore => {
                self.limit = self.limit.saturating_mul(2);
                self.refresh_op_log();
            }
            // Not an operation of its own; the key handler deals with it.
            OpLogTabEvent::Unbound => {}
        }

        Ok(None)
    }
}

impl Tab for OpLogTab<'_> {
    fn refresh(&mut self) -> Result<()> {
        self.refresh_op_log();
        self.stale = false;

        Ok(())
    }

    fn mark_stale(&mut self) {
        self.stale = true;
    }

    fn config_changed(&mut self) {
        self.show_panel.config_changed();
        self.keybinds = OpLogTabKeybinds::new();
        self.details_keybinds = DetailsPanelKeybinds::new();
    }

    fn toggle_layout(&mut self) {
        self.pane_divider.toggle_layout();
    }

    fn is_stale(&self) -> bool {
        self.stale
    }

    fn drop_caches(&mut self) {
        self.show_panel.mark_dirty();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        self.scroll_operations(scroll.distance(self.op_panel.visible_items()));
        Ok(())
    }

    /// Go to the operation the repo is at, which is the one it is at as
    /// of the last read rather than as of now.
    fn focus_current(&mut self) -> Result<()> {
        if let Some(current) = self
            .op_panel
            .items()
            .into_iter()
            .find(|operation| operation.current)
        {
            self.op_panel.set_selected(current);
            self.sync_show_output();
        }

        Ok(())
    }

    fn open_context_menu(&self) -> Result<Option<AppAction>> {
        Ok(self.context_menu(self.op_panel.selected_position()))
    }

    fn selection(&self) -> Selection {
        Selection::default().operation(&self.op_panel.selected.id)
    }

    fn main_panel_bindings(&self) -> Vec<Binding> {
        self.keybinds.bindings()
    }

    fn details_panel_bindings(&self) -> Vec<Binding> {
        self.details_keybinds.bindings()
    }
}

impl Component for OpLogTab<'_> {
    fn update(&mut self) -> Result<Option<AppAction>> {
        self.show_panel.update();

        Ok(None)
    }

    fn task_done(&mut self, result: TaskResult) -> Result<Option<AppAction>> {
        if let TaskSlot::OpShow(_, request) = result.slot {
            self.show_panel.task_done(request, result.output);
        }
        Ok(None)
    }

    fn needs_periodic_redraw(&self) -> bool {
        self.show_panel.needs_periodic_redraw()
    }

    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = self.pane_divider.split(area);

        self.op_panel.draw(f, chunks[0])?;
        self.show_panel.draw(f, chunks[1]);

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
                    self.show_panel.handle_event(ev);
                    return Ok(ComponentInputResult::Handled);
                }
            }

            return match self.keybinds.match_event(key) {
                // Not the tab's to act on, so whoever else wants the key
                // is welcome to it.
                OpLogTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event)?.into()),
            };
        }

        Ok(ComponentInputResult::Handled)
    }

    fn input_mouse(&mut self, mouse: Mouse) -> Result<ComponentInputResult> {
        if self.pane_divider.handle_mouse(mouse) {
            return Ok(ComponentInputResult::Handled);
        }
        match route_mouse(mouse, &mut [&mut self.op_panel, &mut self.show_panel]) {
            MouseInput::Scroll(delta) => self.scroll_operations(delta),
            MouseInput::Select(index) => {
                if let Some(operation) = self.op_panel.item_at_log_line(index) {
                    self.op_panel.set_selected(operation);
                    self.sync_show_output();
                }
            }
            // The graph takes lines of its own, which name no operation
            // for a menu to act on.
            MouseInput::Context(index) => {
                if let Some(operation) = self.op_panel.item_at_log_line(index) {
                    self.op_panel.set_selected_in_place(operation);
                    self.sync_show_output();
                    return Ok(self.context_menu(Some(mouse.position())).into());
                }
            }
            MouseInput::Copy(text) => return Ok(copy_marked(text)),
            // Nothing here has a second thing a double click could do.
            MouseInput::Activate | MouseInput::Handled => {}
            MouseInput::NotHandled => return Ok(ComponentInputResult::NotHandled),
        }
        Ok(ComponentInputResult::Handled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_test_env;

    fn tab() -> OpLogTab<'static> {
        set_test_env();
        let (sender, _receiver) = std::sync::mpsc::channel();

        OpLogTab::new(BackgroundTasks::new(sender))
    }

    #[test]
    fn asking_for_more_operations_reads_further_back_every_time() -> Result<()> {
        let mut tab = tab();

        tab.handle_event(OpLogTabEvent::LoadMore)?;
        assert_eq!(tab.limit, INITIAL_LIMIT * 2);

        tab.handle_event(OpLogTabEvent::LoadMore)?;
        assert_eq!(tab.limit, INITIAL_LIMIT * 4);

        Ok(())
    }
}
