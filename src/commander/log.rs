/*!
[Commander] member functions related to jj log.

This module has features to parse the log output to extract change id and commit id.
It is mostly used in the [log_tab][crate::ui::log_tab] module.
*/

use std::fmt::Display;
use std::hash::Hash;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use itertools::Itertools;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tracing::instrument;

use crate::commander::CommandError;
use crate::commander::Commander;
use crate::commander::JjCommand;
use crate::commander::RemoveEndLine;
use crate::commander::ids::ChangeId;
use crate::commander::ids::CommitId;
use crate::commander::revset::Revset;
use crate::env::DiffFormat;

/// A change as [head_template] describes it. The field names are the ones
/// the template writes.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Deserialize)]
pub struct Head {
    pub change_id: ChangeId,
    pub commit_id: CommitId,
    pub divergent: bool,
    pub immutable: bool,
}

/// A parent of a commit, as [parents_template] describes it.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct Parent {
    #[serde(flatten)]
    pub head: Head,
    /// The first line of the parent's description, empty if it has none.
    pub description: String,
}

/// How many lines `builtin_log_compact` writes per item, which is what
/// [Commander::get_log] renders the graph with.
pub const LOG_LINES_PER_ITEM: usize = 2;

/// How many lines [EVOLOG_TEMPLATE] writes per item.
pub const EVOLOG_LINES_PER_ITEM: usize = 3;

/// What a log graph draws a line for, as the template behind it
/// describes the thing.
pub trait LogItem: Clone + Eq + Hash + DeserializeOwned {
    /// What the panel remembers a marked item by.
    type Mark: Clone + Eq + Hash;

    /// Whether the two are the same thing as the repo has held it at
    /// different times, so that a selection can follow it across a
    /// rewrite rather than fall back to the top of the log.
    fn same_subject(&self, other: &Self) -> bool;

    fn mark(&self) -> Self::Mark;
}

impl LogItem for Head {
    type Mark = CommitId;

    fn same_subject(&self, other: &Self) -> bool {
        self.change_id == other.change_id
    }

    fn mark(&self) -> CommitId {
        self.commit_id.clone()
    }
}

#[derive(Clone, Debug)]
pub struct LogOutput<T> {
    pub graph: String,
    // Maps graph line -> items
    pub graph_items: Vec<Option<T>>,
    pub items: Vec<T>,
}

impl<T> Default for LogOutput<T> {
    fn default() -> Self {
        Self {
            graph: String::new(),
            graph_items: Vec::new(),
            items: Vec::new(),
        }
    }
}

impl<T> LogOutput<T> {
    pub fn item_at(&self, line: usize) -> Option<&T> {
        self.graph_items.get(line).and_then(Option::as_ref)
    }
}

#[derive(Error, Debug)]
pub struct RecordParseError(String);

impl Display for RecordParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Record parse error: {}", self.0)
    }
}

/// Template writing the [Head] of the commit the template expression
/// `commit` names as a JSON object. `escape_json()` keeps a value that
/// needs quoting from ending the object early, or the line.
///
/// Naming the commit rather than taking the one in context lets a command
/// whose `self` is something else point at the commit it holds, as
/// `jj bookmark list` does.
pub(super) fn head_template(commit: &str) -> String {
    let fields = head_fields(commit);
    format!(r#"'{{' ++ {fields} ++ '}}'"#)
}

/// The fields [head_template] writes, without the braces around them, so
/// that a template describing more than a [Head] can add its own.
fn head_fields(commit: &str) -> String {
    format!(
        r#"
    '"change_id":' ++ stringify({commit}.change_id()).escape_json()
    ++ ',"commit_id":' ++ stringify({commit}.commit_id()).escape_json()
    ++ ',"divergent":' ++ {commit}.divergent()
    ++ ',"immutable":' ++ {commit}.immutable()
"#
    )
}

/// [head_template] for the commit in context, with a newline behind it so
/// that output holding more than one head has one object per line.
fn head_template_nl() -> String {
    let head = head_template("self");
    format!(r#"{head} ++ "\n""#)
}

/// Template writing one [Parent] per line for the parents of the commit
/// in context, in the order the commit names them.
fn parents_template() -> String {
    let fields = head_fields("parent");
    format!(
        r#"
    self.parents().map(|parent|
        '{{' ++ {fields}
        ++ ',"description":' ++ parent.description().first_line().escape_json()
        ++ '}}'
    ).join("\n")
"#
    )
}

/// Template rendering an evolog entry the way `builtin_evolog_compact`
/// does, except that the operation always takes a line of its own. The
/// builtin leaves it out for an entry recorded before jj tracked which
/// operation produced one, which would make entries take a varying number
/// of lines, and the heads read alongside them no longer line up with the
/// graph.
const EVOLOG_TEMPLATE: &str = r#"
    builtin_log_compact(commit)
    ++ separate(" ",
        label("separator", "--"),
        "operation",
        if(operation, operation.id().short() ++ " " ++ operation.description().first_line(), "unknown"),
    ) ++ "\n"
"#;

/// Parse the record one line of template output describes.
///
/// jj draws the graph in front of what the template writes, so the object
/// starts at the first brace of the line. A line the graph draws for edges
/// alone carries no template output, and neither does one for an elided
/// revision, so those have no brace and no record.
pub(super) fn parse_record<T: DeserializeOwned>(text: &str) -> Result<T> {
    text.find('{')
        .and_then(|start| serde_json::from_str(&text[start..]).ok())
        .ok_or_else(|| RecordParseError(text.to_owned()).into())
}

impl Commander {
    fn execute_jj_log(
        &self,
        revset: impl Into<Revset>,
        template: &str,
    ) -> Result<String, CommandError> {
        self.jj([
            "log",
            "--no-graph",
            "--template",
            template,
            "-r",
            revset.into().as_str(),
        ])
        .run()
    }

    fn execute_jj_log_one(
        &self,
        revset: impl Into<Revset>,
        template: &str,
    ) -> Result<String, CommandError> {
        self.jj([
            "log",
            "--no-graph",
            "--template",
            template,
            "-r",
            revset.into().as_str(),
            "--limit",
            "1",
        ])
        .run()
    }

    /// A graph and the item behind each of its lines.
    ///
    /// `command` is the whole invocation except `graph_template`, which
    /// draws the graph in `lines_per_item` lines per item, and
    /// `record_template` writes the item one of those lines belongs to.
    /// Leaves the working copy alone.
    pub(super) fn get_graph_log<T: LogItem>(
        &self,
        command: &[&str],
        graph_template: &str,
        record_template: &str,
        lines_per_item: usize,
    ) -> Result<LogOutput<T>, CommandError> {
        let graph = self
            .jj([command, &["--template", graph_template]].concat())
            .color()
            .ignore_working_copy()
            .run()?;

        // Read the graph once more, this time with a template describing
        // the item behind each of its lines, so that a graph line is an
        // index into the items. The root commit breaks that, taking a
        // single line however many the template writes, but it comes last
        // and so only leaves items past the end of the graph.
        let items_template =
            std::iter::repeat_n(record_template, lines_per_item).join(r#" ++ "\n" ++ "#);
        let graph_items: Vec<Option<T>> = self
            .jj([command, &["--template", &items_template]].concat())
            .ignore_working_copy()
            .run()?
            .lines()
            .map(|line| parse_record(line).ok())
            .collect();

        let items = graph_items.clone().into_iter().flatten().unique().collect();

        Ok(LogOutput {
            graph,
            graph_items,
            items,
        })
    }

    /// Get log. Returns human readable log and mapping to log line to item.
    /// Leaves the working copy alone.
    /// Maps to `jj log --ignore-working-copy`
    #[instrument(level = "trace", skip(self))]
    pub fn get_log(&self, revset: &Option<String>) -> Result<LogOutput<Head>, CommandError> {
        let mut command = vec!["log"];
        if let Some(revset) = revset {
            command.push("-r");
            command.push(revset);
        }

        self.get_graph_log(
            &command,
            "builtin_log_compact",
            &head_template("self"),
            LOG_LINES_PER_ITEM,
        )
    }

    /// Get the evolog of a commit: the versions it came out of, newest
    /// first. A squash folds two changes into one, so these are not all
    /// of one change. Leaves the working copy alone.
    /// Maps to `jj evolog -r <commit> --ignore-working-copy`
    #[instrument(level = "trace", skip(self))]
    pub fn get_evolog(&self, commit_id: &CommitId) -> Result<LogOutput<Head>, CommandError> {
        self.get_graph_log(
            &["evolog", "-r", commit_id.as_str()],
            EVOLOG_TEMPLATE,
            &head_template("commit"),
            EVOLOG_LINES_PER_ITEM,
        )
    }

    /// Create the JjCommand for the evolog entry of a commit, showing
    /// what the rewrite that produced it changed.
    ///
    /// The entry is asked for by commit id, which names it even once it is
    /// hidden, and the limit keeps the versions before it out of the
    /// output.
    #[instrument(level = "trace", skip(self))]
    pub fn build_jj_evolog_entry(
        &self,
        commit_id: &CommitId,
        diff_format: &DiffFormat,
        ignore_working_copy: bool,
    ) -> JjCommand {
        let args = vec![
            "evolog",
            "-r",
            commit_id.as_str(),
            "--limit",
            "1",
            "--no-graph",
            "--patch",
        ];

        let mut command = self.jj_diff(args, diff_format);
        if ignore_working_copy {
            command = command.ignore_working_copy();
        }

        command
    }

    /// Create the JjCommmand for `jj show <commit>`
    #[instrument(level = "trace", skip(self))]
    pub fn build_jj_commit_show(
        &self,
        commit_id: &CommitId,
        diff_format: &DiffFormat,
        ignore_working_copy: bool,
    ) -> JjCommand {
        let args = vec!["show", commit_id.as_str()];

        let mut command = self.jj_diff(args, diff_format);
        if ignore_working_copy {
            command = command.ignore_working_copy();
        }

        command
    }

    /// Get the current head.
    /// Maps to `jj log -r @`
    #[instrument(level = "trace", skip(self))]
    pub fn get_current_head(&self) -> Result<Head> {
        parse_record(
            &self
                .execute_jj_log_one(Revset::working_copy(), &head_template_nl())
                .context("Failed getting current head")?
                .remove_end_line(),
        )
    }

    /// Get the latest version of a head. Can detect evolution of divergent head.
    #[instrument(level = "trace", skip(self))]
    pub fn get_head_latest(&self, head: &Head) -> Result<Head> {
        // Get all heads which point to the same change ID
        let latest_heads_res =
            self.execute_jj_log(Revset::change(&head.change_id), &head_template_nl());
        let Ok(latest_heads_res) = latest_heads_res else {
            return self.get_head_latest(&self.get_current_head()?);
        };
        if latest_heads_res.is_empty() {
            return self.get_head_latest(&self.get_current_head()?);
        }
        let latest_heads: Vec<Head> = latest_heads_res
            .lines()
            .map(parse_record)
            .collect::<Result<Vec<Head>>>()?;

        // If the current head exist, that means it wasn't updated
        if let Some(head) = latest_heads.iter().find(|latest_head| latest_head == &head) {
            return Ok(head.to_owned());
        }

        // Check obslog for each head. If the obslog contains the head's commit, it means
        // there's a new commit for the head
        for latest_head in latest_heads.iter() {
            let parent_commits: Vec<ChangeId> = self
                .jj([
                    "obslog",
                    "--no-graph",
                    "--template",
                    r#"commit.change_id() ++ "\n""#,
                    "-r",
                    latest_head.commit_id.as_str(),
                ])
                .run()
                .context("Failed getting latest head parent commits")?
                .lines()
                .map(|line| ChangeId(line.to_owned()))
                .collect();

            if parent_commits
                .iter()
                .any(|parent_commit| parent_commit == &head.change_id)
            {
                return Ok(latest_head.to_owned());
            }
        }

        bail!(
            "Could not find head latest: {} {} {:?}",
            head.change_id,
            head.commit_id,
            latest_heads
        );
    }

    /// Get a commit's parents, in the order the commit names them. The
    /// root commit has none.
    /// Maps to `jj log -r <revision> -T 'self.parents()...'`
    #[instrument(level = "trace", skip(self))]
    pub fn get_commit_parents(&self, commit_id: &CommitId) -> Result<Vec<Parent>> {
        self.execute_jj_log_one(commit_id, &parents_template())
            .with_context(|| format!("Failed getting commit parents: {commit_id}"))?
            .lines()
            .map(parse_record)
            .collect()
    }

    /// Get a commit's first parent.
    #[instrument(level = "trace", skip(self))]
    pub fn get_commit_parent(&self, commit_id: &CommitId) -> Result<Head> {
        self.get_commit_parents(commit_id)?
            .into_iter()
            .next()
            .map(|parent| parent.head)
            .with_context(|| format!("Commit has no parent: {commit_id}"))
    }

    /// Get commit's description.
    /// Maps to `jj log -r <revision> -T description`
    #[instrument(level = "trace", skip(self))]
    pub fn get_commit_description(&self, commit_id: &CommitId) -> Result<String> {
        Ok(self
            .execute_jj_log_one(commit_id, "description")
            .with_context(|| format!("Failed getting commit description: {commit_id}"))?
            .remove_end_line())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use insta::assert_debug_snapshot;

    use super::*;
    use crate::commander::cancel::CancelToken;
    use crate::commander::tests::TestRepo;

    fn head(change_id: &str, commit_id: &str, divergent: bool, immutable: bool) -> Head {
        Head {
            change_id: ChangeId(change_id.to_owned()),
            commit_id: CommitId(commit_id.to_owned()),
            divergent,
            immutable,
        }
    }

    #[test]
    fn parse_record_reads_a_record_of_its_own() -> Result<()> {
        assert_eq!(
            parse_record::<Head>(
                r#"{"change_id":"kxq","commit_id":"1f2e","divergent":false,"immutable":true}"#
            )?,
            head("kxq", "1f2e", false, true)
        );

        Ok(())
    }

    #[test]
    fn parse_record_reads_a_record_behind_the_graph() -> Result<()> {
        assert_eq!(
            parse_record::<Head>(
                r#"│ ├─╮  {"change_id":"kxq","commit_id":"1f2e","divergent":true,"immutable":false}"#
            )?,
            head("kxq", "1f2e", true, false)
        );

        Ok(())
    }

    #[test]
    fn a_graph_line_without_a_record_has_no_head() {
        assert!(parse_record::<Head>("│ ├─╯").is_err());
        assert!(parse_record::<Head>("~  (elided revisions)").is_err());
        assert!(parse_record::<Head>("").is_err());
    }

    #[test]
    fn a_record_missing_a_field_is_no_head() {
        assert!(parse_record::<Head>(r#"{"change_id":"kxq","commit_id":"1f2e"}"#).is_err());
    }

    #[test]
    fn get_log() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let log = test_repo.commander.get_log(&None)?;

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(r"[k-z]{8} .*? [0-9a-fA-F]{8}", "[LINE]");
        let _bound = settings.bind_to_scope();

        assert_debug_snapshot!(log.graph);

        assert!(log.graph_items.iter().all(|graph_item| {
            graph_item
                .as_ref()
                .is_none_or(|graph_item| log.items.contains(graph_item))
        }));

        Ok(())
    }

    #[test]
    fn run_commit_show() -> Result<()> {
        let test_repo = TestRepo::new()?;

        fs::write(test_repo.directory.path().join("README"), b"AAA")?;

        let head = test_repo.commander.get_current_head()?;
        let show = test_repo
            .commander
            .build_jj_commit_show(&head.commit_id, &DiffFormat::ColorWords, false)
            .run_cancellable(&CancelToken::new())?
            .remove_end_line();

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(r"Commit ID: [0-9a-fA-F]{40}", "Commit ID: [COMMIT_ID]");
        settings.add_filter(r"Change ID: [k-z]{32}", "Change ID: [Change ID]");
        settings.add_filter(r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})", "([DATE_TIME])");
        let _bound = settings.bind_to_scope();

        assert_debug_snapshot!(show);

        Ok(())
    }

    #[test]
    fn get_evolog() -> Result<()> {
        let test_repo = TestRepo::new()?;

        fs::write(test_repo.directory.path().join("README"), b"AAA")?;
        test_repo.commander.jj(["describe", "-m", "first"]).run()?;
        let head = test_repo.commander.get_current_head()?;

        let evolog = test_repo.commander.get_evolog(&head.commit_id)?;

        // Every entry is a version of the change, the newest first
        assert_eq!(evolog.items.first(), Some(&head));
        assert!(evolog.items.len() > 1);
        assert!(
            evolog
                .items
                .iter()
                .all(|entry| entry.change_id == head.change_id)
        );

        // The items line up with the graph they were read alongside
        assert_eq!(evolog.graph.lines().count(), evolog.graph_items.len());
        assert!(evolog.graph_items.iter().all(Option::is_some));

        Ok(())
    }

    /// A squash gives the change a second line of predecessors, which the
    /// graph draws an edge for. That edge shares a line with the entry it
    /// belongs to, so the entries still take three lines each.
    #[test]
    fn get_evolog_of_a_squashed_change() -> Result<()> {
        let test_repo = TestRepo::new()?;

        fs::write(test_repo.directory.path().join("README"), b"AAA")?;
        test_repo.commander.jj(["describe", "-m", "first"]).run()?;
        test_repo.commander.jj(["new", "-m", "second"]).run()?;
        fs::write(test_repo.directory.path().join("README"), b"BBB")?;
        test_repo
            .commander
            .jj(["squash", "-u", "--into", "@-"])
            .run()?;
        let working_copy = test_repo.commander.get_current_head()?;
        let head = test_repo
            .commander
            .get_commit_parent(&working_copy.commit_id)?;

        let evolog = test_repo.commander.get_evolog(&head.commit_id)?;

        // The versions of the change squashed in are entries of their own
        assert!(
            evolog
                .items
                .iter()
                .any(|entry| entry.change_id != head.change_id)
        );

        assert_eq!(evolog.graph.lines().count(), evolog.graph_items.len());
        assert!(evolog.graph_items.iter().all(Option::is_some));

        Ok(())
    }

    #[test]
    fn run_evolog_entry() -> Result<()> {
        let test_repo = TestRepo::new()?;

        fs::write(test_repo.directory.path().join("README"), b"AAA")?;
        let head = test_repo.commander.get_current_head()?;
        let evolog = test_repo.commander.get_evolog(&head.commit_id)?;

        let entry = test_repo
            .commander
            .build_jj_evolog_entry(&head.commit_id, &DiffFormat::Git, false)
            .run_cancellable(&CancelToken::new())?;

        // The entry says what the rewrite that produced this version
        // changed, and nothing of the versions before it
        assert!(entry.contains(head.commit_id.short()));
        assert!(entry.contains("+AAA"));
        assert!(!entry.contains(evolog.items[1].commit_id.short()));

        Ok(())
    }

    #[test]
    fn get_commit_parent() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let head = test_repo.commander.get_current_head()?;

        assert_eq!(
            test_repo.commander.get_commit_parent(&head.commit_id)?,
            Head {
                commit_id: CommitId("0000000000000000000000000000000000000000".to_owned()),
                change_id: ChangeId("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz".to_owned()),
                divergent: false,
                immutable: true,
            }
        );

        Ok(())
    }

    #[test]
    fn get_commit_parents_of_a_merge() -> Result<()> {
        let test_repo = TestRepo::new()?;

        test_repo.commander.jj(["describe", "-m", "left"]).run()?;
        let left = test_repo.commander.get_current_head()?;

        test_repo
            .commander
            .jj(["new", "root()", "-m", "right"])
            .run()?;
        let right = test_repo.commander.get_current_head()?;

        test_repo
            .commander
            .jj(["new", left.commit_id.as_str(), right.commit_id.as_str()])
            .run()?;
        let merge = test_repo.commander.get_current_head()?;

        assert_eq!(
            test_repo.commander.get_commit_parents(&merge.commit_id)?,
            vec![
                Parent {
                    head: left,
                    description: "left".to_owned()
                },
                Parent {
                    head: right,
                    description: "right".to_owned()
                },
            ]
        );

        Ok(())
    }

    #[test]
    fn the_root_commit_has_no_parent() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let head = test_repo.commander.get_current_head()?;
        let root = test_repo.commander.get_commit_parent(&head.commit_id)?;

        assert!(
            test_repo
                .commander
                .get_commit_parents(&root.commit_id)?
                .is_empty()
        );
        assert!(
            test_repo
                .commander
                .get_commit_parent(&root.commit_id)
                .is_err()
        );

        Ok(())
    }

    #[test]
    fn get_head_latest() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let old_head = test_repo.commander.get_current_head()?;

        fs::write(test_repo.directory.path().join("README"), b"AAA")?;

        let new_head = test_repo.commander.get_current_head()?;

        assert_ne!(old_head, new_head);

        assert_eq!(new_head, test_repo.commander.get_head_latest(&old_head)?);

        Ok(())
    }
}
