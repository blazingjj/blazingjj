/*!
[Commander] member functions related to jj operations.
*/
use tracing::instrument;

use crate::commander::CommandError;
use crate::commander::Commander;
use crate::commander::RemoveEndLine;
use crate::commander::ids::OperationId;

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
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Result;

    use crate::commander::tests::TestRepo;

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
}
