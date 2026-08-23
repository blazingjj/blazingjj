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

/// 'jj show' output depends on all these values
#[derive(PartialEq, Eq, Hash, Clone)]
pub struct CommitShowKey {
    /// Commit id of shown change
    pub id: Head,
    /// Formatting used to render change
    pub format: DiffFormat,
    /// Render width.
    /// Set to 0 for all except format=DiffTool.
    /// For DiffTool it is set to the inner with of the details panel,
    /// which is given to the tool via the COLUMNS environment variable.
    pub width: usize,
}

impl CommitShowKey {
    /// Create a new key. If DiffFormat is not DiffTool, then width
    /// will be set to zero.
    pub fn new(id: Head, format: DiffFormat, width: usize) -> Self {
        // Keep with only for the DiffTool format
        let width = if let DiffFormat::DiffTool(_) = format {
            width
        } else {
            0
        };
        Self { id, format, width }
    }
}

/// The output from 'jj show' in a form that is fast to render a subset of
/// A structure that allows fast rendering of document with millions of lines
pub struct CommitShowValue {
    key: CommitShowKey,
    /// Rank in the order the cache received its documents in.
    /// Assigned by [insert_document](CommitShowCache::insert_document)
    serial: u64,
    jj_output: LargeString,
}

impl CommitShowValue {
    /// Index value, and store both key and value
    pub fn new(key: CommitShowKey, value: String) -> Self {
        Self {
            key,
            serial: 0,
            jj_output: LargeString::new(value),
        }
    }
    pub fn value(&self) -> &LargeString {
        &self.jj_output
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
    /// Declare which commits should be kept. Any commit outside this set
    /// that shares change id with this set will be kept until the correct
    /// commit is available.
    ///   The Head of the key is replaced with each head
    /// from active_heads before inserting in active commits.
    pub fn set_active(&mut self, active_heads: Vec<Head>, key: &CommitShowKey) {
        // Construct map of active_commits from ChangeId to HashSet<CommitShowKey>
        // containing all visible heads
        self.active_commits = HashMap::new();
        for head in active_heads {
            let mut key = key.clone();
            key.id = head;
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

    /// Mark all active heads as dirty by changing their width to 1.
    /// This way they will all be seen as old next time [set_active](Self.set_active) is called.
    pub fn mark_dirty(&mut self) {
        // Collect all keys for active commits
        // std::mem::take moves the map out of self and leaves an empty one in its place
        let active_commits = std::mem::take(&mut self.active_commits);
        let active_keys: Vec<CommitShowKey> = active_commits.values().flatten().cloned().collect();
        // Mark document as dirty
        for ac_key in active_keys {
            let Some(mut value) = self.commit_document.remove(&ac_key) else {
                continue;
            };
            value.key.width = 1;
            self.insert_document(value);
        }
    }

    /// Return true if the key is present as active
    pub fn has_exact_match(&self, key: &CommitShowKey) -> bool {
        self.commit_document.contains_key(key)
    }

    /// Search for best match of the provided key.
    pub fn get(&self, key: &CommitShowKey) -> Option<&CommitShowValue> {
        // Look for direct hit via CommitId
        if self.has_exact_match(key) {
            return self.commit_document.get(key);
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

    fn key(change_id: &str, commit_id: &str) -> CommitShowKey {
        CommitShowKey::new(head(change_id, commit_id), DiffFormat::ColorWords, 0)
    }

    /// A key of the only format that renders at a given width
    fn tool_key(change_id: &str, commit_id: &str, width: usize) -> CommitShowKey {
        CommitShowKey::new(
            head(change_id, commit_id),
            DiffFormat::DiffTool(None),
            width,
        )
    }

    fn insert(cache: &mut CommitShowCache, key: &CommitShowKey, text: &str) {
        cache.insert_document(CommitShowValue::new(key.clone(), text.to_owned()));
    }

    fn text_of(value: Option<&CommitShowValue>) -> String {
        value.unwrap().value().render(0, 1).to_string()
    }

    #[test]
    fn rewritten_commit_keeps_showing_its_previous_output() {
        let mut cache = CommitShowCache::new();
        let old = key("abc", "111");
        cache.set_active(vec![old.id.clone()], &old);
        insert(&mut cache, &old, "old output");

        let new = key("abc", "222");
        cache.set_active(vec![new.id.clone()], &new);

        assert!(!cache.has_exact_match(&new));
        assert_eq!(text_of(cache.get(&new)), "old output");
    }

    #[test]
    fn new_output_replaces_the_previous_one() {
        let mut cache = CommitShowCache::new();
        let old = key("abc", "111");
        insert(&mut cache, &old, "old output");

        let new = key("abc", "222");
        cache.set_active(vec![new.id.clone()], &new);
        insert(&mut cache, &new, "new output");

        assert_eq!(text_of(cache.get(&new)), "new output");
        assert_eq!(cache.commit_document.len(), 1);
    }

    #[test]
    fn output_rendered_for_another_width_is_kept() {
        let mut cache = CommitShowCache::new();
        let narrow = tool_key("abc", "111", 80);
        cache.set_active(vec![narrow.id.clone()], &narrow);
        insert(&mut cache, &narrow, "narrow output");

        let wide = tool_key("abc", "111", 100);
        cache.set_active(vec![wide.id.clone()], &wide);

        assert!(!cache.has_exact_match(&wide));
        assert_eq!(text_of(cache.get(&wide)), "narrow output");
    }

    #[test]
    fn dirty_output_is_kept_until_it_is_rebuilt() {
        let mut cache = CommitShowCache::new();
        let active = key("abc", "111");
        cache.set_active(vec![active.id.clone()], &active);
        insert(&mut cache, &active, "stale output");

        cache.mark_dirty();
        cache.set_active(vec![active.id.clone()], &active);

        assert!(!cache.has_exact_match(&active));
        assert_eq!(text_of(cache.get(&active)), "stale output");
    }

    #[test]
    fn only_the_newest_output_of_a_change_is_kept() {
        let mut cache = CommitShowCache::new();
        insert(&mut cache, &key("abc", "111"), "first output");
        insert(&mut cache, &key("abc", "222"), "second output");

        let new = key("abc", "333");
        cache.set_active(vec![new.id.clone()], &new);

        assert_eq!(text_of(cache.get(&new)), "second output");
        assert_eq!(cache.commit_document.len(), 1);
    }

    #[test]
    fn divergent_commits_of_a_change_are_both_active() {
        let mut cache = CommitShowCache::new();
        let first = key("abc", "111");
        let second = key("abc", "222");
        cache.set_active(vec![first.id.clone(), second.id.clone()], &first);
        insert(&mut cache, &first, "first output");
        insert(&mut cache, &second, "second output");

        cache.set_active(vec![first.id.clone(), second.id.clone()], &first);

        assert_eq!(text_of(cache.get(&first)), "first output");
        assert_eq!(text_of(cache.get(&second)), "second output");
    }
}
