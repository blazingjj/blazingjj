/*!
[Commander] member functions related to jj operations.
*/
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use tracing::instrument;

use crate::commander::CommandError;
use crate::commander::Commander;
use crate::commander::JjCommand;
use crate::commander::RemoveEndLine;
use crate::commander::ids::OperationId;
use crate::commander::log::LogItem;
use crate::commander::log::LogOutput;
use crate::env::DiffFormat;

/// How many lines [OP_LOG_TEMPLATE] writes per operation.
pub const OP_LOG_LINES_PER_ITEM: usize = 2;

/// Template rendering an operation as `builtin_op_log_compact` does,
/// except that it stops at the first line of what the operation says of
/// itself and leaves out the command line that produced it.
///
/// Both of those run to as many lines as they please -- the command line
/// of a `jj commit` carries the whole message -- and every operation has
/// to take the same number of lines, or the operations read alongside the
/// graph no longer line up with it. The command line is in `jj op show`
/// as well, which the details panel has.
const OP_LOG_TEMPLATE: &str = r#"
    label(if(self.current_operation(), "current_operation"),
        separate(" ",
            format_short_operation_id(self.id()),
            self.user(),
            self.workspace_name(),
            format_time_range(self.time()),
        ) ++ "\n"
        ++ if(self.root(),
            label("root", "root()"),
            self.description().first_line(),
        ) ++ "\n"
    )
"#;

/// An entry of the operation log, as [operation_template] describes it.
/// The field names are the ones the template writes.
#[derive(Clone, Default, PartialEq, Eq, Hash, Debug, Deserialize)]
pub struct Operation {
    pub id: OperationId,
    /// The first line of what the operation says of itself, empty when it
    /// says nothing.
    pub description: String,
    /// Whether the repo is at this operation
    pub current: bool,
    /// Whether this is the operation the repo began at
    pub root: bool,
}

impl LogItem for Operation {
    type Mark = OperationId;

    /// The operation log only ever grows, so an entry is never another
    /// version of anything.
    fn same_subject(&self, other: &Self) -> bool {
        self == other
    }

    fn mark(&self) -> OperationId {
        self.id.clone()
    }
}

/// Template writing the [Operation] in context as a JSON object.
/// `escape_json()` keeps a description that needs quoting from ending the
/// object early, or the line.
fn operation_template() -> String {
    r#"
    '{' ++ '"id":' ++ stringify(self.id()).escape_json()
    ++ ',"description":' ++ self.description().first_line().escape_json()
    ++ ',"current":' ++ self.current_operation()
    ++ ',"root":' ++ self.root()
    ++ '}'
"#
    .to_owned()
}

impl Commander {
    /// Get the operation the repo is at, snapshotting the working copy
    /// first unless the commander leaves it alone. Snapshotting makes the
    /// two a single step, so no snapshot of ours lands after the answer.
    /// Maps to `jj op log --limit 1 --no-graph -T id`
    #[instrument(level = "trace", skip(self))]
    pub fn get_operation_id(&self) -> Result<OperationId, CommandError> {
        Ok(OperationId(
            self.jj(["op", "log", "--limit", "1", "--no-graph", "-T", "id"])
                .run()?
                .remove_end_line(),
        ))
    }

    /// Get the operation log: what the repo has been through, newest
    /// first, at most `limit` operations of it. Leaves the working copy
    /// alone, so that reading it is not an operation of its own.
    /// Maps to `jj op log -n <limit> --ignore-working-copy`
    #[instrument(level = "trace", skip(self))]
    pub fn get_op_log(&self, limit: usize) -> Result<LogOutput<Operation>, CommandError> {
        let limit = limit.to_string();

        self.get_graph_log(
            &["op", "log", "-n", &limit],
            OP_LOG_TEMPLATE,
            &operation_template(),
            OP_LOG_LINES_PER_ITEM,
        )
    }

    /// Create the JjCommand showing what an operation did to the repo,
    /// down to the patches of the changes it touched. Leaves the working
    /// copy alone.
    #[instrument(level = "trace", skip(self))]
    pub fn build_jj_op_show(&self, id: &OperationId, diff_format: &DiffFormat) -> JjCommand {
        let mut args = vec!["op", "show", id.as_str(), "--patch"];
        args.append(&mut diff_format.get_args());

        self.jj(args).ignore_working_copy()
    }

    /// Take the repo back to the state an operation left it in.
    /// Maps to `jj op restore <id>`
    #[instrument(level = "trace", skip(self))]
    pub fn run_op_restore(&self, id: &OperationId) -> Result<()> {
        self.jj(["op", "restore", id.as_str()])
            .run_void()
            .context("Failed executing jj op restore")
    }

    /// Take back a single operation, leaving whatever happened after it
    /// in place.
    /// Maps to `jj op revert <id>`
    #[instrument(level = "trace", skip(self))]
    pub fn run_op_revert(&self, id: &OperationId) -> Result<()> {
        self.jj(["op", "revert", id.as_str()])
            .run_void()
            .context("Failed executing jj op revert")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Result;

    use super::*;
    use crate::commander::cancel::CancelToken;
    use crate::commander::tests::TestRepo;

    /// A repo that has been through a few operations, the newest of them
    /// a description of the working copy commit.
    fn worked_in() -> Result<TestRepo> {
        let test_repo = TestRepo::new()?;

        fs::write(test_repo.directory.path().join("README"), b"AAA")?;
        test_repo.commander.jj(["describe", "-m", "first"]).run()?;
        test_repo.commander.jj(["new"]).run()?;
        // A description of several lines, which the command line of the
        // operation that made it carries in full.
        test_repo
            .commander
            .jj(["describe", "-m", "second\n\nwith a body"])
            .run()?;

        Ok(test_repo)
    }

    #[test]
    fn get_operation_id_ignoring_working_copy() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let mut checking = test_repo.commander.clone();
        checking.ignore_working_copy();

        let operation_id = checking.get_operation_id()?;
        assert!(!operation_id.0.is_empty());

        // Asking twice gives the same answer, so a caller can compare
        // against what it saw last without its own check moving the
        // operation on.
        assert_eq!(operation_id, checking.get_operation_id()?);

        // A change to the working copy is only an operation once some
        // other command has snapshotted it, so it goes unnoticed here.
        fs::write(test_repo.directory.path().join("README"), b"AAA")?;
        assert_eq!(operation_id, checking.get_operation_id()?);

        Ok(())
    }

    #[test]
    fn get_operation_id_snapshotting_working_copy() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let operation_id = test_repo.commander.get_operation_id()?;

        // Snapshotting a changed working copy is an operation of its
        // own, so the answer moves on.
        fs::write(test_repo.directory.path().join("README"), b"AAA")?;
        let snapshot_id = test_repo.commander.get_operation_id()?;
        assert_ne!(operation_id, snapshot_id);

        // With nothing left to snapshot, it stays where it is.
        assert_eq!(snapshot_id, test_repo.commander.get_operation_id()?);

        Ok(())
    }

    #[test]
    fn get_op_log_reads_the_operations_newest_first() -> Result<()> {
        let test_repo = worked_in()?;

        let op_log = test_repo.commander.get_op_log(100)?;

        // The newest operation is the one the repo is at, and the oldest
        // the one it began at
        let current = op_log.items.first().expect("an operation");
        assert!(current.current);
        assert!(current.description.starts_with("describe commit"));
        assert!(op_log.items.last().expect("the root operation").root);
        assert_eq!(op_log.items.iter().filter(|entry| entry.current).count(), 1);

        // The operations line up with the graph they were read alongside,
        // which the template pads so that every one of them takes the
        // same number of lines.
        assert_eq!(op_log.graph.lines().count(), op_log.graph_items.len());
        assert_eq!(
            op_log.graph_items.len(),
            op_log.items.len() * OP_LOG_LINES_PER_ITEM
        );
        assert!(op_log.graph_items.iter().all(Option::is_some));
        assert!(
            op_log
                .graph
                .lines()
                .next()
                .expect("a first line")
                .contains(current.id.short())
        );

        Ok(())
    }

    #[test]
    fn get_op_log_reads_no_more_operations_than_it_is_asked_for() -> Result<()> {
        let test_repo = worked_in()?;

        assert_eq!(test_repo.commander.get_op_log(2)?.items.len(), 2);

        Ok(())
    }

    #[test]
    fn get_op_log_leaves_the_working_copy_alone() -> Result<()> {
        let test_repo = worked_in()?;

        let mut checking = test_repo.commander.clone();
        checking.ignore_working_copy();
        let before = checking.get_operation_id()?;

        // A change to the working copy would be an operation of its own
        // once something snapshotted it, which reading the operation log
        // is not.
        fs::write(test_repo.directory.path().join("README"), b"BBB")?;
        test_repo.commander.get_op_log(100)?;

        assert_eq!(before, checking.get_operation_id()?);

        Ok(())
    }

    #[test]
    fn op_show_says_what_the_operation_did() -> Result<()> {
        let test_repo = worked_in()?;
        let current = test_repo.commander.get_operation_id()?;

        let show = test_repo
            .commander
            .build_jj_op_show(&current, &DiffFormat::Git)
            .run_cancellable(&CancelToken::new())?;

        assert!(show.contains(current.short()));
        assert!(show.contains("describe commit"));

        Ok(())
    }

    /// What the working copy commit and the one before it say of
    /// themselves.
    fn descriptions(test_repo: &TestRepo) -> Result<(String, String)> {
        let head = test_repo.commander.get_current_head()?;
        let parent = test_repo.commander.get_commit_parent(&head.commit_id)?;

        Ok((
            test_repo
                .commander
                .get_commit_description(&head.commit_id)?,
            test_repo
                .commander
                .get_commit_description(&parent.commit_id)?,
        ))
    }

    #[test]
    fn run_op_restore_takes_the_repo_back_to_an_operation() -> Result<()> {
        let test_repo = worked_in()?;

        // The operation before the description of the working copy commit
        // left it without one.
        let before = &test_repo.commander.get_op_log(2)?.items[1];
        test_repo.commander.run_op_restore(&before.id)?;

        assert_eq!(
            descriptions(&test_repo)?,
            ("".to_owned(), "first".to_owned())
        );

        Ok(())
    }

    #[test]
    fn run_op_revert_takes_back_a_single_operation() -> Result<()> {
        let test_repo = worked_in()?;

        // Reverting the description of the change made before the working
        // copy commit leaves the working copy commit's own alone. Two
        // operations described a commit, the older of them that one.
        let op_log = test_repo.commander.get_op_log(100)?;
        let described_first = op_log
            .items
            .iter()
            .rev()
            .find(|entry| entry.description.starts_with("describe commit"))
            .expect("the operation that described the first commit");
        test_repo.commander.run_op_revert(&described_first.id)?;

        assert_eq!(
            descriptions(&test_repo)?,
            ("second\n\nwith a body".to_owned(), "".to_owned())
        );

        Ok(())
    }
}
