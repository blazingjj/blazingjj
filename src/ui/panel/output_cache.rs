/*! The jj output a details panel needs, and a cache of what it produced

It is optimized for continous editing, which means that the
automatic rebase that happens when a change is modified will
also empty cache values. It does allow divergent changes, where
several visible commits share the same change id.

The design prevents a single huge commit from eating memory if
an ancester causes it to be rebased without modification lots of time.
*/

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;

use crate::app::TabId;
use crate::background_tasks::TaskOutput;
use crate::background_tasks::TaskSlot;
use crate::commander::cancel::CancelToken;
use crate::env::DiffFormat;
use crate::ui::utils::LargeString;
use crate::ui::utils::tabs_to_spaces;

/// One document of jj output, named by everything it depends on except
/// the width it is rendered at.
pub trait OutputKey: Clone + Eq + Hash + Debug + Send + 'static {
    /// What a tab points its panel at. The format is the panel's own, so
    /// it makes the key out of both.
    type Subject: Clone + PartialEq;

    /// What two keys have to agree on for the output of one to stand in
    /// for the other while that one is produced.
    type Identity: Clone + Eq + Hash;

    /// The command the output comes from, as the panel names it while it
    /// waits.
    const COMMAND: &'static str;

    fn new(subject: Self::Subject, format: DiffFormat) -> Self;

    /// The format the output is rendered in.
    fn format(&self) -> &DiffFormat;

    /// What this output may stand in for, if anything. A divergent change
    /// has nothing: its change id names more than one commit, and the
    /// output of the wrong one is not worth showing.
    fn identity(&self) -> Option<Self::Identity>;

    /// The width the output is produced at in a panel this wide, which is
    /// no width at all for a format that does not depend on it.
    fn render_width(&self, panel_width: usize) -> usize;

    /// Run the command that produces the output, rendered `width` wide.
    /// Blocks until the command is done, so it belongs on a background
    /// task.
    fn run(&self, width: usize, cancel: &CancelToken) -> TaskOutput;

    /// The slot `request` runs in on behalf of the panel in `owner`.
    fn slot(owner: TabId, request: OutputRequest<Self>) -> TaskSlot;
}

/// A command to run: the output to produce, and the width to produce it
/// at. Two requests that differ only in a width neither format nor tool
/// acts on are the same request.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct OutputRequest<K> {
    key: K,
    width: usize,
}

impl<K: OutputKey> OutputRequest<K> {
    /// A request for the details panel of the given width.
    pub fn new(key: K, panel_width: usize) -> Self {
        let width = key.render_width(panel_width);
        Self { key, width }
    }

    pub fn key(&self) -> &K {
        &self.key
    }

    /// Whether `other` asks for the same content as this request, rendered
    /// at a width it does not want. Both fill the same cache entry, so only
    /// one of them can be there.
    pub fn differs_only_in_width(&self, other: &Self) -> bool {
        self.key == other.key && self.width != other.width
    }

    /// Produce what this request asks for. Blocks until the command is
    /// done, so it belongs on a background task.
    pub fn run(&self, cancel: &CancelToken) -> TaskOutput {
        let output = self.key.run(self.width, cancel)?;

        // A copy of the whole document, which the UI thread has no reason
        // to make once the task is already holding it.
        Ok(tabs_to_spaces(&output))
    }

    /// The output this request produced, ready for the cache.
    pub fn into_value(self, output: TaskOutput) -> OutputValue<K> {
        OutputValue::new(self.key, self.width, output)
    }
}

/// What a request produced, in a form that is fast to render a subset of:
/// the document, or the error that came instead of one.
pub struct OutputValue<K> {
    key: K,
    /// The render width this output was produced at
    width: usize,
    /// Whether the repo may have moved on since this output was produced
    dirty: bool,
    /// Rank in the order the cache received its documents in.
    /// Assigned by [insert_document](OutputCache::insert_document)
    serial: u64,
    output: Result<LargeString, String>,
}

impl<K: OutputKey> OutputValue<K> {
    /// Index the output, and store it under its key
    pub fn new(key: K, width: usize, output: TaskOutput) -> Self {
        Self {
            key,
            width,
            dirty: false,
            serial: 0,
            output: output.map(LargeString::new).map_err(|err| err.to_string()),
        }
    }

    /// What the panel renders: the document, or the message of the error
    /// that came instead.
    pub fn output(&self) -> Result<&LargeString, &str> {
        self.output.as_ref().map_err(String::as_str)
    }

    /// Whether this output is still what a panel of the given render
    /// width is to show
    fn is_fresh(&self, width: usize) -> bool {
        !self.dirty && self.width == width
    }
}

/// A cache of the output a details panel has shown. You declare which
/// keys are currently active; the output of any other key either stands
/// in for an active one of the same [identity](OutputKey::identity), or
/// is discarded once the real one is there.
pub struct OutputCache<K: OutputKey> {
    /// The newest key of each identity that is no longer active. Its
    /// document is kept as a stand-in until the active one arrives. There
    /// is only one per identity, since we do not care about anything older
    /// than the closest thing to what is wanted.
    stand_in: HashMap<K::Identity, K>,
    /// The cache of jj output
    documents: HashMap<K, OutputValue<K>>,
    /// Serial to hand to the next document
    next_serial: u64,
}

impl<K: OutputKey> OutputCache<K> {
    /// Create an empty cache
    pub fn new() -> Self {
        Self {
            stand_in: HashMap::new(),
            documents: HashMap::new(),
            next_serial: 0,
        }
    }

    /// Declare what the panel may come to show, as rendered in the given
    /// format. Everything else is kept only as long as it stands in for
    /// one of these.
    pub fn set_active(&mut self, subjects: Vec<K::Subject>, format: &DiffFormat) {
        let active: HashSet<K> = subjects
            .into_iter()
            .map(|subject| K::new(subject, format.clone()))
            .collect();

        // Everything else in the cache is old. Of those we keep the newest
        // per identity, which is the one that resembles the active document
        // it stands in for the most.
        let mut old: Vec<&OutputValue<K>> = self
            .documents
            .values()
            .filter(|value| !active.contains(&value.key))
            .collect();
        old.sort_unstable_by_key(|value| value.serial);

        self.stand_in = HashMap::new();
        let mut surplus = Vec::new();
        for value in old {
            // A document that may stand in for nothing is of no further use
            let Some(identity) = value.key.identity() else {
                surplus.push(value.key.clone());
                continue;
            };
            if let Some(replaced) = self.stand_in.insert(identity, value.key.clone()) {
                surplus.push(replaced);
            }
        }
        for key in surplus {
            self.documents.remove(&key);
        }
    }

    /// Mark every cached output as dirty, so that it is produced again
    /// the next time it is asked for.
    pub fn mark_dirty(&mut self) {
        for value in self.documents.values_mut() {
            value.dirty = true;
        }
    }

    /// Return true if the cache holds what the request asks for, rendered
    /// at the width it asks for and not outdated since.
    pub fn is_fresh(&self, request: &OutputRequest<K>) -> bool {
        self.documents
            .get(&request.key)
            .is_some_and(|value| value.is_fresh(request.width))
    }

    /// The best the cache can do for `key`: its own output, or that of
    /// whatever stands in for it.
    pub fn get(&self, key: &K) -> Option<&OutputValue<K>> {
        if let Some(value) = self.documents.get(key) {
            return Some(value);
        }
        self.documents.get(self.stand_in.get(&key.identity()?)?)
    }

    /// Move the specified value into the cache as the output of its key,
    /// removing whatever stood in for it.
    pub fn insert_document(&mut self, mut value: OutputValue<K>) {
        value.serial = self.next_serial;
        self.next_serial += 1;
        if let Some(identity) = value.key.identity()
            && let Some(stand_in) = self.stand_in.remove(&identity)
        {
            self.documents.remove(&stand_in);
        }
        self.documents.insert(value.key.clone(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander::ids::ChangeId;
    use crate::commander::ids::CommitId;
    use crate::commander::log::Head;
    use crate::ui::panel::CommitShowKey;

    fn head(change_id: &str, commit_id: &str) -> Head {
        Head {
            change_id: ChangeId(change_id.to_owned()),
            commit_id: CommitId(commit_id.to_owned()),
            divergent: false,
            immutable: false,
        }
    }

    const PANEL_WIDTH: usize = 80;

    /// The width of a format whose output does not depend on it
    const NO_WIDTH: usize = 0;

    fn key(change_id: &str, commit_id: &str) -> CommitShowKey {
        CommitShowKey::new(head(change_id, commit_id), DiffFormat::ColorWords)
    }

    fn request(key: &CommitShowKey, panel_width: usize) -> OutputRequest<CommitShowKey> {
        OutputRequest::new(key.clone(), panel_width)
    }

    fn insert(cache: &mut OutputCache<CommitShowKey>, key: &CommitShowKey, text: &str) {
        insert_at(cache, key, NO_WIDTH, text);
    }

    fn insert_at(
        cache: &mut OutputCache<CommitShowKey>,
        key: &CommitShowKey,
        width: usize,
        text: &str,
    ) {
        cache.insert_document(OutputValue::new(key.clone(), width, Ok(text.to_owned())));
    }

    fn text_of(value: Option<&OutputValue<CommitShowKey>>) -> String {
        value.unwrap().output().unwrap().render(0, 1).to_string()
    }

    #[test]
    fn rewritten_commit_keeps_showing_its_previous_output() {
        let mut cache = OutputCache::new();
        let old = key("abc", "111");
        cache.set_active(vec![head("abc", "111")], &DiffFormat::ColorWords);
        insert(&mut cache, &old, "old output");

        let new = key("abc", "222");
        cache.set_active(vec![head("abc", "222")], &DiffFormat::ColorWords);

        assert!(!cache.is_fresh(&request(&new, PANEL_WIDTH)));
        assert_eq!(text_of(cache.get(&new)), "old output");
    }

    #[test]
    fn new_output_replaces_the_previous_one() {
        let mut cache = OutputCache::new();
        insert(&mut cache, &key("abc", "111"), "old output");

        let new = key("abc", "222");
        cache.set_active(vec![head("abc", "222")], &DiffFormat::ColorWords);
        insert(&mut cache, &new, "new output");

        assert_eq!(text_of(cache.get(&new)), "new output");
        assert_eq!(cache.documents.len(), 1);
    }

    #[test]
    fn a_resize_makes_a_request_for_the_same_change_collide() {
        let diff_tool = CommitShowKey::new(head("abc", "111"), DiffFormat::DiffTool(None));
        let narrow = request(&diff_tool, PANEL_WIDTH);
        let wide = request(&diff_tool, PANEL_WIDTH * 2);

        assert!(narrow.differs_only_in_width(&wide));
        assert!(wide.differs_only_in_width(&narrow));
        assert!(!narrow.differs_only_in_width(&narrow));
    }

    #[test]
    fn a_request_for_another_change_does_not_collide() {
        let key = CommitShowKey::new(head("abc", "111"), DiffFormat::DiffTool(None));
        let other = CommitShowKey::new(head("def", "222"), DiffFormat::DiffTool(None));

        assert!(
            !request(&key, PANEL_WIDTH).differs_only_in_width(&request(&other, PANEL_WIDTH * 2))
        );
    }

    #[test]
    fn a_resize_leaves_a_width_independent_request_alone() {
        let color_words = key("abc", "111");

        assert!(
            !request(&color_words, PANEL_WIDTH)
                .differs_only_in_width(&request(&color_words, PANEL_WIDTH * 2))
        );
    }

    #[test]
    fn output_rendered_for_another_width_is_kept() {
        let mut cache = OutputCache::new();
        let diff_tool = CommitShowKey::new(head("abc", "111"), DiffFormat::DiffTool(None));
        cache.set_active(vec![head("abc", "111")], &DiffFormat::DiffTool(None));
        insert_at(&mut cache, &diff_tool, PANEL_WIDTH, "narrow output");

        assert!(!cache.is_fresh(&request(&diff_tool, PANEL_WIDTH * 2)));
        assert_eq!(text_of(cache.get(&diff_tool)), "narrow output");
    }

    #[test]
    fn output_that_does_not_depend_on_the_width_survives_a_resize() {
        let mut cache = OutputCache::new();
        let color_words = key("abc", "111");
        insert(&mut cache, &color_words, "output");

        assert!(cache.is_fresh(&request(&color_words, PANEL_WIDTH)));
        assert!(cache.is_fresh(&request(&color_words, PANEL_WIDTH * 2)));
    }

    #[test]
    fn dirty_output_is_kept_until_it_is_rebuilt() {
        let mut cache = OutputCache::new();
        let active = key("abc", "111");
        cache.set_active(vec![head("abc", "111")], &DiffFormat::ColorWords);
        insert(&mut cache, &active, "stale output");

        cache.mark_dirty();

        assert!(!cache.is_fresh(&request(&active, PANEL_WIDTH)));
        assert_eq!(text_of(cache.get(&active)), "stale output");
    }

    #[test]
    fn only_the_newest_output_of_a_change_is_kept() {
        let mut cache = OutputCache::new();
        insert(&mut cache, &key("abc", "111"), "first output");
        insert(&mut cache, &key("abc", "222"), "second output");

        let new = key("abc", "333");
        cache.set_active(vec![head("abc", "333")], &DiffFormat::ColorWords);

        assert_eq!(text_of(cache.get(&new)), "second output");
        assert_eq!(cache.documents.len(), 1);
    }

    #[test]
    fn divergent_commits_of_a_change_are_both_active() {
        let mut cache = OutputCache::new();
        let mut first_head = head("abc", "111");
        first_head.divergent = true;
        let mut second_head = head("abc", "222");
        second_head.divergent = true;
        let first = CommitShowKey::new(first_head.clone(), DiffFormat::ColorWords);
        let second = CommitShowKey::new(second_head.clone(), DiffFormat::ColorWords);
        let active = vec![first_head, second_head];
        cache.set_active(active.clone(), &DiffFormat::ColorWords);
        insert(&mut cache, &first, "first output");
        insert(&mut cache, &second, "second output");

        cache.set_active(active, &DiffFormat::ColorWords);

        assert_eq!(text_of(cache.get(&first)), "first output");
        assert_eq!(text_of(cache.get(&second)), "second output");
    }

    #[test]
    fn a_divergent_commit_is_never_stood_in_for() {
        let mut cache = OutputCache::new();
        let mut divergent = head("abc", "111");
        divergent.divergent = true;
        insert(
            &mut cache,
            &CommitShowKey::new(divergent.clone(), DiffFormat::ColorWords),
            "output",
        );

        // Its change id names its sibling as well, so the sibling gets
        // nothing rather than the wrong commit
        let mut sibling = head("abc", "222");
        sibling.divergent = true;
        cache.set_active(vec![sibling.clone()], &DiffFormat::ColorWords);

        assert!(
            cache
                .get(&CommitShowKey::new(sibling, DiffFormat::ColorWords))
                .is_none()
        );
        assert!(cache.documents.is_empty());
    }
}
