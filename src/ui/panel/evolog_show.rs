/*! What one entry of 'jj evolog' changed, as a details panel shows it.
*/

use super::output_cache::OutputKey;
use super::output_cache::OutputRequest;
use super::output_panel::OutputPanel;
use crate::app::TabId;
use crate::background_tasks::TaskOutput;
use crate::background_tasks::TaskSlot;
use crate::commander::cancel::CancelToken;
use crate::commander::ids::CommitId;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::env::DiffFormat;

/// A details panel showing what one evolog entry changed.
pub type EvologShowPanel = OutputPanel<EvologShowKey>;

/// The evolog entry and formatting an output belongs to
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct EvologShowKey {
    /// The version of a change the entry records
    commit_id: CommitId,
    /// Formatting used to render the patch
    format: DiffFormat,
}

impl OutputKey for EvologShowKey {
    type Subject = Head;
    type Identity = CommitId;

    const COMMAND: &'static str = "jj evolog";

    fn new(entry: Head, format: DiffFormat) -> Self {
        Self {
            commit_id: entry.commit_id,
            format,
        }
    }

    /// An entry records a version the change has had, which no later
    /// rewrite alters, so the only output that stands in for it is the
    /// same entry rendered in another format.
    fn identity(&self) -> Option<CommitId> {
        Some(self.commit_id.clone())
    }

    fn render_width(&self, panel_width: usize) -> usize {
        self.format.render_width(panel_width)
    }

    fn run(&self, width: usize, cancel: &CancelToken) -> TaskOutput {
        let mut commander = new_commander();
        commander.limit_width(width);
        Ok(commander
            .build_jj_evolog_entry(&self.commit_id, &self.format, true)
            .color()
            .run_cancellable(cancel)?)
    }

    fn slot(owner: TabId, request: OutputRequest<Self>) -> TaskSlot {
        TaskSlot::EvologShow(owner, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander::ids::ChangeId;

    /// An entry of the evolog of change "change"
    fn key(commit_id: &str, format: DiffFormat) -> EvologShowKey {
        EvologShowKey::new(
            Head {
                change_id: ChangeId("change".to_owned()),
                commit_id: CommitId(commit_id.to_owned()),
                divergent: false,
                immutable: false,
            },
            format,
        )
    }

    #[test]
    fn a_version_stands_in_for_itself_in_another_format() {
        let color_words = key("commit", DiffFormat::ColorWords);
        let git = key("commit", DiffFormat::Git);

        assert_ne!(color_words, git);
        assert_eq!(color_words.identity(), git.identity());
    }

    #[test]
    fn another_version_of_the_change_is_an_entry_of_its_own() {
        assert_ne!(
            key("commit", DiffFormat::Git).identity(),
            key("previous", DiffFormat::Git).identity()
        );
    }
}
