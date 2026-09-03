/*! What a tab has selected, and the placeholders a command names it by.

A command the user writes, whether typed into the command popup or
configured as one of their own, says what to run it against by putting a
placeholder where the argument goes. What each one stands for is
whatever the tab it is run from has selected, so `$selected` means the
change in the log and the file in the files tab.

A placeholder the current tab has nothing for is an error rather than an
empty argument: a command that was to be run against something is not
one to run against nothing instead.
*/

use std::fmt;

use crate::commander::ids::CommitId;
use crate::commander::ids::OperationId;
use crate::commander::log::Head;
use crate::commander::revset::Revset;

/// How the revision a tab is on is named to something outside the app:
/// by its change id, so that it is read as the change stands, except
/// where what is shown is not the change as it stands. A version out of
/// the evolog and one of several divergent commits are both only to be
/// found by their commit id.
pub fn shown_revision(head: &Head, pinned: bool) -> &str {
    if pinned || head.divergent {
        head.commit_id.as_str()
    } else {
        head.change_id.as_str()
    }
}

/// One of the placeholders a command is written with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placeholder {
    /// What the tab is about: the revision, or the file, bookmark or
    /// operation where the tab has one of those.
    Selected,
    Revision,
    Marked,
    File,
    Bookmark,
    Operation,
}

impl Placeholder {
    /// Every placeholder there is, in the order they are documented in.
    pub const ALL: [Self; 6] = [
        Self::Selected,
        Self::Marked,
        Self::Revision,
        Self::File,
        Self::Bookmark,
        Self::Operation,
    ];

    /// The names it is written as, of which the first is the one to
    /// read it back by.
    pub fn names(self) -> &'static [&'static str] {
        match self {
            Self::Selected => &["$selected", "$s"],
            Self::Revision => &["$revision"],
            Self::Marked => &["$marked", "$m"],
            Self::File => &["$file"],
            Self::Bookmark => &["$bookmark"],
            Self::Operation => &["$operation"],
        }
    }

    /// What it stands for, as the help and the settings tab say it.
    pub fn doc(self) -> &'static str {
        match self {
            Self::Selected => "what the tab has selected",
            Self::Revision => "the revision the tab is on",
            Self::Marked => "the changes the log has marked, as one revset",
            Self::File => "the selected file",
            Self::Bookmark => "the selected bookmark",
            Self::Operation => "the selected operation",
        }
    }
}

impl fmt::Display for Placeholder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.names()[0])
    }
}

/// What a tab has selected, of each of the kinds a placeholder names.
///
/// A tab is about one kind of thing but may know of others: the files
/// tab has a file and the revision it is shown at, so a command run
/// from it can name either.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    revision: Option<String>,
    marked: Vec<CommitId>,
    file: Option<String>,
    bookmark: Option<String>,
    operation: Option<String>,
}

impl Selection {
    /// The selection with the revision `head` is on, named as
    /// [shown_revision] names it.
    pub fn revision(mut self, head: &Head, pinned: bool) -> Self {
        self.revision = Some(shown_revision(head, pinned).to_owned());
        self
    }

    /// The selection with `marked` marked, which only the log has.
    pub fn marked(mut self, marked: &[CommitId]) -> Self {
        self.marked = marked.to_vec();
        self
    }

    pub fn file(mut self, path: &str) -> Self {
        self.file = Some(path.to_owned());
        self
    }

    pub fn bookmark(mut self, name: &str) -> Self {
        self.bookmark = Some(name.to_owned());
        self
    }

    pub fn operation(mut self, id: &OperationId) -> Self {
        self.operation = Some(id.as_str().to_owned());
        self
    }

    /// What `placeholder` stands for, or None where the tab has nothing
    /// of that kind.
    fn value(&self, placeholder: Placeholder) -> Option<String> {
        match placeholder {
            // What the tab is about is the most particular thing it has:
            // the file in the files tab, whose revision is only what it
            // is read at.
            Placeholder::Selected => self
                .operation
                .clone()
                .or_else(|| self.bookmark.clone())
                .or_else(|| self.file.clone())
                .or_else(|| self.revision.clone()),
            Placeholder::Revision => self.revision.clone(),
            // Several changes go to a command as the one revset that
            // names them all, that being how jj takes more than one.
            Placeholder::Marked => {
                Revset::union(&self.marked).map(|revset| revset.as_str().to_owned())
            }
            Placeholder::File => self.file.clone(),
            Placeholder::Bookmark => self.bookmark.clone(),
            Placeholder::Operation => self.operation.clone(),
        }
    }

    /// `args` with every placeholder replaced by what the selection has
    /// for it, refused when one of them has nothing to stand for.
    ///
    /// Substitution happens inside an argument, so that `--rev=$s` names
    /// the selection as much as `$s` on its own does, and after the
    /// command has been split into arguments, so that what a placeholder
    /// stands for is one argument however it reads. `$$` is a `$` of its
    /// own rather than the start of a placeholder.
    pub fn substitute(&self, args: &[String]) -> Result<Vec<String>, Missing> {
        args.iter().map(|arg| self.substitute_one(arg)).collect()
    }

    /// `arg` with every placeholder replaced by what the selection has
    /// for it.
    fn substitute_one(&self, arg: &str) -> Result<String, Missing> {
        let mut left = arg;
        let mut done = String::with_capacity(arg.len());

        while let Some(at) = left.find('$') {
            done.push_str(&left[..at]);
            let rest = &left[at..];

            if let Some(escaped) = rest.strip_prefix("$$") {
                done.push('$');
                left = escaped;
                continue;
            }

            match Self::placeholder_at(rest) {
                Some((placeholder, name)) => {
                    done.push_str(&self.value(placeholder).ok_or(Missing(placeholder))?);
                    left = &rest[name.len()..];
                }
                // A `$` that starts no placeholder is a `$` like any
                // other character, as a revset full of them has.
                None => {
                    done.push('$');
                    left = &rest[1..];
                }
            }
        }
        done.push_str(left);

        Ok(done)
    }

    /// The placeholder `arg` starts with and the name it is written as,
    /// if it starts with one at all.
    ///
    /// A name has to end where the word does, so that the short names
    /// do not turn every word starting with one into a placeholder:
    /// `$selection` is not `$s` with `election` after it.
    fn placeholder_at(arg: &str) -> Option<(Placeholder, &'static str)> {
        Placeholder::ALL.into_iter().find_map(|placeholder| {
            let name = placeholder.names().iter().find(|name| {
                arg.strip_prefix(**name)
                    .is_some_and(|rest| !rest.starts_with(|char: char| char.is_alphanumeric()))
            })?;

            Some((placeholder, *name))
        })
    }
}

/// A placeholder the tab it was to be filled from has nothing for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Missing(pub Placeholder);

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} stands for {}, and this tab has none.",
            self.0,
            self.0.doc()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander::ids::ChangeId;

    fn head(change_id: &str, commit_id: &str, divergent: bool) -> Head {
        Head {
            change_id: ChangeId(change_id.to_owned()),
            commit_id: CommitId(commit_id.to_owned()),
            divergent,
            immutable: false,
        }
    }

    /// The selection of a log with `marked` marked on top of a change
    fn log(marked: &[&str]) -> Selection {
        let marked: Vec<CommitId> = marked.iter().map(|id| CommitId((*id).to_owned())).collect();

        Selection::default()
            .revision(&head("change", "commit", false), false)
            .marked(&marked)
    }

    /// What `command` reads as against `selection`, as one line
    fn substituted(selection: &Selection, command: &[&str]) -> Result<String, Missing> {
        let args: Vec<String> = command.iter().map(|arg| (*arg).to_owned()).collect();

        Ok(selection.substitute(&args)?.join(" "))
    }

    #[test]
    fn a_placeholder_stands_for_what_the_tab_has_selected() {
        let selection = log(&[]);

        assert_eq!(
            substituted(&selection, &["show", "$selected"]),
            Ok("show change".to_owned())
        );
        assert_eq!(
            substituted(&selection, &["show", "$s"]),
            Ok("show change".to_owned())
        );
        assert_eq!(
            substituted(&selection, &["show", "$revision"]),
            Ok("show change".to_owned())
        );
    }

    /// What the tab is about is the most particular thing it knows of,
    /// not the revision every tab about a change has.
    #[test]
    fn the_selection_is_what_the_tab_is_about() {
        let files = Selection::default()
            .revision(&head("change", "commit", false), false)
            .file("src/main.rs");
        assert_eq!(
            substituted(&files, &["$selected", "$revision"]),
            Ok("src/main.rs change".to_owned())
        );

        let bookmarks = Selection::default()
            .revision(&head("change", "commit", false), false)
            .bookmark("main");
        assert_eq!(
            substituted(&bookmarks, &["$selected"]),
            Ok("main".to_owned())
        );

        let operations = Selection::default().operation(&OperationId("op".to_owned()));
        assert_eq!(
            substituted(&operations, &["$selected"]),
            Ok("op".to_owned())
        );
    }

    /// A change that is only to be found by its commit id is named by
    /// it, as it is everywhere else the app names one outside itself.
    #[test]
    fn a_revision_not_shown_as_the_change_stands_is_named_by_its_commit() {
        let pinned = Selection::default().revision(&head("change", "commit", false), true);
        assert_eq!(
            substituted(&pinned, &["$revision"]),
            Ok("commit".to_owned())
        );

        let divergent = Selection::default().revision(&head("change", "commit", true), false);
        assert_eq!(
            substituted(&divergent, &["$revision"]),
            Ok("commit".to_owned())
        );
    }

    /// jj takes more than one revision as the revset that names them
    /// all, so that is what the marks come to.
    #[test]
    fn the_marked_changes_are_one_revset() {
        assert_eq!(
            substituted(&log(&["a", "b"]), &["abandon", "$marked"]),
            Ok("abandon a | b".to_owned())
        );
        assert_eq!(
            substituted(&log(&["a"]), &["abandon", "$m"]),
            Ok("abandon a".to_owned())
        );
    }

    /// A command that was to be run against what is marked is not one
    /// to run against everything or nothing instead.
    #[test]
    fn nothing_marked_leaves_the_marked_placeholder_with_nothing_to_stand_for() {
        assert_eq!(
            substituted(&log(&[]), &["abandon", "$marked"]),
            Err(Missing(Placeholder::Marked))
        );
    }

    #[test]
    fn a_placeholder_the_tab_has_nothing_for_is_refused() {
        assert_eq!(
            substituted(&log(&[]), &["$file"]),
            Err(Missing(Placeholder::File))
        );
        assert_eq!(
            substituted(&log(&[]), &["$bookmark"]),
            Err(Missing(Placeholder::Bookmark))
        );
        assert_eq!(
            substituted(&log(&[]), &["$operation"]),
            Err(Missing(Placeholder::Operation))
        );
        // A tab about nothing at all has nothing to select.
        assert_eq!(
            substituted(&Selection::default(), &["$selected"]),
            Err(Missing(Placeholder::Selected))
        );
    }

    /// A placeholder stands for the selection wherever in an argument it
    /// is written, so that the flags that take a revision can name it.
    #[test]
    fn a_placeholder_is_replaced_inside_the_argument_holding_it() {
        assert_eq!(
            substituted(&log(&["a", "b"]), &["--rev=$s", "-r$m"]),
            Ok("--rev=change -ra | b".to_owned())
        );
    }

    /// What a placeholder stands for is one argument however it reads,
    /// the command having been split into arguments before it was put
    /// there.
    #[test]
    fn what_a_placeholder_stands_for_is_never_split_into_arguments() {
        let selection = Selection::default().file("a file with spaces.txt");

        assert_eq!(
            selection.substitute(&["diff".to_owned(), "$file".to_owned()]),
            Ok(vec!["diff".to_owned(), "a file with spaces.txt".to_owned()])
        );
    }

    #[test]
    fn a_dollar_that_starts_no_placeholder_is_one_of_its_own() {
        let selection = log(&[]);

        assert_eq!(
            substituted(&selection, &["show", "$revset"]),
            Ok("show $revset".to_owned())
        );
        assert_eq!(substituted(&selection, &["$"]), Ok("$".to_owned()));
        // A word that only starts with the name of a placeholder is not
        // that placeholder with the rest of the word after it.
        assert_eq!(
            substituted(&selection, &["$selection"]),
            Ok("$selection".to_owned())
        );
    }

    /// A revset says what to do to the changes around one, so what
    /// follows a placeholder is as much a part of the revset as the
    /// placeholder is.
    #[test]
    fn a_revset_can_be_written_around_a_placeholder() {
        assert_eq!(
            substituted(&log(&["a", "b"]), &["$s-", "$s::", "($m)|$s"]),
            Ok("change- change:: (a | b)|change".to_owned())
        );
    }

    /// A command with a `$` of its own to pass on writes it twice, as
    /// one meant for a shell variable is.
    #[test]
    fn a_doubled_dollar_is_a_dollar_of_its_own() {
        assert_eq!(
            substituted(&log(&[]), &["$$selected", "$$$s"]),
            Ok("$selected $change".to_owned())
        );
    }
}
