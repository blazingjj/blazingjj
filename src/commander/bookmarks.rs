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
/// A bookmark with more than one target is written the way jj lists it:
/// a line naming the bookmark, which points at no change of its own and
/// so holds no head, and one line per target under it.
fn bookmark_template() -> String {
    let record =
        |head: &str| format!(r#"'{{' ++ {BOOKMARK_FIELDS} ++ ',"head":' ++ {head} ++ '}}'"#);
    let single = record(&head_template("self.normal_target()"));
    let target = record(&head_template("target"));

    format!(
        r#"
if(self.conflict(),
  '{{' ++ {BOOKMARK_FIELDS} ++ '}}' ++ "\n"
  ++ self.removed_targets().map(|target| {target} ++ "\n").join("")
  ++ self.added_targets().map(|target| {target} ++ "\n").join(""),
  {single} ++ "\n",
)
"#
    )
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
            .jj([vec!["bookmark", "list"], args.clone()].concat())
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

    /// A repo whose only bookmark is torn between three targets: two
    /// operations from the same point, each moving it somewhere else,
    /// leave the change it was on as the one given up and both of the
    /// changes it was moved to as ones taken, which is jj's `-` and `+`.
    fn conflicted_bookmark() -> Result<TestRepo> {
        let test_repo = TestRepo::new()?;
        let commander = &test_repo.commander;

        let change = |message: &str| -> Result<CommitId> {
            commander.jj(["new", "root()", "-m", message]).run_void()?;

            Ok(commander.get_current_head()?.commit_id)
        };
        let base = change("base")?;
        let one = change("one")?;
        let two = change("two")?;

        let set_to = |commit_id: &CommitId| {
            vec![
                "bookmark".to_owned(),
                "set".to_owned(),
                "bm".to_owned(),
                "-r".to_owned(),
                commit_id.as_str().to_owned(),
                "--allow-backwards".to_owned(),
            ]
        };
        commander
            .jj(["bookmark", "create", "bm", "-r", base.as_str()])
            .run_void()?;
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
        commander.jj(set_to(&one)).run_void()?;
        commander
            .jj([
                vec!["--at-op".to_owned(), at_op.trim().to_owned()],
                set_to(&two),
            ]
            .concat())
            .run_void()?;

        Ok(test_repo)
    }

    /// A bookmark with more than one target has no single commit to
    /// point at. Leaving it out would hide it from the one listing that
    /// offers to set it, which is how such a bookmark is resolved.
    #[test]
    fn get_bookmarks_list_offers_a_conflicted_bookmark() -> Result<()> {
        let test_repo = conflicted_bookmark()?;
        let commander = &test_repo.commander;
        let on = commander.get_current_head()?.commit_id;

        let names: Vec<String> = commander
            .get_bookmarks_list(false, &on)?
            .into_iter()
            .filter(|bookmark| bookmark.remote.is_none())
            .map(|bookmark| bookmark.name)
            .collect();

        assert_eq!(names, ["bm"]);

        Ok(())
    }

    /// jj lists such a bookmark as a line naming it and one per target,
    /// and the tab shows what jj prints, so the two have to agree on how
    /// many lines that is.
    #[test]
    fn get_bookmarks_lists_a_conflicted_bookmark_once_per_target() -> Result<()> {
        let test_repo = conflicted_bookmark()?;
        let commander = &test_repo.commander;

        let listed = commander.get_bookmarks(false)?;
        let targets: Vec<CommitId> = listed
            .iter()
            .filter_map(|line| match line {
                BookmarkLine::Parsed { bookmark, head, .. } if bookmark.remote.is_none() => {
                    Some(head.commit_id.clone())
                }
                _ => None,
            })
            .collect();
        let naming = listed
            .iter()
            .filter(|line| matches!(line, BookmarkLine::Unparsable(_)))
            .count();

        // The line naming the bookmark points at nothing, and each of the
        // three below it holds the target it stands for.
        assert_eq!(naming, 1, "{listed:?}");
        assert_eq!(targets.iter().unique().count(), 3, "{listed:?}");

        Ok(())
    }

    /// The two listings are paired line by line, so a target has to end
    /// up beside the line jj wrote about that same target.
    #[test]
    fn get_bookmarks_pairs_each_target_with_what_jj_says_about_it() -> Result<()> {
        let test_repo = conflicted_bookmark()?;

        let mismatched: Vec<BookmarkLine> = test_repo
            .commander
            .get_bookmarks(false)?
            .into_iter()
            .filter(|line| match line {
                BookmarkLine::Parsed { text, head, .. } => {
                    !text.contains(&head.commit_id.as_str()[..8])
                }
                BookmarkLine::Unparsable(_) => false,
            })
            .collect();

        assert!(mismatched.is_empty(), "{mismatched:?}");

        Ok(())
    }
}
