/*!
[Commander] member functions related to jj workspaces.

This module has features to parse the `jj workspace list` output and to
pick the workspace we are running in out of it.
*/
use std::fs::canonicalize;
use std::path::Path;

use serde::Deserialize;
use tracing::instrument;

use crate::commander::CommandError;
use crate::commander::Commander;
use crate::commander::log::Head;
use crate::commander::log::head_template;

/// A workspace as [workspace_template] describes it. The field names are
/// the ones the template writes.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Workspace {
    pub name: String,
    /// Where the workspace is on disk, or None where the repo records no
    /// path for it -- a repo made before jj 0.38 records none for the
    /// workspace it was made in -- or the path it records no longer
    /// leads anywhere. The workspace we are running in always has one,
    /// that being where we are running.
    pub root: Option<String>,
    /// The working-copy commit of the workspace
    pub target: Head,
    /// Whether this is the workspace the app is running in, which is the
    /// one every command of ours goes to.
    #[serde(skip)]
    pub current: bool,
}

/// A workspace and what jj says about where it is being read from, as
/// [workspace_template] describes them.
#[derive(Deserialize)]
struct WorkspaceRecord {
    #[serde(flatten)]
    workspace: Workspace,
    /// Whether the change the workspace holds is the one the working
    /// copy is on, which for the workspace a command runs in is what jj
    /// says of it.
    on_working_copy: bool,
}

/// Template writing a [WorkspaceRecord] as a JSON object, one per line.
/// `escape_json()` keeps a name or path that needs quoting from ending
/// the object early, or the line.
fn workspace_template() -> String {
    let target = head_template("self.target()");

    format!(
        r#"
    '{{' ++ '"name":' ++ stringify(self.name()).escape_json()
    ++ ',"root":' ++ if(self.root(), stringify(self.root()).escape_json(), 'null')
    ++ ',"on_working_copy":' ++ self.target().current_working_copy()
    ++ ',"target":' ++ {target}
    ++ '}}' ++ "\n"
"#
    )
}

/// Whether both paths lead to the same directory, following whatever
/// links either of them goes through. A path that leads nowhere is only
/// the same as itself.
fn is_same_directory(one: &str, other: &str) -> bool {
    let resolve = |path: &str| canonicalize(Path::new(path)).ok();

    match (resolve(one), resolve(other)) {
        (Some(one), Some(other)) => one == other,
        _ => one == other,
    }
}

impl Commander {
    /// The workspace we are running in, which is none where the repo
    /// records no directory for any of them and none holds the change
    /// the working copy is on. Leaves the working copy alone.
    /// Maps to `jj workspace list --ignore-working-copy`
    #[instrument(level = "trace", skip(self))]
    pub fn get_current_workspace(&self) -> Result<Option<Workspace>, CommandError> {
        let current = self
            .jj(["workspace", "list", "-T", &workspace_template()])
            .ignore_working_copy()
            .run()?
            .lines()
            .filter_map(parse_workspace)
            .map(|record| self.reading(record))
            .find(|workspace| workspace.current);

        Ok(current)
    }

    /// The workspace `record` describes, with whether we are running in
    /// it settled.
    ///
    /// It is the directory that says so, that being what we run every
    /// command in. Where the repo records none, we go by the change the
    /// workspace holds, which is how jj itself reads a listing of a repo
    /// made before it recorded where a workspace is.
    fn reading(&self, record: WorkspaceRecord) -> Workspace {
        let mut workspace = record.workspace;
        workspace.current = match workspace.root.as_deref() {
            Some(root) => is_same_directory(root, &self.env.root),
            None => record.on_working_copy,
        };

        // Whatever the repo says, the workspace we are running in is
        // where we are running, which is all it takes to act on it.
        if workspace.current {
            workspace.root = Some(self.env.root.clone());
        }

        workspace
    }
}

/// Parse the [WorkspaceRecord] one line of [workspace_template] output
/// describes.
fn parse_workspace(text: &str) -> Option<WorkspaceRecord> {
    serde_json::from_str(text).ok()
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;
    use crate::commander::tests::TestRepo;

    #[test]
    fn a_new_repo_is_read_in_the_workspace_it_was_made_in() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let current = test_repo
            .commander
            .get_current_workspace()?
            .expect("the workspace we are running in");

        assert_eq!(current.name, "default");
        assert!(
            is_same_directory(
                current.root.as_deref().expect("a recorded root path"),
                &test_repo.commander.env.root
            ),
            "{current:?} is not the repo we made"
        );
        // The working-copy commit of a new repo is the one it starts on.
        assert_eq!(
            current.target,
            test_repo.commander.get_current_head()?,
            "the workspace holds a change other than the one it is on"
        );

        Ok(())
    }
}
