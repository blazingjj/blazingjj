/*!
[Commander] member functions related to various simpler jj commands.

The module implementes a number of jj commands.
Surprisingly, this module also contains jj bookmark commands.
These functions are used everywhere (bookmark tab, log tab).
*/
use anyhow::Context;
use anyhow::Result;
use tracing::instrument;

use crate::commander::CommandError;
use crate::commander::Commander;
use crate::commander::bookmarks::Bookmark;
use crate::commander::ids::CommitId;
use crate::commander::revset::Revset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewInsertMode {
    /// `jj new <rev>` — leaves children of rev in place.
    Child,
    /// `jj new --insert-after <rev>` — splices between rev and its children.
    After,
    /// `jj new --insert-before <rev>` — splices between rev and its parents.
    Before,
}

/// What a rebase takes along with the change it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseSource {
    /// `jj rebase -s <rev>` — the change and its descendants.
    Descendants,
    /// `jj rebase -b <rev>` — the whole branch the change is on.
    Branch,
    /// `jj rebase -r <rev>` — the change alone.
    SingleRevision,
}

/// Where a rebase puts what it has taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseTarget {
    /// `jj rebase -d <rev>` — onto the change, as a branch of its own.
    Onto,
    /// `jj rebase -A <rev>` — between the change and its children.
    After,
    /// `jj rebase -B <rev>` — between the change and its parents.
    Before,
}

/// What a push sends to the remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushTarget {
    /// `jj git push -r <rev>` — the tracked bookmarks on the change.
    Revision(Revset),
    /// `jj git push -b <name>` — these bookmarks, which jj starts
    /// tracking if the remote does not have them yet.
    Bookmarks(Vec<String>),
    /// `jj git push --tracked` — every tracked bookmark.
    Tracked,
    /// `jj git push --all` — every bookmark, new ones included.
    All,
    /// `jj git push -c <rev>` — a new bookmark for the change, named by
    /// jj after it.
    Change(Revset),
    /// `jj git push --named <name>=<rev>` — a new bookmark of this name
    /// for the change.
    Named { name: String, revset: Revset },
}

impl Commander {
    /// Create a new change. Maps to `jj new <revset>`. Only the tests
    /// create one without saying where it goes.
    #[cfg(test)]
    pub fn run_new(&self, revset: impl Into<Revset>) -> Result<()> {
        self.run_new_with_insert(revset, NewInsertMode::Child)
    }

    /// Like [`Self::run_new()`], but with an explicit insertion mode.
    pub fn run_new_with_insert(
        &self,
        revset: impl Into<Revset>,
        insert: NewInsertMode,
    ) -> Result<()> {
        self.run_new_inner(revset.into().as_str(), insert)
    }

    #[instrument(level = "trace", name = "run_new", skip(self))]
    fn run_new_inner(&self, revset: &str, insert: NewInsertMode) -> Result<()> {
        let mut args = vec!["new"];
        match insert {
            NewInsertMode::Child => {}
            NewInsertMode::After => args.push("--insert-after"),
            NewInsertMode::Before => args.push("--insert-before"),
        }
        args.push(revset);

        self.jj(args).run_void().context("Failed executing jj new")
    }

    /// Duplicate changes. Maps to `jj duplicate <revset>`.
    pub fn run_duplicate(&self, revset: impl Into<Revset>) -> Result<()> {
        self.jj(["duplicate", revset.into().as_str()])
            .run_void()
            .context("Failed executing jj duplicate")
    }

    /// Edit change. Maps to `jj edit <revset>`.
    pub fn run_edit(&self, revset: impl Into<Revset>, ignore_immutable: bool) -> Result<()> {
        self.run_edit_inner(revset.into().as_str(), ignore_immutable)
    }

    #[instrument(level = "trace", name = "run_edit", skip(self))]
    fn run_edit_inner(&self, revset: &str, ignore_immutable: bool) -> Result<()> {
        let mut args = vec!["edit", revset];
        if ignore_immutable {
            args.push("--ignore-immutable");
        }

        self.jj(args).run_void().context("Failed executing jj edit")
    }

    /// Abandon change. Maps to `jj abandon <revset>`.
    pub fn run_abandon(&self, revset: impl Into<Revset>) -> Result<()> {
        self.run_abandon_inner(revset.into().as_str())
    }

    #[instrument(level = "trace", name = "run_abandon", skip(self))]
    fn run_abandon_inner(&self, revset: &str) -> Result<()> {
        self.jj(["abandon", revset])
            .run_void()
            .context("Failed executing jj abandon")
    }

    /// Describe change. Maps to `jj describe <revset> --stdin`
    ///
    /// The message is passed on stdin rather than via `-m`, since jj would
    /// otherwise mistake a message starting with a dash for a flag.
    pub fn run_describe(&self, revset: impl Into<Revset>, message: &str) -> Result<()> {
        self.run_describe_inner(revset.into().as_str(), message)
    }

    #[instrument(level = "trace", name = "run_describe", skip(self))]
    fn run_describe_inner(&self, revset: &str, message: &str) -> Result<()> {
        self.jj(["describe", revset, "--stdin"])
            .stdin(message)
            .run_void()
            .context("Failed executing jj describe")
    }

    /// Rebase changes. Maps to `jj rebase -s <rev> -d <rev>` or similar
    pub fn run_rebase(
        &self,
        source: RebaseSource,
        src_rev: impl Into<Revset>,
        target: RebaseTarget,
        tgt_rev: impl Into<Revset>,
    ) -> Result<()> {
        self.run_rebase_inner(
            source,
            src_rev.into().as_str(),
            target,
            tgt_rev.into().as_str(),
        )
    }

    #[instrument(level = "trace", name = "run_rebase", skip(self))]
    fn run_rebase_inner(
        &self,
        source: RebaseSource,
        src_rev: &str,
        target: RebaseTarget,
        tgt_rev: &str,
    ) -> Result<()> {
        let source = match source {
            RebaseSource::Descendants => "-s",
            RebaseSource::Branch => "-b",
            RebaseSource::SingleRevision => "-r",
        };
        let target = match target {
            RebaseTarget::Onto => "-d",
            RebaseTarget::After => "-A",
            RebaseTarget::Before => "-B",
        };

        Ok(self
            .jj(["rebase", source, src_rev, target, tgt_rev])
            .run_void()?)
    }

    /// Parallelize changes. Maps to `jj parallelize <revset>`.
    pub fn run_parallelize(&self, revset: impl Into<Revset>) -> Result<()> {
        self.run_parallelize_inner(revset.into().as_str())
    }

    #[instrument(level = "trace", name = "run_parallelize", skip(self))]
    fn run_parallelize_inner(&self, revset: &str) -> Result<()> {
        self.jj(["parallelize", revset])
            .run_void()
            .context("Failed executing jj parallelize")
    }

    /// Squash changes. Maps to `jj squash -u [--from <revset>] --into <revset>`.
    /// `from` defaults to the working copy when `None`.
    pub fn run_squash(
        &self,
        from: Option<Revset>,
        into: impl Into<Revset>,
        ignore_immutable: bool,
    ) -> Result<()> {
        self.run_squash_inner(
            from.as_ref().map(Revset::as_str),
            into.into().as_str(),
            ignore_immutable,
        )
    }

    #[instrument(level = "trace", name = "run_squash", skip(self))]
    fn run_squash_inner(
        &self,
        from: Option<&str>,
        into: &str,
        ignore_immutable: bool,
    ) -> Result<()> {
        let mut args = vec!["squash", "-u", "--into", into];
        if let Some(f) = from {
            args.extend_from_slice(&["--from", f]);
        }
        if ignore_immutable {
            args.push("--ignore-immutable");
        }

        self.jj(args)
            .run_void()
            .context("Failed executing jj squash")
    }

    /// Absorb a change's diff into its mutable ancestors. Maps to `jj absorb --from <revset>`.
    pub fn run_absorb(&self, revset: impl Into<Revset>) -> Result<()> {
        self.run_absorb_inner(revset.into().as_str())
    }

    #[instrument(level = "trace", name = "run_absorb", skip(self))]
    fn run_absorb_inner(&self, revset: &str) -> Result<()> {
        self.jj(["absorb", "--from", revset])
            .run_void()
            .context("Failed executing jj absorb")
    }

    /// Create bookmark. Maps to `jj bookmark create <name>`
    #[instrument(level = "trace", skip(self))]
    pub fn create_bookmark(&self, name: &str) -> Result<Bookmark, CommandError> {
        self.jj(["bookmark", "create", name]).run_void()?;
        // jj only creates local bookmarks
        Ok(Bookmark {
            name: name.to_owned(),
            remote: None,
            present: true,
        })
    }

    /// Set bookmark pointing to commit. Maps to `jj bookmark set <name> -r <revision>`
    #[instrument(level = "trace", skip(self))]
    pub fn set_bookmark_commit(
        &self,
        name: &str,
        commit_id: &CommitId,
    ) -> Result<(), CommandError> {
        // TODO: Maybe don't do --allow-backwards by default?
        self.jj([
            "bookmark",
            "set",
            name,
            "-r",
            commit_id.as_str(),
            "--allow-backwards",
        ])
        .run_void()
    }

    /// Rename bookmark. Maps to `jj bookmark rename <old> <new>`
    #[instrument(level = "trace", skip(self))]
    pub fn rename_bookmark(&self, old: &str, new: &str) -> Result<(), CommandError> {
        self.jj(["bookmark", "rename", old, new]).run_void()
    }

    /// Delete bookmark. Maps to `jj bookmark delete <name>`
    #[instrument(level = "trace", skip(self))]
    pub fn delete_bookmark(&self, name: &str) -> Result<(), CommandError> {
        self.jj(["bookmark", "delete", name]).run_void()
    }

    /// Forget bookmark. Maps to `jj bookmark forget <name>`
    #[instrument(level = "trace", skip(self))]
    pub fn forget_bookmark(&self, name: &str) -> Result<(), CommandError> {
        self.jj(["bookmark", "forget", name]).run_void()
    }

    /// Track bookmark. Maps to `jj bookmark track <bookmark>@<remote>`
    #[instrument(level = "trace", skip(self))]
    pub fn track_bookmark(&self, bookmark: &Bookmark) -> Result<(), CommandError> {
        self.jj(["bookmark", "track", &bookmark.to_string()])
            .run_void()
    }

    /// Untrack bookmark. Maps to `jj bookmark untrack <bookmark>@<remote>`
    #[instrument(level = "trace", skip(self))]
    pub fn untrack_bookmark(&self, bookmark: &Bookmark) -> Result<(), CommandError> {
        self.jj(["bookmark", "untrack", &bookmark.to_string()])
            .run_void()
    }

    /// Git push. Maps to `jj git push`, which only says what it would do
    /// when `dry_run`.
    #[instrument(level = "trace", skip(self))]
    pub fn git_push(&self, target: &PushTarget, dry_run: bool) -> Result<String, CommandError> {
        let mut args = vec!["git".to_owned(), "push".to_owned()];
        if dry_run {
            args.push("--dry-run".to_owned());
        }
        match target {
            PushTarget::Revision(revset) => {
                args.push("-r".to_owned());
                args.push(revset.as_str().to_owned());
            }
            // A bookmark name is a revset symbol, quoted where a plain
            // name would not do, which is what `exact:` takes as well.
            PushTarget::Bookmarks(names) => {
                for name in names {
                    args.push("-b".to_owned());
                    args.push(format!("exact:{name}"));
                }
            }
            PushTarget::Tracked => args.push("--tracked".to_owned()),
            PushTarget::All => args.push("--all".to_owned()),
            PushTarget::Change(revset) => {
                args.push("-c".to_owned());
                args.push(revset.as_str().to_owned());
            }
            PushTarget::Named { name, revset } => {
                args.push("--named".to_owned());
                args.push(format!("{name}={}", revset.as_str()));
            }
        }

        let command = self.jj(args).color();
        if dry_run {
            // What the push would do is all jj has to say here, and it
            // says it as it does every report: on stderr, and not at all
            // when told to be quiet.
            command.verbose().with_stderr().run()
        } else {
            command.run()
        }
    }

    /// Git fetch. Maps to `jj git fetch`
    #[instrument(level = "trace", skip(self))]
    pub fn git_fetch(&self, all_remotes: bool) -> Result<String, CommandError> {
        let mut args = vec!["git", "fetch"];
        if all_remotes {
            args.push("--all-remotes");
        }

        self.jj(args).color().run()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander::ids::ChangeId;
    use crate::commander::log::Head;
    use crate::commander::tests::TestRepo;

    #[test]
    fn run_new() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let head = test_repo.commander.get_current_head()?;
        test_repo.commander.run_new(&head.commit_id)?;
        assert_ne!(head, test_repo.commander.get_current_head()?);

        Ok(())
    }

    /// A repo holding a described `base` with a described `child` on top,
    /// so that neither is abandoned for being empty when we move away.
    fn chain_of_two(test_repo: &TestRepo) -> Result<(Head, Head)> {
        let commander = &test_repo.commander;

        let base = commander.get_current_head()?;
        commander.run_describe(&base.commit_id, "base")?;
        commander.run_new(&base.commit_id)?;
        let child = commander.get_current_head()?;
        commander.run_describe(&child.commit_id, "child")?;

        Ok((base, child))
    }

    fn parent_change_id(test_repo: &TestRepo, head: &Head) -> Result<ChangeId> {
        let head = test_repo.commander.get_head_latest(head)?;
        Ok(test_repo
            .commander
            .get_commit_parent(&head.commit_id)?
            .change_id)
    }

    #[test]
    fn run_new_insert_after() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let (base, child) = chain_of_two(&test_repo)?;

        test_repo
            .commander
            .run_new_with_insert(&base.commit_id, NewInsertMode::After)?;

        let inserted = test_repo.commander.get_current_head()?;
        assert_eq!(parent_change_id(&test_repo, &inserted)?, base.change_id);
        assert_eq!(parent_change_id(&test_repo, &child)?, inserted.change_id);

        Ok(())
    }

    #[test]
    fn run_new_insert_before() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let (base, child) = chain_of_two(&test_repo)?;

        test_repo
            .commander
            .run_new_with_insert(&child.commit_id, NewInsertMode::Before)?;

        let inserted = test_repo.commander.get_current_head()?;
        assert_eq!(parent_change_id(&test_repo, &inserted)?, base.change_id);
        assert_eq!(parent_change_id(&test_repo, &child)?, inserted.change_id);

        Ok(())
    }

    #[test]
    fn run_new_leaves_the_children_in_place() -> Result<()> {
        let test_repo = TestRepo::new()?;
        let (base, child) = chain_of_two(&test_repo)?;

        test_repo.commander.run_new(&base.commit_id)?;

        let sibling = test_repo.commander.get_current_head()?;
        assert_eq!(parent_change_id(&test_repo, &sibling)?, base.change_id);
        assert_eq!(parent_change_id(&test_repo, &child)?, base.change_id);

        Ok(())
    }

    #[test]
    fn run_edit() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let head = test_repo.commander.get_current_head()?;
        test_repo.commander.run_new(&head.commit_id)?;
        assert_ne!(head, test_repo.commander.get_current_head()?);
        test_repo.commander.run_edit(&head.commit_id, false)?;
        assert_eq!(head, test_repo.commander.get_current_head()?);

        Ok(())
    }

    #[test]
    fn run_abandon() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let head = test_repo.commander.get_current_head()?;
        test_repo.commander.run_abandon(&head.commit_id)?;
        assert_ne!(head, test_repo.commander.get_current_head()?);

        Ok(())
    }

    #[test]
    fn run_describe() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let head = test_repo.commander.get_current_head()?;
        test_repo.commander.run_describe(&head.commit_id, "AAA")?;

        let head = test_repo.commander.get_current_head()?.commit_id;
        assert_eq!(test_repo.commander.get_commit_description(&head)?, "AAA");

        Ok(())
    }

    #[test]
    fn run_describe_leading_dash() -> Result<()> {
        let test_repo = TestRepo::new()?;

        // A message starting with a dash must not be mistaken for a flag.
        let head = test_repo.commander.get_current_head()?;
        test_repo.commander.run_describe(&head.commit_id, "-AAA")?;

        let head = test_repo.commander.get_current_head()?.commit_id;
        assert_eq!(test_repo.commander.get_commit_description(&head)?, "-AAA");

        Ok(())
    }

    #[test]
    fn run_squash_from() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let source = test_repo.commander.get_current_head()?;
        test_repo.commander.run_new(&source.commit_id)?;
        let dest = test_repo.commander.get_current_head()?;

        test_repo.commander.run_squash(
            Some(Revset::from(&source.commit_id)),
            &dest.commit_id,
            false,
        )?;

        // The destination commit must have been rewritten — its new
        // version is the current head.
        let new_head = test_repo.commander.get_current_head()?;
        assert_ne!(new_head.commit_id, dest.commit_id);

        Ok(())
    }

    #[test]
    fn create_bookmark() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let bookmark = test_repo.commander.create_bookmark("test")?;
        let bookmarks = test_repo.bookmarks()?;

        assert_eq!(
            bookmarks,
            [Bookmark {
                name: bookmark.name,
                remote: bookmark.remote,
                present: bookmark.present,
            }]
        );

        Ok(())
    }

    #[test]
    fn set_bookmark_commit() -> Result<()> {
        let test_repo = TestRepo::new()?;

        // Create new change, since by default `jj bookmark create` uses current change
        let old_head = test_repo.commander.get_current_head()?;
        test_repo.commander.run_new(&old_head.commit_id)?;
        let new_head = test_repo.commander.get_current_head()?;
        assert_ne!(old_head, new_head);

        let bookmark = test_repo.commander.create_bookmark("test")?;

        let log = test_repo
            .commander
            .jj([
                "log",
                "--limit",
                "1",
                "--no-graph",
                "-T",
                "commit_id",
                "-r",
                &bookmark.name,
            ])
            .run()?;

        assert_eq!(new_head.commit_id.to_string(), log);

        test_repo
            .commander
            .set_bookmark_commit(&bookmark.name, &old_head.commit_id)?;

        let log = test_repo
            .commander
            .jj([
                "log",
                "--limit",
                "1",
                "--no-graph",
                "-T",
                "commit_id",
                "-r",
                &bookmark.name,
            ])
            .run()?;

        assert_eq!(old_head.commit_id.to_string(), log);

        Ok(())
    }

    #[test]
    fn set_bookmark_commit_creates_a_bookmark_that_is_not_there_yet() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let head = test_repo.commander.get_current_head()?;
        test_repo
            .commander
            .set_bookmark_commit("test", &head.commit_id)?;

        let log = test_repo
            .commander
            .jj([
                "log",
                "--limit",
                "1",
                "--no-graph",
                "-T",
                "commit_id",
                "-r",
                "test",
            ])
            .run()?;

        assert_eq!(head.commit_id.to_string(), log);

        Ok(())
    }

    #[test]
    fn rename_bookmark() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let bookmark = test_repo.commander.create_bookmark("test1")?;

        let bookmarks = test_repo.bookmarks()?;
        assert_eq!(
            bookmarks,
            [Bookmark {
                name: bookmark.name.clone(),
                remote: bookmark.remote,
                present: bookmark.present,
            }]
        );

        test_repo
            .commander
            .rename_bookmark(&bookmark.name, "test2")?;

        let bookmarks = test_repo.bookmarks()?;
        assert_eq!(
            bookmarks,
            [Bookmark {
                name: "test2".to_owned(),
                remote: None,
                present: true,
            }]
        );

        Ok(())
    }

    #[test]
    fn delete_bookmark() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let bookmark = test_repo.commander.create_bookmark("test")?;

        let bookmarks = test_repo.bookmarks()?;
        assert_eq!(
            bookmarks,
            [Bookmark {
                name: bookmark.name.clone(),
                remote: bookmark.remote,
                present: bookmark.present,
            }]
        );

        test_repo.commander.delete_bookmark(&bookmark.name)?;

        let bookmarks = test_repo.bookmarks()?;
        assert_eq!(bookmarks, []);

        Ok(())
    }

    #[test]
    fn forget_bookmark() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let bookmark = test_repo.commander.create_bookmark("test")?;

        let bookmarks = test_repo.bookmarks()?;
        assert_eq!(
            bookmarks,
            [Bookmark {
                name: bookmark.name.clone(),
                remote: bookmark.remote,
                present: bookmark.present,
            }]
        );

        test_repo.commander.forget_bookmark(&bookmark.name)?;

        let bookmarks = test_repo.bookmarks()?;
        assert_eq!(bookmarks, []);

        Ok(())
    }
}
