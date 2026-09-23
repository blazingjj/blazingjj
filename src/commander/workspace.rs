/*!
[Commander] member functions related to jj workspaces.

This module has features to parse the `jj workspace list` output and the
commands that add, forget and rename a workspace.
*/
use std::fs::canonicalize;
use std::path::Path;

use itertools::Itertools;
use serde::Deserialize;
use tracing::instrument;

use crate::commander::CommandError;
use crate::commander::Commander;
use crate::commander::log::Head;
use crate::commander::log::RecordParseError;
use crate::commander::log::head_template;

/// A workspace as [record_template] describes it. The field names are
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
/// [record_template] describes them.
#[derive(Deserialize)]
struct WorkspaceRecord {
    #[serde(flatten)]
    workspace: Workspace,
    /// Whether the change the workspace holds is the one the working
    /// copy is on, which for the workspace a command runs in is what jj
    /// says of it.
    on_working_copy: bool,
}

/// Template writing a [WorkspaceRecord] as a JSON object. `escape_json()`
/// keeps a name or path that needs quoting from ending the object early,
/// or the line.
fn record_template() -> String {
    let target = head_template("self.target()");

    format!(
        r#"
    '{{' ++ '"name":' ++ stringify(self.name()).escape_json()
    ++ ',"root":' ++ if(self.root(), stringify(self.root()).escape_json(), 'null')
    ++ ',"on_working_copy":' ++ self.target().current_working_copy()
    ++ ',"target":' ++ {target}
    ++ '}}'
"#
    )
}

/// [record_template], one per line.
fn workspace_template() -> String {
    format!(r#"{} ++ "\n""#, record_template())
}

/// An entry of the workspace listing: what jj writes for it, and the
/// workspace it describes.
#[derive(Debug)]
pub struct WorkspaceLine {
    pub text: String,
    pub workspace: Workspace,
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
    /// Get the workspaces attached to the repo, in the order jj lists
    /// them. Leaves the working copy alone.
    /// Maps to `jj workspace list --ignore-working-copy`
    #[instrument(level = "trace", skip(self))]
    pub fn get_workspaces(&self) -> anyhow::Result<Vec<WorkspaceLine>> {
        // One run for both, so an entry and its record always describe
        // the same workspace. `stringify()` keeps colour out of the record.
        let template = format!(
            r#"stringify({}) ++ "\0" ++ {} ++ "\0""#,
            record_template(),
            self.env.jj_config.workspace_list_template()
        );
        let output = self
            .jj(["workspace", "list", "-T", &template])
            .color()
            .ignore_working_copy()
            .run()?;

        output
            .split_terminator('\0')
            .tuples()
            .map(|(record, text)| {
                let record =
                    parse_workspace(record).ok_or_else(|| RecordParseError(record.to_owned()))?;

                Ok(WorkspaceLine {
                    text: text.trim_end_matches('\n').to_owned(),
                    workspace: self.reading(record),
                })
            })
            .collect()
    }

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

    /// Add a workspace at `destination`, named after the directory it
    /// creates. A relative destination is taken from the workspace we
    /// are running in.
    /// Maps to `jj workspace add <destination>`
    #[instrument(level = "trace", skip(self))]
    pub fn run_workspace_add(&self, destination: &str) -> Result<(), CommandError> {
        self.jj(["workspace", "add", destination]).run_void()
    }

    /// Stop tracking the working-copy commit of the workspace of this
    /// name, leaving whatever is on disk alone.
    /// Maps to `jj workspace forget <name>`
    #[instrument(level = "trace", skip(self))]
    pub fn run_workspace_forget(&self, name: &str) -> Result<(), CommandError> {
        self.jj(["workspace", "forget", name]).run_void()
    }

    /// Rename the workspace whose root is `root` to `new_name`.
    /// Maps to `jj workspace rename <new_name>`, run in that workspace
    #[instrument(level = "trace", skip(self))]
    pub fn run_workspace_rename(&self, root: &str, new_name: &str) -> Result<(), CommandError> {
        // jj renames the workspace the command is run in, so the one to
        // rename is the one we run it in.
        let mut commander = self.clone();
        commander.env.root = root.to_owned();

        commander.jj(["workspace", "rename", new_name]).run_void()
    }
}

/// Parse the [WorkspaceRecord] one [record_template] output describes.
fn parse_workspace(text: &str) -> Option<WorkspaceRecord> {
    serde_json::from_str(text).ok()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Result;

    use super::*;
    use crate::commander::tests::TestRepo;

    /// The workspaces of the repo, those we can make out of the listing.
    fn workspaces(test_repo: &TestRepo) -> Result<Vec<Workspace>> {
        Ok(test_repo
            .commander
            .get_workspaces()?
            .into_iter()
            .map(|line| line.workspace)
            .collect())
    }

    /// The names of the workspaces of the repo, in the order jj lists
    /// them.
    fn names(test_repo: &TestRepo) -> Result<Vec<String>> {
        Ok(workspaces(test_repo)?
            .into_iter()
            .map(|workspace| workspace.name)
            .collect())
    }

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

    #[test]
    fn a_workspace_is_added_under_the_name_of_its_directory() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let added = test_repo.directory.path().join("elsewhere");

        test_repo
            .commander
            .run_workspace_add(&added.to_string_lossy())?;

        let listed = test_repo.commander.get_workspaces()?;
        let [ours, theirs] = listed.as_slice() else {
            panic!("the repo has two workspaces, got {listed:?}");
        };
        let (ours, theirs) = (&ours.workspace, &theirs.workspace);

        assert_eq!(ours.name, "default");
        assert!(ours.current);
        assert_eq!(theirs.name, "elsewhere");
        assert!(!theirs.current);
        assert!(is_same_directory(
            theirs.root.as_deref().expect("a recorded root path"),
            &added.to_string_lossy()
        ));
        assert_ne!(theirs.target, ours.target);
        // Each entry is shown as jj writes the workspace it describes.
        assert!(listed[1].text.contains("elsewhere"), "{listed:?}");

        Ok(())
    }

    #[test]
    fn a_forgotten_workspace_is_no_longer_listed() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let added = test_repo.directory.path().join("elsewhere");
        test_repo
            .commander
            .run_workspace_add(&added.to_string_lossy())?;

        test_repo.commander.run_workspace_forget("elsewhere")?;

        assert_eq!(names(&test_repo)?, ["default"]);

        Ok(())
    }

    #[test]
    fn a_workspace_other_than_the_current_one_is_renamed_where_it_is() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let added = test_repo.directory.path().join("elsewhere");
        test_repo
            .commander
            .run_workspace_add(&added.to_string_lossy())?;

        test_repo
            .commander
            .run_workspace_rename(&added.to_string_lossy(), "renamed")?;

        assert_eq!(names(&test_repo)?, ["default", "renamed"]);

        Ok(())
    }

    #[test]
    fn the_current_workspace_is_renamed_where_the_app_runs() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let root = test_repo.commander.env.root.clone();

        test_repo.commander.run_workspace_rename(&root, "renamed")?;

        assert_eq!(names(&test_repo)?, ["renamed"]);
        assert!(
            workspaces(&test_repo)?[0].current,
            "the renamed workspace is still the one we are in"
        );

        Ok(())
    }

    /// A repo made before jj recorded where a workspace is records no
    /// directory for any of them; ours is still known by where we run.
    #[test]
    fn the_workspace_we_are_in_is_known_by_the_directory_we_run_in() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let added = test_repo.directory.path().join("elsewhere");
        test_repo
            .commander
            .run_workspace_add(&added.to_string_lossy())?;

        // As a repo that has recorded no path for any of its workspaces
        // reads.
        fs::write(
            Path::new(&test_repo.commander.env.root).join(".jj/repo/workspace_store/index"),
            b"",
        )?;

        let workspaces = workspaces(&test_repo)?;
        let [ours, theirs] = workspaces.as_slice() else {
            panic!("the repo has two workspaces, got {workspaces:?}");
        };

        assert!(ours.current);
        assert!(is_same_directory(
            ours.root.as_deref().expect("the directory we run in"),
            &test_repo.commander.env.root
        ));
        // There is nothing left to say where the other one is.
        assert!(!theirs.current);
        assert_eq!(theirs.root, None);

        Ok(())
    }
}
