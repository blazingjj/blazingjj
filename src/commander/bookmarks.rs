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
use crate::commander::RemoveEndLine;
use crate::commander::ids::ChangeId;
use crate::env::DiffFormat;

/// A bookmark as [BRANCH_TEMPLATE] describes it. The field names are the
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

/// Template writing a [Bookmark] as a JSON object, one per line.
///
/// `name` and `remote` are revset symbols, which jj only quotes when it
/// renders them as a template -- `stringify()` alone would hand back the
/// bare name. Concatenating is what puts them through that rendering.
///
/// A bookmark that has no single target has no timestamp either, and jj
/// writes its error where the number belongs. That leaves the line
/// unparsable, which is how such a bookmark is meant to come out.
const BRANCH_TEMPLATE: &str = r#"
    '{"name":' ++ stringify(concat(name)).escape_json()
    ++ ',"remote":' ++ if(remote, stringify(concat(remote)).escape_json(), 'null')
    ++ ',"present":' ++ present
    ++ ',"timestamp":' ++ self.normal_target().committer().timestamp().format("%s")
    ++ '}' ++ "\n"
"#;

/// Parse the [Bookmark] one line of [BRANCH_TEMPLATE] output describes.
fn parse_bookmark(text: &str) -> Option<Bookmark> {
    serde_json::from_str(text).ok()
}

#[derive(Clone, Debug)]
pub enum BookmarkLine {
    Unparsable(String),
    Parsed { text: String, bookmark: Bookmark },
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
    /// Maps to `jj bookmark list`
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
            .run()?;

        let bookmarks: Vec<BookmarkLine> = self
            .jj([vec!["bookmark", "list", "-T", BRANCH_TEMPLATE], args].concat())
            .run()?
            .lines()
            .zip(bookmarks_colored.lines())
            .map(|(line, line_colored)| match parse_bookmark(line) {
                Some(bookmark) => BookmarkLine::Parsed {
                    text: line_colored.to_owned(),
                    bookmark,
                },
                None => BookmarkLine::Unparsable(line_colored.to_owned()),
            })
            .collect();

        Ok(bookmarks)
    }

    #[instrument(level = "trace", skip(self))]
    pub fn get_bookmarks_list(&self, show_all: bool) -> Result<Vec<Bookmark>, CommandError> {
        let mut args = vec![
            "bookmark".to_owned(),
            "list".to_owned(),
            "-T".to_owned(),
            format!(r#"if(present, {BRANCH_TEMPLATE}, "")"#),
        ];
        if show_all {
            args.push("--all-remotes".to_owned());
        }

        let bookmarks: Vec<Bookmark> = self
            .jj(args)
            .run()?
            .lines()
            .filter_map(parse_bookmark)
            .sorted_by(|a, b| b.timestamp.cmp(&a.timestamp))
            .collect();

        Ok(bookmarks)
    }

    /// Get bookmark details.
    /// Maps to `jj show <bookmark>`
    #[instrument(level = "trace", skip(self))]
    pub fn get_bookmark_show(
        &self,
        bookmark: &Bookmark,
        diff_format: &DiffFormat,
        ignore_working_copy: bool,
    ) -> Result<String, CommandError> {
        let bookmark_arg = &bookmark.to_string();
        let mut args = vec!["show", bookmark_arg];
        args.append(&mut diff_format.get_args());

        let mut command = self.jj(args).color();
        if ignore_working_copy {
            command = command.ignore_working_copy();
        }

        Ok(command.run()?.remove_end_line())
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

    use insta::assert_debug_snapshot;

    use super::*;
    use crate::commander::tests::TestRepo;

    #[test]
    fn parse_bookmark_reads_a_local_bookmark() {
        assert_eq!(
            parse_bookmark(
                r#"{"name":"main","remote":null,"present":true,"timestamp":1786973730}"#
            ),
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
            parse_bookmark(
                r#"{"name":"main","remote":"origin","present":false,"timestamp":1786973730}"#
            ),
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
            parse_bookmark(
                r#"{"name":"\"feature@v2\"","remote":"origin","present":true,"timestamp":1}"#
            ),
            Some(Bookmark {
                name: "\"feature@v2\"".to_owned(),
                remote: Some("origin".to_owned()),
                present: true,
                timestamp: 1,
            })
        );
    }

    /// Which is what jj leaves behind for a bookmark without a single
    /// target, in place of the timestamp.
    #[test]
    fn parse_bookmark_rejects_a_record_holding_an_error() {
        assert!(
            parse_bookmark(
                r#"{"name":"main","remote":null,"present":true,"timestamp":<Error: No commit available>}"#
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

    #[test]
    fn get_bookmark_show() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let bookmark = test_repo.commander.create_bookmark("test")?;
        let bookmark_show =
            test_repo
                .commander
                .get_bookmark_show(&bookmark, &DiffFormat::default(), false)?;

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(r"Commit ID: [0-9a-fA-F]{40}", "Commit ID: [COMMIT_ID]");
        settings.add_filter(r"Change ID: [k-z]{32}", "Change ID: [Change ID]");
        settings.add_filter(r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})", "([DATE_TIME])");
        let _bound = settings.bind_to_scope();

        assert_debug_snapshot!(bookmark_show);

        Ok(())
    }
}
