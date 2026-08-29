/*!
[Commander] member functions related to jj bookmark.

This module has features to parse the `jj bookmark list` output. The
other jj bookmark commands are defined in module [jj][super::jj].

It is mostly used in the [bookmarks_tab][crate::ui::bookmarks_tab] module.
*/
use std::collections::HashMap;
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
use crate::commander::ids::CommitId;
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

/// The [Bookmark] fields, as the members of a JSON object without the
/// braces around them.
///
/// `name` and `remote` are revset symbols, which jj only quotes when it
/// renders them as a template -- `stringify()` alone would hand back the
/// bare name. Concatenating is what puts them through that rendering.
const BOOKMARK_FIELDS: &str = r#"
    '"name":' ++ stringify(concat(name)).escape_json()
    ++ ',"remote":' ++ if(remote, stringify(concat(remote)).escape_json(), 'null')
    ++ ',"present":' ++ present
"#;

/// Template writing a [Bookmark] as a JSON object, one per line. Says
/// nothing about what the bookmark points at, so a bookmark with no
/// single target is written like any other.
fn bookmark_only_template() -> String {
    format!(r#"'{{' ++ {BOOKMARK_FIELDS} ++ '}}' ++ "\n""#)
}

/// Template writing a [BookmarkRecord] as a JSON object, one per line.
///
/// A bookmark that has no single target has no head, and jj writes its
/// error where one belongs. That leaves the line unparsable, which is how
/// such a bookmark is meant to come out of a listing that shows what each
/// one points at.
fn bookmark_template() -> String {
    let head = head_template("self.normal_target()");
    format!(r#"'{{' ++ {BOOKMARK_FIELDS} ++ ',"head":' ++ {head} ++ '}}' ++ "\n""#)
}

/// Parse the [BookmarkRecord] one line of [bookmark_template] output
/// describes.
fn parse_bookmark(text: &str) -> Option<BookmarkRecord> {
    serde_json::from_str(text).ok()
}

/// Parse the [Bookmark] one line of [bookmark_only_template] output
/// describes.
fn parse_bookmark_only(text: &str) -> Option<Bookmark> {
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

    /// The bookmarks that exist, the ones `near` already stands on first
    /// and the nearest of those before the rest: a bookmark is usually
    /// set on a change it is behind. Leaves the working copy alone.
    #[instrument(level = "trace", skip(self))]
    pub fn get_bookmarks_list(
        &self,
        show_all: bool,
        near: &CommitId,
    ) -> Result<Vec<Bookmark>, CommandError> {
        let mut args = vec![
            "bookmark".to_owned(),
            "list".to_owned(),
            "-T".to_owned(),
            format!(r#"if(present, {}, "")"#, bookmark_only_template()),
        ];
        if show_all {
            args.push("--all-remotes".to_owned());
        }

        let ranks = self.get_bookmark_ranks(near)?;
        let bookmarks: Vec<Bookmark> = self
            .jj(args)
            .ignore_working_copy()
            .run()?
            .lines()
            .filter_map(parse_bookmark_only)
            .sorted_by_key(|bookmark| match bookmark.remote {
                // Only local bookmarks are ranked, so one on a remote
                // would otherwise take the rank of the local bookmark it
                // shares a name with.
                Some(_) => usize::MAX,
                None => ranks.get(&bookmark.name).copied().unwrap_or(usize::MAX),
            })
            .collect();

        Ok(bookmarks)
    }

    /// Where each local bookmark on an ancestor of `commit_id` comes in
    /// walking back from it, nearest first, by name. One with more than
    /// one target is ranked by whichever of them is reached first.
    ///
    /// Ranking a bookmark on a remote by the same name would say where
    /// the remote has it rather than where it is, so both the revset and
    /// the template leave those out.
    fn get_bookmark_ranks(
        &self,
        commit_id: &CommitId,
    ) -> Result<HashMap<String, usize>, CommandError> {
        let listed = self
            .jj([
                "log",
                "-r",
                &format!("bookmarks() & ::{}", commit_id.as_str()),
                "--no-graph",
                "-T",
                r#"local_bookmarks.map(|bookmark| bookmark.name()).join("\n") ++ "\n""#,
            ])
            .ignore_working_copy()
            .run()?;

        let mut ranks = HashMap::new();
        for (rank, name) in listed.lines().enumerate() {
            ranks.entry(name.to_owned()).or_insert(rank);
        }

        Ok(ranks)
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
                r#""name":"main","remote":null,"present":true"#
            )),
            Some(Bookmark {
                name: "main".to_owned(),
                remote: None,
                present: true,
            })
        );
    }

    #[test]
    fn parse_bookmark_reads_a_remote_bookmark() {
        assert_eq!(
            parse_bookmark_of(&bookmark_line(
                r#""name":"main","remote":"origin","present":false"#
            )),
            Some(Bookmark {
                name: "main".to_owned(),
                remote: Some("origin".to_owned()),
                present: false,
            })
        );
    }

    /// A name jj had to quote keeps its quotes, and an `@` in it stays on
    /// the name side rather than being taken for the remote separator.
    #[test]
    fn parse_bookmark_reads_a_name_that_needed_quoting() {
        assert_eq!(
            parse_bookmark_of(&bookmark_line(
                r#""name":"\"feature@v2\"","remote":"origin","present":true"#
            )),
            Some(Bookmark {
                name: "\"feature@v2\"".to_owned(),
                remote: Some("origin".to_owned()),
                present: true,
            })
        );
    }

    #[test]
    fn parse_bookmark_reads_the_change_the_bookmark_points_at() {
        assert_eq!(
            parse_bookmark(&bookmark_line(
                r#""name":"main","remote":null,"present":true"#
            ))
            .map(|record| record.head),
            Some(head())
        );
    }

    /// Which is what jj leaves behind for a bookmark without a single
    /// target, in place of the head.
    #[test]
    fn parse_bookmark_rejects_a_record_holding_an_error() {
        assert!(
            parse_bookmark(
                r#"{"name":"main","remote":null,"present":true,"head":<Error: No commit available>}"#
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
                }),
                _ => None,
            }),
            Some(Bookmark {
                name: bookmark.name.clone(),
                remote: bookmark.remote.clone(),
                present: bookmark.present,
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
        let bookmarks = test_repo.bookmarks()?;

        assert_eq!(
            bookmarks
                .iter()
                .map(|b| Bookmark {
                    name: b.name.clone(),
                    remote: b.remote.clone(),
                    present: b.present,
                })
                .collect::<Vec<_>>(),
            [Bookmark {
                name: bookmark.name,
                remote: bookmark.remote,
                present: bookmark.present,
            }]
        );

        Ok(())
    }

    /// The names the set-bookmark dialog would list, in the order it
    /// would offer them.
    fn listed_names(test_repo: &TestRepo) -> Result<Vec<String>> {
        Ok(test_repo
            .bookmarks()?
            .into_iter()
            .map(|bookmark| bookmark.name)
            .collect())
    }

    /// The one the change is standing on is the one it is most likely to
    /// be moved to.
    #[test]
    fn get_bookmarks_list_offers_the_nearest_bookmark_first() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let commander = &test_repo.commander;

        // near <- far, with the working copy on top of both.
        commander.create_bookmark("far")?;
        commander.run_new(&commander.get_current_head()?.commit_id)?;
        commander.create_bookmark("near")?;
        commander.run_new(&commander.get_current_head()?.commit_id)?;

        assert_eq!(listed_names(&test_repo)?, ["near", "far"]);

        Ok(())
    }

    /// One the change is not standing on is still worth offering, just
    /// not before the ones it is.
    #[test]
    fn get_bookmarks_list_offers_a_bookmark_off_to_the_side_last() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let commander = &test_repo.commander;

        commander.create_bookmark("behind")?;
        let behind = commander.get_current_head()?.commit_id;
        commander.jj(["new", "root()"]).run_void()?;
        commander.create_bookmark("aside")?;
        commander.run_new(&behind)?;

        assert_eq!(listed_names(&test_repo)?, ["behind", "aside"]);

        Ok(())
    }

    /// Two bookmarks on one change are one line of the ranking each, so
    /// neither swallows the other.
    #[test]
    fn get_bookmarks_list_offers_every_bookmark_on_the_same_change() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let commander = &test_repo.commander;

        // Both are ranked only if the change they share is read as two
        // names rather than as one run-together name, which would leave
        // them behind the bookmark off to the side.
        commander.jj(["new", "root()"]).run_void()?;
        commander.create_bookmark("aside")?;
        commander.jj(["new", "root()"]).run_void()?;
        commander.create_bookmark("both")?;
        commander.create_bookmark("here")?;
        commander.run_new(&commander.get_current_head()?.commit_id)?;

        let names = listed_names(&test_repo)?;

        assert_eq!(names.last(), Some(&"aside".to_owned()), "{names:?}");
        assert!(names.contains(&"both".to_owned()), "{names:?}");
        assert!(names.contains(&"here".to_owned()), "{names:?}");

        Ok(())
    }

    /// A bookmark with more than one target has no single commit to
    /// point at. Leaving it out would hide it from the one listing that
    /// offers to set it, which is how such a bookmark is resolved.
    #[test]
    fn get_bookmarks_list_offers_a_conflicted_bookmark() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let commander = &test_repo.commander;

        commander.jj(["new", "root()", "-m", "one"]).run_void()?;
        let one = commander.get_current_head()?.commit_id;
        commander.jj(["new", "root()", "-m", "two"]).run_void()?;
        let two = commander.get_current_head()?.commit_id;

        // Two operations from the same point, each putting the bookmark
        // somewhere else, is what leaves it with both targets.
        let at_op = commander
            .jj([
                "op",
                "log",
                "--no-graph",
                "--limit",
                "1",
                "-T",
                "id.short()",
            ])
            .run()?;
        commander
            .jj(["bookmark", "create", "bm", "-r", one.as_str()])
            .run_void()?;
        commander
            .jj([
                "--at-op",
                at_op.trim(),
                "bookmark",
                "create",
                "bm",
                "-r",
                two.as_str(),
            ])
            .run_void()?;

        let names: Vec<String> = commander
            .get_bookmarks_list(false, &two)?
            .into_iter()
            .filter(|bookmark| bookmark.remote.is_none())
            .map(|bookmark| bookmark.name)
            .collect();

        assert_eq!(names, ["bm"]);

        Ok(())
    }
}
