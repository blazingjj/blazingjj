/*! What 'jj op show' says one operation did to the repo, as a details
panel shows it.
*/

use super::output_cache::OutputKey;
use super::output_cache::OutputRequest;
use super::output_panel::OutputPanel;
use crate::app::TabId;
use crate::background_tasks::TaskOutput;
use crate::background_tasks::TaskSlot;
use crate::commander::cancel::CancelToken;
use crate::commander::ids::OperationId;
use crate::commander::new_commander;
use crate::commander::operation::Operation;
use crate::env::DiffFormat;

/// A details panel showing what one operation did.
pub type OpShowPanel = OutputPanel<OpShowKey>;

/// The operation and formatting an output belongs to
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct OpShowKey {
    id: OperationId,
    /// Formatting used to render the patches
    format: DiffFormat,
}

impl OutputKey for OpShowKey {
    type Subject = Operation;
    type Identity = OperationId;

    const COMMAND: &'static str = "jj op show";

    fn new(operation: Operation, format: DiffFormat) -> Self {
        Self {
            id: operation.id,
            format,
        }
    }

    fn format(&self) -> &DiffFormat {
        &self.format
    }

    /// An operation records what the repo went through, which nothing
    /// later alters, so the only output that stands in for it is the same
    /// operation rendered in another format.
    fn identity(&self) -> Option<OperationId> {
        Some(self.id.clone())
    }

    fn render_width(&self, panel_width: usize) -> usize {
        self.format.render_width(panel_width)
    }

    fn run(&self, width: usize, cancel: &CancelToken) -> TaskOutput {
        let mut commander = new_commander();
        commander.limit_width(width);
        Ok(commander
            .build_jj_op_show(&self.id, &self.format)
            .color()
            .run_cancellable(cancel)?)
    }

    fn slot(owner: TabId, request: OutputRequest<Self>) -> TaskSlot {
        TaskSlot::OpShow(owner, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The operation of this id, as the op log lists it
    fn operation(id: &str) -> Operation {
        Operation {
            id: OperationId(id.to_owned()),
            description: "describe commit".to_owned(),
            current: false,
            root: false,
        }
    }

    fn key(id: &str, format: DiffFormat) -> OpShowKey {
        OpShowKey::new(operation(id), format)
    }

    #[test]
    fn an_operation_stands_in_for_itself_in_another_format() {
        let color_words = key("operation", DiffFormat::ColorWords);
        let git = key("operation", DiffFormat::Git);

        assert_ne!(color_words, git);
        assert_eq!(color_words.identity(), git.identity());
    }

    #[test]
    fn another_operation_is_an_output_of_its_own() {
        assert_ne!(
            key("operation", DiffFormat::Git).identity(),
            key("earlier", DiffFormat::Git).identity()
        );
    }
}
