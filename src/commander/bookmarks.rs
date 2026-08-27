/*!
[Commander] member functions related to jj bookmark.

This module has features to parse the `jj bookmark list` output. The
other jj bookmark commands are defined in module [jj][super::jj].

It is mostly used in the [bookmarks_tab][crate::ui::bookmarks_tab] module.
*/
use std::fmt::Display;

use ansi_to_tui::IntoText;
use anyhow::Result;
use itertools::Itertools;
use ratatui::text::Text;
use serde::Deserialize;
use tracing::instrument;

use crate::commander::CommandError;
use crate::commander::Commander;
use crate::commander::ids::ChangeId;
use crate::commander::log::Head;
use crate::commander::log::head_template;

/// A bookmark as [bookmark_template] describes it. The field names are the
/// ones the template writes.
///
/// `name` and `remote` are revset symbols, quoted where a plain name would
/// not do, as every jj command taking one of them wants it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Bookmark {
    pub name: String,
    pub remote: Option<String>,
    pub present: bool,
    pub timestamp: i64,
}

impl Display for Bookmark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut text = self.name.clone();
        if let Some(remote) = self.remote.as_ref() {
            text.push('@');
            text.push_str(remote);
        }
        write!(f, "{text}")
    }
}

/// A bookmark and the change it points at, as [bookmark_template]
/// describes them. The field names are the ones the template writes.
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct BookmarkRecord {
    #[serde(flatten)]
    bookmark: Bookmark,
    head: Head,
}

/// Template writing a [BookmarkRecord] as a JSON object, one per line.
///
/// `name` and `remote` are revset symbols, which jj only quotes when it
/// renders them as a template -- `stringify()` alone would hand back the
/// bare name. Concatenating is what puts them through that rendering.
///
/// A bookmark that has no single target has neither timestamp nor head,
/// and jj writes its error where each of them belongs. That leaves the
/// line unparsable, which is how such a bookmark is meant to come out.
fn bookmark_template() -> String {
    let head = head_template("self.normal_target()");
    format!(
        r#"
    '{{"name":' ++ stringify(concat(name)).escape_json()
    ++ ',"remote":' ++ if(remote, stringify(concat(remote)).escape_json(), 'null')
    ++ ',"present":' ++ present
    ++ ',"timestamp":' ++ self.normal_target().committer().timestamp().format("%s")
    ++ ',"head":' ++ {head}
    ++ '}}' ++ "\n"
"#
    )
}

/// Parse the [BookmarkRecord] one line of [bookmark_template] output
/// describes.
fn parse_bookmark(text: &str) -> Option<BookmarkRecord> {
    serde_json::from_str(text).ok()
}

#[derive(Clone, Debug)]
pub enum BookmarkLine {
    Unparsable(String),
    Parsed {
        text: String,
        bookmark: Bookmark,
        /// The change the bookmark points at
        head: Head,
    },
}

impl BookmarkLine {
    pub fn to_text(&self) -> Result<Text<'_>, ansi_to_tui::Error> {
        match self {
            BookmarkLine::Unparsable(text) => text.to_text(),
            BookmarkLine::Parsed { text, .. } => text.to_text(),
        }
    }
}

impl Commander {
    /// Get bookmarks.
    /// Leaves the working copy alone.
    /// Maps to `jj bookmark list --ignore-working-copy`
    #[instrument(level = "trace", skip(self))]
    pub fn get_bookmarks(&self, show_all: bool) -> Result<Vec<BookmarkLine>, CommandError> {
        let mut args = vec![];
        if show_all {
            args.push("--all-remotes");
        }
        args.push("--sort");
        args.push("committer-date-");
        let bookmarks_colored = self
            .jj([
                vec![
                    "bookmark",
                    "list",
                    "--config",
                    // Override format_ref_targets to not list conflicts
                    r#"template-aliases.'format_ref_targets(ref)'='''
                        if(ref.conflict(),
                          " " ++ label("conflict", "(conflicted)"),
                          ": " ++ format_commit_summary_with_refs(ref.normal_target(), ""),
                        )
                    '''"#,
                ],
                args.clone(),
            ]
            .concat())
            .color()
            .ignore_working_copy()
            .run()?;

        let bookmarks: Vec<BookmarkLine> = self
            .jj([vec!["bookmark", "list", "-T", &bookmark_template()], args].concat())
            .ignore_working_copy()
            .run()?
            .lines()
            .zip(bookmarks_colored.lines())
            .map(|(line, line_colored)| match parse_bookmark(line) {
                Some(record) => BookmarkLine::Parsed {
                    text: line_colored.to_owned(),
                    bookmark: record.bookmark,
                    head: record.head,
                },
                None => BookmarkLine::Unparsable(line_colored.to_owned()),
            })
            .collect();

        Ok(bookmarks)
    }

    /// Get the bookmarks that exist, newest first, leaving the working
    /// copy alone.
    #[instrument(level = "trace", skip(self))]
    pub fn get_bookmarks_list(&self, show_all: bool) -> Result<Vec<Bookmark>, CommandError> {
        let mut args = vec![
            "bookmark".to_owned(),
            "list".to_owned(),
            "-T".to_owned(),
            format!(r#"if(present, {}, "")"#, bookmark_template()),
        ];
        if show_all {
            args.push("--all-remotes".to_owned());
        }

        let bookmarks: Vec<Bookmark> = self
            .jj(args)
            .ignore_working_copy()
            .run()?
            .lines()
            .filter_map(parse_bookmark)
            .map(|record| record.bookmark)
            .sorted_by(|a, b| b.timestamp.cmp(&a.timestamp))
            .collect();

        Ok(bookmarks)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn generate_bookmark_name(&self, change_id: &ChangeId) -> Result<String, CommandError> {
        self.jj([
            "show",
            "--no-patch",
            "--template",
            self.env.jj_config.bookmark_template().as_str(),
            "-r",
            change_id.as_str(),
        ])
        .verbose()
        .run()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::commander::ids::CommitId;
    use crate::commander::tests::TestRepo;

    /// One line of [bookmark_template] output, describing the bookmark
    /// `fields` names and the change [head] describes.
    fn bookmark_line(fields: &str) -> String {
        format!(
            r#"{{{fields},"head":{{"change_id":"kkmpqwpv","commit_id":"c13337796487","divergent":false,"immutable":true}}}}"#
        )
    }

    fn head() -> Head {
        Head {
            change_id: ChangeId("kkmpqwpv".to_owned()),
            commit_id: CommitId("c13337796487".to_owned()),
            divergent: false,
            immutable: true,
        }
    }

    fn parse_bookmark_of(text: &str) -> Option<Bookmark> {
        parse_bookmark(text).map(|record| record.bookmark)
    }

    #[test]
    fn parse_bookmark_reads_a_local_bookmark() {
        assert_eq!(
            parse_bookmark_of(&bookmark_line(
                r#""name":"main","remote":null,"present":true,"timestamp":1786973730"#
            )),
            Some(Bookmark {
                name: "main".to_owned(),
                remote: None,
                present: true,
                timestamp: 1786973730,
            })
        );
    }

    #[test]
    fn parse_bookmark_reads_a_remote_bookmark() {
        assert_eq!(
            parse_bookmark_of(&bookmark_line(
                r#""name":"main","remote":"origin","present":false,"timestamp":1786973730"#
            )),
            Some(Bookmark {
                name: "main".to_owned(),
                remote: Some("origin".to_owned()),
                present: false,
                timestamp: 1786973730,
            })
        );
    }

    /// A name jj had to quote keeps its quotes, and an `@` in it stays on
    /// the name side rather than being taken for the remote separator.
    #[test]
    fn parse_bookmark_reads_a_name_that_needed_quoting() {
        assert_eq!(
            parse_bookmark_of(&bookmark_line(
                r#""name":"\"feature@v2\"","remote":"origin","present":true,"timestamp":1"#
            )),
            Some(Bookmark {
                name: "\"feature@v2\"".to_owned(),
                remote: Some("origin".to_owned()),
                present: true,
                timestamp: 1,
            })
        );
    }

    #[test]
    fn parse_bookmark_reads_the_change_the_bookmark_points_at() {
        assert_eq!(
            parse_bookmark(&bookmark_line(
                r#""name":"main","remote":null,"present":true,"timestamp":1"#
            ))
            .map(|record| record.head),
            Some(head())
        );
    }

    /// Which is what jj leaves behind for a bookmark without a single
    /// target, in place of the timestamp and of the head.
    #[test]
    fn parse_bookmark_rejects_a_record_holding_an_error() {
        assert!(
            parse_bookmark(
                r#"{"name":"main","remote":null,"present":true,"timestamp":<Error: No commit available>,"head":<Error: No commit available>}"#
            )
            .is_none()
        );
    }

    #[test]
    fn get_bookmarks() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let bookmark = test_repo.commander.create_bookmark("test")?;
        let bookmarks = test_repo.commander.get_bookmarks(false)?;

        assert_eq!(bookmarks.len(), 1);
        assert_eq!(
            bookmarks.first().and_then(|bookmark| match bookmark {
                BookmarkLine::Parsed { bookmark, .. } => Some(Bookmark {
                    name: bookmark.name.clone(),
                    remote: bookmark.remote.clone(),
                    present: bookmark.present,
                    timestamp: 0,
                }),
                _ => None,
            }),
            Some(Bookmark {
                name: bookmark.name.clone(),
                remote: bookmark.remote.clone(),
                present: bookmark.present,
                timestamp: 0,
            })
        );

        Ok(())
    }

    #[test]
    fn get_bookmarks_reads_the_change_each_bookmark_points_at() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let bookmark = test_repo.commander.create_bookmark("test")?;
        let bookmarks = test_repo.commander.get_bookmarks(false)?;

        assert_eq!(
            bookmarks.first().and_then(|line| match line {
                BookmarkLine::Parsed { head, .. } => Some(head.clone()),
                BookmarkLine::Unparsable(_) => None,
            }),
            Some(test_repo.commander.get_bookmark_head(&bookmark)?)
        );

        Ok(())
    }

    #[test]
    fn get_bookmarks_list() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let bookmark = test_repo.commander.create_bookmark("test")?;
        let bookmarks = test_repo.commander.get_bookmarks_list(false)?;

        assert_eq!(
            bookmarks
                .iter()
                .map(|b| Bookmark {
                    name: b.name.clone(),
                    remote: b.remote.clone(),
                    present: b.present,
                    timestamp: 0,
                })
                .collect::<Vec<_>>(),
            [Bookmark {
                name: bookmark.name,
                remote: bookmark.remote,
                present: bookmark.present,
                timestamp: 0,
            }]
        );

        Ok(())
    }
}
