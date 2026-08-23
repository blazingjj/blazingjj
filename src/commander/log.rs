/*!
[Commander] member functions related to jj log.

This module has features to parse the log output to extract change id and commit id.
It is mostly used in the [log_tab][crate::ui::log_tab] module.
*/

use std::fmt::Display;
use std::process::Child;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use itertools::Itertools;
use serde::Deserialize;
use thiserror::Error;
use tracing::instrument;

use crate::commander::CommandError;
use crate::commander::Commander;
use crate::commander::JjCommand;
use crate::commander::RemoveEndLine;
use crate::commander::bookmarks::Bookmark;
use crate::commander::ids::ChangeId;
use crate::commander::ids::CommitId;
use crate::env::DiffFormat;

/// A change as [HEAD_TEMPLATE] describes it. The field names are the ones
/// the template writes.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Deserialize)]
pub struct Head {
    pub change_id: ChangeId,
    pub commit_id: CommitId,
    pub divergent: bool,
    pub immutable: bool,
}

#[derive(Clone, Debug)]
pub struct LogOutput {
    pub graph: String,
    // Maps graph line -> heads
    pub graph_heads: Vec<Option<Head>>,
    pub heads: Vec<Head>,
}

impl LogOutput {
    pub fn head_at(&self, line: usize) -> Option<&Head> {
        self.graph_heads.get(line).and_then(Option::as_ref)
    }
}

#[derive(Error, Debug)]
pub struct HeadParseError(String);

impl Display for HeadParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Head parse error: {}", self.0)
    }
}

/// Template writing a [Head] as a JSON object, for `jj log` and the other
/// commands that take a template. `escape_json()` keeps a value that needs
/// quoting from ending the object early, or the line.
const HEAD_TEMPLATE: &str = r#"
    '{"change_id":' ++ stringify(change_id).escape_json()
    ++ ',"commit_id":' ++ stringify(commit_id).escape_json()
    ++ ',"divergent":' ++ divergent
    ++ ',"immutable":' ++ immutable
    ++ '}'
"#;
/// [HEAD_TEMPLATE] with a newline behind it, so that output holding more
/// than one head has one object per line.
const HEAD_TEMPLATE_NL: &str = r#"
    '{"change_id":' ++ stringify(change_id).escape_json()
    ++ ',"commit_id":' ++ stringify(commit_id).escape_json()
    ++ ',"divergent":' ++ divergent
    ++ ',"immutable":' ++ immutable
    ++ '}'
    ++ "\n"
"#;

/// Parse the [Head] one line of [HEAD_TEMPLATE] output describes.
///
/// jj draws the graph in front of what the template writes, so the object
/// starts at the first brace of the line. A line the graph draws for edges
/// alone carries no template output, and neither does one for an elided
/// revision, so those have no brace and no head.
fn parse_head(text: &str) -> Result<Head> {
    text.find('{')
        .and_then(|start| serde_json::from_str(&text[start..]).ok())
        .ok_or_else(|| HeadParseError(text.to_owned()).into())
}

impl Commander {
    fn execute_jj_log(&self, revset: &str, template: &str) -> Result<String, CommandError> {
        self.jj(["log", "--no-graph", "--template", template, "-r", revset])
            .run()
    }

    fn execute_jj_log_one(&self, revset: &str, template: &str) -> Result<String, CommandError> {
        self.jj([
            "log",
            "--no-graph",
            "--template",
            template,
            "-r",
            revset,
            "--limit",
            "1",
        ])
        .run()
    }

    /// Get log. Returns human readable log and mapping to log line to head.
    /// Maps to `jj log`
    #[instrument(level = "trace", skip(self))]
    pub fn get_log(&self, revset: &Option<String>) -> Result<LogOutput, CommandError> {
        let mut args = vec![];

        if let Some(revset) = revset {
            args.push("-r");
            args.push(revset);
        }

        // Force builtin_log_compact which uses 2 lines per change
        let graph = self
            .jj([
                vec!["log", "--template", "builtin_log_compact"],
                args.clone(),
            ]
            .concat())
            .color()
            .run()?;

        // Extract the log one more time, but this time use a template
        // which describes the head behind each line. Since jj has
        // 2 lines per change, there will also be two lines with head info.
        // The number of lines in graph and the number of items in graph_heads
        // should be identical.
        let graph_heads: Vec<Option<Head>> = self
            .jj([
                vec![
                    "log",
                    "--template",
                    // Match builtin_log_compact with 2 lines per change
                    &format!(r#"{HEAD_TEMPLATE} ++ "\n" ++ {HEAD_TEMPLATE}"#),
                ],
                args,
            ]
            .concat())
            .run()?
            .lines()
            .map(|line| parse_head(line).ok())
            .collect();

        let heads = graph_heads.clone().into_iter().flatten().unique().collect();

        Ok(LogOutput {
            graph,
            graph_heads,
            heads,
        })
    }

    /// Spawn child process to get commit details.
    /// Maps to `jj show <commit>`
    #[instrument(level = "trace", skip(self))]
    pub fn spawn_commit_show(
        &self,
        commit_id: &CommitId,
        diff_format: &DiffFormat,
        ignore_working_copy: bool,
    ) -> Result<Child, CommandError> {
        self.build_jj_commit_show(commit_id, diff_format, ignore_working_copy)
            .color()
            .spawn()
    }

    /// Create the JjCommmand for `jj show <commit>`
    #[instrument(level = "trace", skip(self))]
    pub fn build_jj_commit_show(
        &self,
        commit_id: &CommitId,
        diff_format: &DiffFormat,
        ignore_working_copy: bool,
    ) -> JjCommand<'_> {
        let mut args = vec!["show", commit_id.as_str()];
        args.append(&mut diff_format.get_args());

        let mut command = self.jj(args);
        if ignore_working_copy {
            command = command.ignore_working_copy();
        }

        command
    }

    /// Get the current head.
    /// Maps to `jj log -r @`
    #[instrument(level = "trace", skip(self))]
    pub fn get_current_head(&self) -> Result<Head> {
        parse_head(
            &self
                .execute_jj_log_one("@", HEAD_TEMPLATE_NL)
                .context("Failed getting current head")?
                .remove_end_line(),
        )
    }

    /// Get the latest version of a head. Can detect evolution of divergent head.
    #[instrument(level = "trace", skip(self))]
    pub fn get_head_latest(&self, head: &Head) -> Result<Head> {
        // Get all heads which point to the same change ID
        let latest_heads_res = self.execute_jj_log(
            &format!(r#"change_id({})"#, head.change_id.as_str()),
            HEAD_TEMPLATE_NL,
        );
        let Ok(latest_heads_res) = latest_heads_res else {
            return self.get_head_latest(&self.get_current_head()?);
        };
        if latest_heads_res.is_empty() {
            return self.get_head_latest(&self.get_current_head()?);
        }
        let latest_heads: Vec<Head> = latest_heads_res
            .lines()
            .map(parse_head)
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

    /// Get a commit's parent.
    /// Maps to `jj log -r <revision>-`
    #[instrument(level = "trace", skip(self))]
    pub fn get_commit_parent(&self, commit_id: &CommitId) -> Result<Head> {
        parse_head(
            &self
                .execute_jj_log_one(&format!("{commit_id}-"), HEAD_TEMPLATE_NL)
                .with_context(|| format!("Failed getting commit parent: {commit_id}"))?
                .remove_end_line(),
        )
    }

    /// Get commit's description.
    /// Maps to `jj log -r <revision> -T description`
    #[instrument(level = "trace", skip(self))]
    pub fn get_commit_description(&self, commit_id: &CommitId) -> Result<String> {
        Ok(self
            .execute_jj_log_one(commit_id.as_str(), "description")
            .with_context(|| format!("Failed getting commit description: {commit_id}"))?
            .remove_end_line())
    }

    /// Check if a revision is immutable
    /// Maps to `jj log -r <revision> -T immutable`
    #[instrument(level = "trace", skip(self))]
    pub fn check_revision_immutable(&self, revision: &str) -> Result<bool> {
        Ok(self
            .execute_jj_log_one(revision, "immutable")
            .with_context(|| format!("Failed checking if revision is immutable: {revision}"))?
            .remove_end_line()
            == "true")
    }

    /// Get bookmark head
    /// Maps to `jj log -r <bookmark>[@<remote>]`
    #[instrument(level = "trace", skip(self))]
    pub fn get_bookmark_head(&self, bookmark: &Bookmark) -> Result<Head> {
        parse_head(
            &self
                .execute_jj_log_one(&bookmark.to_string(), HEAD_TEMPLATE_NL)
                .context("Failed getting bookmark head")?
                .remove_end_line(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use insta::assert_debug_snapshot;

    use super::*;
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
    fn parse_head_reads_a_record_of_its_own() -> Result<()> {
        assert_eq!(
            parse_head(
                r#"{"change_id":"kxq","commit_id":"1f2e","divergent":false,"immutable":true}"#
            )?,
            head("kxq", "1f2e", false, true)
        );

        Ok(())
    }

    #[test]
    fn parse_head_reads_a_record_behind_the_graph() -> Result<()> {
        assert_eq!(
            parse_head(
                r#"│ ├─╮  {"change_id":"kxq","commit_id":"1f2e","divergent":true,"immutable":false}"#
            )?,
            head("kxq", "1f2e", true, false)
        );

        Ok(())
    }

    #[test]
    fn a_graph_line_without_a_record_has_no_head() {
        assert!(parse_head("│ ├─╯").is_err());
        assert!(parse_head("~  (elided revisions)").is_err());
        assert!(parse_head("").is_err());
    }

    #[test]
    fn a_record_missing_a_field_is_no_head() {
        assert!(parse_head(r#"{"change_id":"kxq","commit_id":"1f2e"}"#).is_err());
    }

    #[test]
    fn get_log() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let log = test_repo.commander.get_log(&None)?;

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(r"[k-z]{8} .*? [0-9a-fA-F]{8}", "[LINE]");
        let _bound = settings.bind_to_scope();

        assert_debug_snapshot!(log.graph);

        assert!(log.graph_heads.iter().all(|graph_head| {
            graph_head
                .as_ref()
                .is_none_or(|graph_head| log.heads.contains(graph_head))
        }));

        Ok(())
    }

    #[test]
    fn spawn_commit_show() -> Result<()> {
        let test_repo = TestRepo::new()?;

        fs::write(test_repo.directory.path().join("README"), b"AAA")?;

        let head = test_repo.commander.get_current_head()?;
        let output = test_repo
            .commander
            .spawn_commit_show(&head.commit_id, &DiffFormat::ColorWords, false)?
            .wait_with_output()?;
        let show = String::from_utf8(output.stdout)?.remove_end_line();

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(r"Commit ID: [0-9a-fA-F]{40}", "Commit ID: [COMMIT_ID]");
        settings.add_filter(r"Change ID: [k-z]{32}", "Change ID: [Change ID]");
        settings.add_filter(r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})", "([DATE_TIME])");
        let _bound = settings.bind_to_scope();

        assert_debug_snapshot!(show);

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
    fn get_head_latest() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let old_head = test_repo.commander.get_current_head()?;

        fs::write(test_repo.directory.path().join("README"), b"AAA")?;

        let new_head = test_repo.commander.get_current_head()?;

        assert_ne!(old_head, new_head);

        assert_eq!(new_head, test_repo.commander.get_head_latest(&old_head)?);

        Ok(())
    }

    #[test]
    fn check_revision_immutable() -> Result<()> {
        let test_repo = TestRepo::new()?;

        assert!(!(test_repo.commander.check_revision_immutable("@")?));

        Ok(())
    }

    #[test]
    fn get_bookmark_head() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let head = test_repo.commander.get_current_head()?;
        // Git doesn't support bookmark pointing to root commit, so it will advance
        let bookmark = test_repo.commander.create_bookmark("main")?;

        assert_eq!(test_repo.commander.get_bookmark_head(&bookmark)?, head);

        Ok(())
    }
}
