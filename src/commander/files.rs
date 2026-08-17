/*!
[Commander] member functions related to jj diff.

This module has features to parse the diff output.
It is mostly used in the [files_tab][crate::ui::files_tab] module.
*/
use std::sync::LazyLock;

use anyhow::Context;
use anyhow::Result;
use ratatui::style::Color;
use regex::Regex;
use serde::Deserialize;
use tracing::instrument;

use crate::commander::CommandError;
use crate::commander::Commander;
use crate::commander::ids::CommitId;
use crate::commander::log::Head;
use crate::env::DiffFormat;

#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub line: String,
    pub path: Option<String>,
    pub diff_type: Option<DiffType>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DiffType {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    pub path: String,
}

impl DiffType {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "A" => Some(DiffType::Added),
            "M" => Some(DiffType::Modified),
            "D" => Some(DiffType::Deleted),
            "R" => Some(DiffType::Renamed),
            "C" => Some(DiffType::Copied),
            _ => None,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            DiffType::Added => Color::Green,
            DiffType::Modified => Color::Cyan,
            DiffType::Renamed => Color::Cyan,
            DiffType::Copied => Color::Cyan,
            DiffType::Deleted => Color::Red,
        }
    }
}

static CONFLICTS_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(.*)    .*").unwrap());

/// Template writing a changed file as a JSON object, one per line.
///
/// `map()` joins what it renders with a space, so the newline that makes
/// this JSONL has to go inside the closure.
///
/// Paths are formatted for display, which is what makes them relative to
/// the working directory, the way `file:` filesets read them back.
const FILES_TEMPLATE: &str = r#"
    self.diff().files().map(|entry|
        '{"status":' ++ entry.status_char().escape_json()
        ++ ',"display_path":' ++ entry.display_diff_path().escape_json()
        ++ ',"path":' ++ entry.path().display().escape_json()
        ++ '}' ++ "\n"
    ).join("")
"#;

/// A changed file as [FILES_TEMPLATE] describes it. The field names are the
/// ones the template writes.
#[derive(Deserialize)]
struct FileRecord {
    /// Single-character status: one of `AMDRC`
    status: String,
    /// Path as the files list shows it, with a rename spelled out as
    /// `dir/{old => new}`
    display_path: String,
    /// Path to act on, which for a rename or a copy is the new one
    path: String,
}

/// Parse the [File] one line of [FILES_TEMPLATE] output describes.
///
/// jj writes an error into the object it cannot render, as it does for a
/// path that is not valid UTF-8. There is nothing to act on then, so the
/// line is left to be shown as it came.
fn parse_file(line: &str) -> File {
    match serde_json::from_str::<FileRecord>(line) {
        Ok(record) => File {
            line: format!("{} {}", record.status, record.display_path),
            diff_type: DiffType::parse(&record.status),
            path: Some(record.path),
        },
        Err(_) => File {
            line: line.to_owned(),
            diff_type: None,
            path: None,
        },
    }
}

impl Commander {
    /// Get list of changes files in a change. Parses the output.
    /// Maps to `jj log -r <revision>` with [FILES_TEMPLATE]
    #[instrument(level = "trace", skip(self))]
    pub fn get_files(&self, head: &Head) -> Result<Vec<File>, CommandError> {
        Ok(self
            .jj([
                "log",
                "--no-graph",
                "-T",
                FILES_TEMPLATE,
                "-r",
                head.commit_id.as_str(),
            ])
            .run()?
            .lines()
            .map(parse_file)
            .collect())
    }

    /// Get list of changes files in a change. Parses the output.
    /// Maps to `jj diff --summary -r <revision>`
    #[instrument(level = "trace", skip(self))]
    pub fn get_conflicts(&self, commit_id: &CommitId) -> Result<Vec<Conflict>> {
        let output = self
            .jj(["resolve", "--list", "-r", commit_id.as_str()])
            .run();

        match output {
            Ok(output) => Ok(output
                .lines()
                .filter_map(|line| {
                    let captured = CONFLICTS_REGEX.captures(line);
                    captured
                        .as_ref()
                        .and_then(|captured| captured.get(1))
                        .map(|inner_text| Conflict {
                            path: inner_text.as_str().to_owned(),
                        })
                })
                .collect()),
            Err(CommandError::Status(_, Some(2))) => {
                // No conflicts
                Ok(vec![])
            }
            Err(err) => Err(err).context("Failed getting conflicts"),
        }
    }

    /// Get diff for file change in a change.
    /// Maps to `jj diff -r <revision> <path>`
    #[instrument(level = "trace", skip(self))]
    pub fn get_file_diff(
        &self,
        head: &Head,
        current_file: &File,
        diff_format: &DiffFormat,
        ignore_working_copy: bool,
    ) -> Result<Option<String>, CommandError> {
        let Some(path) = current_file.path.as_deref() else {
            return Ok(None);
        };

        let fileset = Self::get_file_revset(path);
        let mut args = vec!["diff", "-r", head.commit_id.as_str(), &fileset];
        args.append(&mut diff_format.get_args());

        let mut command = self.jj(args).color();
        if ignore_working_copy {
            command = command.ignore_working_copy();
        }

        command.run().map(Some)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn untrack_file(&self, current_file: &File) -> Result<Option<String>, CommandError> {
        let Some(path) = current_file.path.as_deref() else {
            return Ok(None);
        };

        let fileset = Self::get_file_revset(path);
        Ok(Some(self.jj(["file", "untrack", &fileset]).run()?))
    }

    #[instrument(level = "trace", skip(self))]
    pub fn restore_file(&self, current_file: &File) -> Result<Option<String>, CommandError> {
        let Some(path) = current_file.path.as_deref() else {
            return Ok(None);
        };

        let fileset = Self::get_file_revset(path);
        Ok(Some(self.jj(["restore", &fileset]).run()?))
    }

    fn get_file_revset(path: &str) -> String {
        format!(
            "file:\"{}\"",
            path.replace("\\", "\\\\").replace('"', "\\\"")
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use insta::assert_debug_snapshot;

    use super::*;
    use crate::commander::tests::TestRepo;

    #[test]
    fn every_status_jj_lists_has_a_diff_type() {
        assert_eq!(DiffType::parse("A"), Some(DiffType::Added));
        assert_eq!(DiffType::parse("M"), Some(DiffType::Modified));
        assert_eq!(DiffType::parse("D"), Some(DiffType::Deleted));
        assert_eq!(DiffType::parse("R"), Some(DiffType::Renamed));
        assert_eq!(DiffType::parse("C"), Some(DiffType::Copied));
        assert_eq!(DiffType::parse("?"), None);
    }

    #[test]
    fn parse_file_reads_a_changed_file() {
        assert_eq!(
            parse_file(r#"{"status":"M","display_path":"src/main.rs","path":"src/main.rs"}"#),
            File {
                line: "M src/main.rs".to_owned(),
                path: Some("src/main.rs".to_owned()),
                diff_type: Some(DiffType::Modified),
            }
        );
    }

    /// The line spells the rename out the way jj does, while the path is the
    /// new name on its own, ready to act on.
    #[test]
    fn parse_file_reads_a_rename() {
        assert_eq!(
            parse_file(
                r#"{"status":"R","display_path":"dir/{a.txt => b.txt}","path":"dir/b.txt"}"#
            ),
            File {
                line: "R dir/{a.txt => b.txt}".to_owned(),
                path: Some("dir/b.txt".to_owned()),
                diff_type: Some(DiffType::Renamed),
            }
        );
    }

    /// A path may hold anything a JSON string can, delimiters included.
    #[test]
    fn parse_file_reads_a_path_that_needed_escaping() {
        assert_eq!(
            parse_file(r#"{"status":"A","display_path":"a\"b\n{c}","path":"a\"b\n{c}"}"#),
            File {
                line: "A a\"b\n{c}".to_owned(),
                path: Some("a\"b\n{c}".to_owned()),
                diff_type: Some(DiffType::Added),
            }
        );
    }

    #[test]
    fn parse_file_leaves_a_record_holding_an_error_to_be_shown_as_it_is() {
        let line = r#"{"status":"A","display_path":<Error: Invalid UTF-8>,"path":""}"#;

        assert_eq!(
            parse_file(line),
            File {
                line: line.to_owned(),
                path: None,
                diff_type: None,
            }
        );
    }

    #[test]
    fn get_files() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let file_path = test_repo.directory.path().join("README");

        // Initial state
        {
            let head = test_repo.commander.get_current_head()?;
            let files = test_repo.commander.get_files(&head)?;
            assert_eq!(files, vec![]);
        }

        // Add file
        {
            fs::write(&file_path, b"AAA")?;

            let head = test_repo.commander.get_current_head()?;
            let files = test_repo.commander.get_files(&head)?;
            assert_eq!(
                files,
                vec![File {
                    line: "A README".to_owned(),
                    path: Some("README".to_owned(),),
                    diff_type: Some(DiffType::Added,),
                },]
            );
        }

        // Commit
        test_repo.commander.jj(["new"]).run_void()?;

        // Modify file
        {
            fs::write(&file_path, b"BBB")?;

            let head = test_repo.commander.get_current_head()?;
            let files = test_repo.commander.get_files(&head)?;
            assert_eq!(
                files,
                vec![File {
                    line: "M README".to_owned(),
                    path: Some("README".to_owned()),
                    diff_type: Some(DiffType::Modified)
                },]
            );
        }

        // Commit
        test_repo.commander.jj(["new"]).run_void()?;

        // Rename file into a directory
        let directory = test_repo.directory.path().join("dir");
        {
            fs::create_dir(&directory)?;
            fs::rename(&file_path, directory.join("README2"))?;

            // jj renders paths for display, so the separator is the platform's.
            let renamed = Path::new("dir").join("README2").display().to_string();

            let head = test_repo.commander.get_current_head()?;
            let files = test_repo.commander.get_files(&head)?;
            assert_eq!(
                files,
                vec![File {
                    line: format!("R {{README => {renamed}}}"),
                    path: Some(renamed),
                    diff_type: Some(DiffType::Renamed)
                },]
            );
        }

        // Delete file, which is the rename with nothing left of it
        {
            fs::remove_dir_all(&directory)?;

            let head = test_repo.commander.get_current_head()?;
            let files = test_repo.commander.get_files(&head)?;
            assert_eq!(
                files,
                vec![File {
                    line: "D README".to_owned(),
                    path: Some("README".to_owned()),
                    diff_type: Some(DiffType::Deleted)
                },]
            );
        }

        Ok(())
    }

    #[test]
    fn get_file_diff() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let mut file_path = test_repo.directory.path().join("README");

        // Add file
        {
            fs::write(&file_path, b"AAA")?;
            let file = File {
                path: Some("README".to_string()),
                diff_type: Some(DiffType::Added),
                line: "A README".to_string(),
            };

            let head = test_repo.commander.get_current_head()?;
            assert_debug_snapshot!(test_repo.commander.get_file_diff(
                &head,
                &file,
                &DiffFormat::ColorWords,
                false
            )?);
            assert_debug_snapshot!(test_repo.commander.get_file_diff(
                &head,
                &file,
                &DiffFormat::Git,
                false
            )?);
        }

        // Commit
        test_repo.commander.jj(["new"]).run_void()?;

        // Modify file
        {
            fs::write(&file_path, b"BBB")?;
            let file = File {
                path: Some("README".to_string()),
                diff_type: Some(DiffType::Modified),
                line: "M README".to_string(),
            };

            let head = test_repo.commander.get_current_head()?;
            assert_debug_snapshot!(test_repo.commander.get_file_diff(
                &head,
                &file,
                &DiffFormat::ColorWords,
                true
            )?);
            assert_debug_snapshot!(test_repo.commander.get_file_diff(
                &head,
                &file,
                &DiffFormat::Git,
                true
            )?);
        }

        // Commit
        test_repo.commander.jj(["new"]).run_void()?;

        // Rename file
        {
            let file_path_new = test_repo.directory.path().join("README2");
            fs::rename(file_path, &file_path_new)?;
            file_path = file_path_new;

            // The path is the one get_files() resolved, while the line keeps
            // both names for display.
            let file = File {
                path: Some("README2".to_string()),
                diff_type: Some(DiffType::Renamed),
                line: "R {README => README2}".to_string(),
            };

            let head = test_repo.commander.get_current_head()?;
            assert_debug_snapshot!(test_repo.commander.get_file_diff(
                &head,
                &file,
                &DiffFormat::ColorWords,
                true
            )?);
            assert_debug_snapshot!(test_repo.commander.get_file_diff(
                &head,
                &file,
                &DiffFormat::Git,
                true
            )?);
        }

        // Commit
        test_repo.commander.jj(["new"]).run_void()?;

        // Delete file
        {
            fs::remove_file(&file_path)?;
            let file = File {
                path: Some("README2".to_string()),
                diff_type: Some(DiffType::Deleted),
                line: "D README2".to_string(),
            };

            let head = test_repo.commander.get_current_head()?;
            assert_debug_snapshot!(test_repo.commander.get_file_diff(
                &head,
                &file,
                &DiffFormat::ColorWords,
                true
            )?);
            assert_debug_snapshot!(test_repo.commander.get_file_diff(
                &head,
                &file,
                &DiffFormat::Git,
                true
            )?);
        }

        Ok(())
    }

    #[test]
    fn get_conflicts() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let file_path = test_repo.directory.path().join("README");

        let head0 = test_repo.commander.get_current_head()?;

        // First change
        test_repo.commander.run_new([head0.commit_id.as_str()])?;
        let head1 = test_repo.commander.get_current_head()?;
        fs::write(&file_path, b"AAA")?;

        test_repo.commander.run_new([head0.commit_id.as_str()])?;
        let head2 = test_repo.commander.get_current_head()?;
        fs::write(&file_path, b"BBB")?;

        test_repo
            .commander
            .jj([
                "rebase",
                "-s",
                head2.change_id.as_str(),
                "-d",
                head1.change_id.as_str(),
            ])
            .run_void()?;

        let head = test_repo.commander.get_current_head()?;

        let conflicts = test_repo.commander.get_conflicts(&head.commit_id)?;

        assert_eq!(
            conflicts,
            [Conflict {
                path: "README".to_owned()
            }]
        );

        Ok(())
    }
}
