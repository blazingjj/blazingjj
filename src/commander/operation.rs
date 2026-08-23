/*!
[Commander] member functions related to jj operations.
*/
use tracing::instrument;

use crate::commander::CommandError;
use crate::commander::Commander;
use crate::commander::RemoveEndLine;
use crate::commander::ids::OperationId;

impl Commander {
    /// Get the operation the repo is at.
    /// Maps to `jj op log --limit 1 --no-graph -T id`
    #[instrument(level = "trace", skip(self))]
    pub fn get_operation_id(&self, ignore_working_copy: bool) -> Result<OperationId, CommandError> {
        let mut command = self.jj(["op", "log", "--limit", "1", "--no-graph", "-T", "id"]);
        if ignore_working_copy {
            command = command.ignore_working_copy();
        }

        Ok(OperationId(command.run()?.remove_end_line()))
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

        let operation_id = test_repo.commander.get_operation_id(true)?;
        assert!(!operation_id.0.is_empty());

        // Asking twice gives the same answer, so a caller can compare
        // against what it saw last without its own check moving the
        // operation on.
        assert_eq!(operation_id, test_repo.commander.get_operation_id(true)?);

        // A change to the working copy is only an operation once some
        // other command has snapshotted it, so it goes unnoticed here.
        fs::write(test_repo.directory.path().join("README"), b"AAA")?;
        assert_eq!(operation_id, test_repo.commander.get_operation_id(true)?);

        Ok(())
    }

    #[test]
    fn get_operation_id_snapshotting_working_copy() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let operation_id = test_repo.commander.get_operation_id(true)?;

        // Snapshotting a changed working copy is an operation of its
        // own, so the answer moves on.
        fs::write(test_repo.directory.path().join("README"), b"AAA")?;
        let snapshot_id = test_repo.commander.get_operation_id(false)?;
        assert_ne!(operation_id, snapshot_id);

        // With nothing left to snapshot, it stays where it is.
        assert_eq!(snapshot_id, test_repo.commander.get_operation_id(false)?);

        Ok(())
    }
}
