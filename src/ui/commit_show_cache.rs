/*! A cache of the output from 'jj show'

It is optimized for continous editing, which means that the
automatic rebase that happens when a change is modified will
also empty cache values. It does allow divergent changes, where
several visible commits share the same change id.

The design prevents a single huge commit from eating memory if
an ancester causes it to be rebased without modification lots of time.
*/

use std::collections::HashMap;
use std::collections::HashSet;

use crate::commander::ids::ChangeId;
use crate::commander::log::Head;
use crate::env::DiffFormat;
use crate::ui::utils::LargeString;

/// The change and formatting a 'jj show' output belongs to
#[derive(PartialEq, Eq, Hash, Clone)]
pub struct CommitShowKey {
    /// Commit id of shown change
    pub id: Head,
    /// Formatting used to render change
    pub format: DiffFormat,
}

impl CommitShowKey {
    pub fn new(id: Head, format: DiffFormat) -> Self {
        Self { id, format }
    }
}

/// The output from 'jj show' in a form that is fast to render a subset of
/// A structure that allows fast rendering of document with millions of lines
pub struct CommitShowValue {
    key: CommitShowKey,
    /// The render width this output was produced at
    width: usize,
    /// Whether the repo may have moved on since this output was produced
    dirty: bool,
    /// Rank in the order the cache received its documents in.
    /// Assigned by [insert_document](CommitShowCache::insert_document)
    serial: u64,
    jj_output: LargeString,
}

impl CommitShowValue {
    /// Index value, and store both key and value
    pub fn new(key: CommitShowKey, width: usize, value: String) -> Self {
        Self {
            key,
            width,
            dirty: false,
            serial: 0,
            jj_output: LargeString::new(value),
        }
    }
    pub fn value(&self) -> &LargeString {
        &self.jj_output
    }

    /// Whether this output is still what a panel of the given render
    /// width is to show
    fn is_fresh(&self, width: usize) -> bool {
        !self.dirty && self.width == width
    }
}

/// A Cache dedicated to the output of jj show for all entries in jj log.
/// Entries use the commit id as key. You specify which are currently
/// active, any commit not active will either be used as default for a
/// request where the change id match, or discarded if a true value exists.
/// You provide a list of commits that are active,
pub struct CommitShowCache {
    /// These commits will be kept. The output is a set, because
    /// ChangeId is not unique when a change is divergent
    active_commits: HashMap<ChangeId, HashSet<CommitShowKey>>,
    /// These commits will be discarded, once an active commit
    /// with same change id is in the cache. The output is not a set
    /// for simplicity. We don't care about old divergent changes and
    /// keep only the newest of them.
    old_commits: HashMap<ChangeId, CommitShowKey>,
    /// The cache of jj show output
    commit_document: HashMap<CommitShowKey, CommitShowValue>,
    /// Serial to hand to the next document
    next_serial: u64,
}

impl CommitShowCache {
    /// Create an empty cache
    pub fn new() -> Self {
        Self {
            active_commits: HashMap::new(),
            old_commits: HashMap::new(),
            commit_document: HashMap::new(),
            next_serial: 0,
        }
    }
    /// Declare which commits should be kept, as rendered in the given
    /// format. Any commit outside this set that shares change id with this
    /// set will be kept until the correct commit is available.
    pub fn set_active(&mut self, active_heads: Vec<Head>, format: &DiffFormat) {
        // Construct map of active_commits from ChangeId to HashSet<CommitShowKey>
        // containing all visible heads
        self.active_commits = HashMap::new();
        for head in active_heads {
            let key = CommitShowKey::new(head, format.clone());
            let change_id = key.id.change_id.clone();
            self.active_commits
                .entry(change_id)
                .or_default()
                .insert(key);
        }

        // Everything else in the cache is an old commit. Of those we keep
        // the newest per change id, which is the one that resembles the
        // active document it stands in for the most.
        let is_active = |key: &CommitShowKey| {
            self.active_commits
                .get(&key.id.change_id)
                .is_some_and(|active_keys| active_keys.contains(key))
        };
        let mut old_values: Vec<&CommitShowValue> = self
            .commit_document
            .values()
            .filter(|value| !is_active(&value.key))
            .collect();
        old_values.sort_unstable_by_key(|value| value.serial);

        self.old_commits = HashMap::new();
        let mut surplus = Vec::new();
        for value in old_values {
            if let Some(replaced) = self
                .old_commits
                .insert(value.key.id.change_id.clone(), value.key.clone())
            {
                surplus.push(replaced);
            }
        }
        for key in surplus {
            self.commit_document.remove(&key);
        }
    }

    /// Mark every cached output as dirty, so that it is rendered again
    /// the next time it is asked for.
    pub fn mark_dirty(&mut self) {
        for value in self.commit_document.values_mut() {
            value.dirty = true;
        }
    }

    /// Return true if the cache holds the output for the key, rendered
    /// at the given width and not outdated since.
    pub fn is_fresh(&self, key: &CommitShowKey, width: usize) -> bool {
        self.commit_document
            .get(key)
            .is_some_and(|value| value.is_fresh(width))
    }

    /// Search for best match of the provided key.
    pub fn get(&self, key: &CommitShowKey) -> Option<&CommitShowValue> {
        // Look for direct hit via CommitId
        if let Some(value) = self.commit_document.get(key) {
            return Some(value);
        }
        // Look for indirect hit via ChangeId
        if let Some(old_key) = self.old_commits.get(&key.id.change_id) {
            return self.commit_document.get(old_key);
        }
        // Give up
        None
    }

    /// Move the specified value into the cache as the active value
    /// of the key. Will remove any old values with the same change id.
    pub fn insert_document(&mut self, mut value: CommitShowValue) {
        value.serial = self.next_serial;
        self.next_serial += 1;
        let key = &value.key;
        if let Some(old_key) = self.old_commits.get(&key.id.change_id) {
            self.commit_document.remove(old_key);
            self.old_commits.remove(&key.id.change_id);
        }
        self.commit_document.insert(key.clone(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander::ids::CommitId;

    fn head(change_id: &str, commit_id: &str) -> Head {
        Head {
            change_id: ChangeId(change_id.to_owned()),
            commit_id: CommitId(commit_id.to_owned()),
            divergent: false,
            immutable: false,
        }
    }

    /// The width of a format that renders the same however wide the panel
    /// showing it is
    const NO_WIDTH: usize = 0;

    fn key(change_id: &str, commit_id: &str) -> CommitShowKey {
        CommitShowKey::new(head(change_id, commit_id), DiffFormat::ColorWords)
    }

    fn insert(cache: &mut CommitShowCache, key: &CommitShowKey, text: &str) {
        insert_at(cache, key, NO_WIDTH, text);
    }

    fn insert_at(cache: &mut CommitShowCache, key: &CommitShowKey, width: usize, text: &str) {
        cache.insert_document(CommitShowValue::new(key.clone(), width, text.to_owned()));
    }

    fn text_of(value: Option<&CommitShowValue>) -> String {
        value.unwrap().value().render(0, 1).to_string()
    }

    #[test]
    fn rewritten_commit_keeps_showing_its_previous_output() {
        let mut cache = CommitShowCache::new();
        let old = key("abc", "111");
        cache.set_active(vec![old.id.clone()], &old.format);
        insert(&mut cache, &old, "old output");

        let new = key("abc", "222");
        cache.set_active(vec![new.id.clone()], &new.format);

        assert!(!cache.is_fresh(&new, NO_WIDTH));
        assert_eq!(text_of(cache.get(&new)), "old output");
    }

    #[test]
    fn new_output_replaces_the_previous_one() {
        let mut cache = CommitShowCache::new();
        let old = key("abc", "111");
        insert(&mut cache, &old, "old output");

        let new = key("abc", "222");
        cache.set_active(vec![new.id.clone()], &new.format);
        insert(&mut cache, &new, "new output");

        assert_eq!(text_of(cache.get(&new)), "new output");
        assert_eq!(cache.commit_document.len(), 1);
    }

    #[test]
    fn output_rendered_for_another_width_is_kept() {
        let mut cache = CommitShowCache::new();
        let diff_tool = CommitShowKey::new(head("abc", "111"), DiffFormat::DiffTool(None));
        cache.set_active(vec![diff_tool.id.clone()], &diff_tool.format);
        insert_at(&mut cache, &diff_tool, 80, "narrow output");

        assert!(!cache.is_fresh(&diff_tool, 100));
        assert_eq!(text_of(cache.get(&diff_tool)), "narrow output");
    }

    #[test]
    fn dirty_output_is_kept_until_it_is_rebuilt() {
        let mut cache = CommitShowCache::new();
        let active = key("abc", "111");
        cache.set_active(vec![active.id.clone()], &active.format);
        insert(&mut cache, &active, "stale output");

        cache.mark_dirty();

        assert!(!cache.is_fresh(&active, NO_WIDTH));
        assert_eq!(text_of(cache.get(&active)), "stale output");
    }

    #[test]
    fn only_the_newest_output_of_a_change_is_kept() {
        let mut cache = CommitShowCache::new();
        insert(&mut cache, &key("abc", "111"), "first output");
        insert(&mut cache, &key("abc", "222"), "second output");

        let new = key("abc", "333");
        cache.set_active(vec![new.id.clone()], &new.format);

        assert_eq!(text_of(cache.get(&new)), "second output");
        assert_eq!(cache.commit_document.len(), 1);
    }

    #[test]
    fn divergent_commits_of_a_change_are_both_active() {
        let mut cache = CommitShowCache::new();
        let first = key("abc", "111");
        let second = key("abc", "222");
        cache.set_active(vec![first.id.clone(), second.id.clone()], &first.format);
        insert(&mut cache, &first, "first output");
        insert(&mut cache, &second, "second output");

        cache.set_active(vec![first.id.clone(), second.id.clone()], &first.format);

        assert_eq!(text_of(cache.get(&first)), "first output");
        assert_eq!(text_of(cache.get(&second)), "second output");
    }
}
