/*! What 'jj show' says about a change, as a details panel shows it.
*/

use super::output_cache::OutputKey;
use super::output_cache::OutputRequest;
use super::output_panel::OutputPanel;
use crate::app::TabId;
use crate::background_tasks::TaskOutput;
use crate::background_tasks::TaskSlot;
use crate::commander::cancel::CancelToken;
use crate::commander::ids::ChangeId;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::env::DiffFormat;

/// A details panel showing what 'jj show' says about a change.
pub type CommitShowPanel = OutputPanel<CommitShowKey>;

/// The change and formatting a 'jj show' output belongs to
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct CommitShowKey {
    /// Commit id of shown change
    id: Head,
    /// Formatting used to render change
    format: DiffFormat,
}

impl OutputKey for CommitShowKey {
    type Subject = Head;
    type Identity = ChangeId;

    const COMMAND: &'static str = "jj show";

    fn new(id: Head, format: DiffFormat) -> Self {
        Self { id, format }
    }

    fn format(&self) -> &DiffFormat {
        &self.format
    }

    fn identity(&self) -> Option<ChangeId> {
        (!self.id.divergent).then(|| self.id.change_id.clone())
    }

    fn render_width(&self, panel_width: usize) -> usize {
        self.format.render_width(panel_width)
    }

    fn run(&self, width: usize, cancel: &CancelToken) -> TaskOutput {
        let mut commander = new_commander();
        commander.limit_width(width);
        Ok(commander
            .build_jj_commit_show(&self.id.commit_id, &self.format, true)
            .color()
            .run_cancellable(cancel)?)
    }

    fn slot(owner: TabId, request: OutputRequest<Self>) -> TaskSlot {
        TaskSlot::CommitShow(owner, request)
    }
}
