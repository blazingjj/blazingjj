/*! This module provides a cache of the output from 'jj show'
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
    id: Head,
    /// Formatting used to render change
    format: DiffFormat,
    /// Render width.
    /// Set to 0 for all except format=DiffTool.
    /// For DiffTool it is set to the inner with of the details panel,
    /// which is given to the tool via the COLUMNS environment variable.
    width: usize,
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
    jj_output: LargeString,
}

impl CommitShowValue {
    /// Index value, and store both key and value
    pub fn new(key: CommitShowKey, value: String) -> Self {
        Self {
            key,
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
    /// pick one at random.
    old_commits: HashMap<ChangeId, CommitShowKey>,
    /// The cache of jj show output
    commit_document: HashMap<CommitShowKey, CommitShowValue>,
}

impl CommitShowCache {
    /// Create an empty cache
    pub fn new() -> Self {
        Self {
            active_commits: HashMap::new(),
            old_commits: HashMap::new(),
            commit_document: HashMap::new(),
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

        // Construct map of old_commits from ChangeId to CommitShowKey
        // with all heads in the cache, that is not marked as an active commit
        self.old_commits = HashMap::new();
        for key in self.commit_document.keys() {
            // All cached values should either be an active or old commit.
            if !self.active_commits.contains_key(&key.id.change_id) {
                self.old_commits
                    .insert(key.id.change_id.clone(), key.clone());
            }
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
    pub fn insert_document(&mut self, value: CommitShowValue) {
        let key = &value.key;
        if let Some(old_key) = self.old_commits.get(&key.id.change_id) {
            self.commit_document.remove(old_key);
            self.old_commits.remove(&key.id.change_id);
        }
        self.commit_document.insert(key.clone(), value);
    }

    /// If key is cached, return a reference to that value,
    /// otherwise generate the value,
    /// insert it, and return a reference to the new inserted value.
    /// This works around current rustc limitations on lifetime.
    /// NOTE: fn_value must return a CommitShowValue that has the same key
    /// as the one provided.
    pub fn get_or_insert<T>(&mut self, key: &CommitShowKey, fn_value: T) -> &CommitShowValue
    where
        T: FnOnce() -> CommitShowValue,
    {
        // To fool the conservative borrow checker, we must first determine
        // which code path to follow - and not getting any borrowed value back.
        if !self.has_exact_match(key) {
            let value = fn_value();
            self.insert_document(value);
            // Assuming that the value has the exact same key as key
            // we are now guaranteed success on self.get(key) and may unwrap
        }
        self.get(key).unwrap()
    }
}
