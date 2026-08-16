/*! A details panel showing what 'jj show' says about a change.

The panel keeps the change it is to show, and has the output for it
produced by a background task, so that a change jj takes a while to render
leaves the UI responsive. What it has rendered goes into a
[cache](super::commit_show_cache), so that coming back to a change is
instant.
*/

use ratatui::crossterm::event::MouseEvent;
use ratatui::layout::Rect;
use ratatui::prelude::Frame;
use tracing::error;

use super::DetailsPanel;
use super::LargeStringContent;
use super::MouseInput;
use super::PanelMouseInput;
use super::TextContent;
use super::commit_show_cache::CommitShowCache;
use super::commit_show_cache::CommitShowKey;
use super::commit_show_cache::CommitShowRequest;
use crate::app::TabId;
use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskOutput;
use crate::background_tasks::TaskSlot;
use crate::commander::log::Head;
use crate::env::DiffFormat;
use crate::env::get_env;
use crate::keybinds::DetailsPanelEvent;
use crate::ui::utils::PanelWait;

/// The change the panel renders, and the title it renders it under. The
/// title travels with the change, so that a change left on screen while
/// the next one is produced keeps the title it went up with.
#[derive(Clone)]
struct Shown {
    key: CommitShowKey,
    title: String,
}

/// A details panel showing a change, as described in the [module
/// documentation](self).
pub struct CommitShowPanel {
    /// The tab this panel belongs to, so that the results of the tasks it
    /// submits find their way back to it
    owner: TabId,

    /// The panel the output is rendered into
    panel: DetailsPanel,

    /// The change to show, if there is one
    head: Option<Head>,

    /// The title to show `head` under
    title: String,

    /// What the panel renders. This lags behind `head` while we wait for
    /// 'jj show'
    shown: Option<Shown>,

    /// Cached 'jj show' output
    cache: CommitShowCache,

    /// The 'jj show' the panel wants, as it wants it rendered
    request: Option<CommitShowRequest>,

    /// The wait for the content of `request`
    wait: PanelWait,

    /// The format changes are rendered in
    diff_format: DiffFormat,

    /// Where 'jj show' is run, so it does not block the UI thread
    background_tasks: BackgroundTasks,
}

impl CommitShowPanel {
    /// An empty panel, showing no change yet.
    pub fn new(owner: TabId, background_tasks: BackgroundTasks) -> Self {
        Self {
            owner,
            panel: DetailsPanel::new(),
            head: None,
            title: String::new(),
            shown: None,
            cache: CommitShowCache::new(),
            request: None,
            wait: PanelWait::default(),
            diff_format: get_env().jj_config.diff_format(),
            background_tasks,
        }
    }

    /// Show `head` under `title`. Without a head the panel stays empty
    /// under the title.
    pub fn show(&mut self, head: Option<Head>, title: String) {
        self.head = head;
        self.title = title;
        // A tab syncs its panels after the main loop has called
        // [Self::update], so waiting for the next one would leave the
        // panel a frame behind.
        self.update();
    }

    /// Make sure the change the panel is to show is on its way. This is
    /// where the width the panel got in the last frame enters the request,
    /// and asking is a no-op once the content is cached or already being
    /// produced.
    pub fn update(&mut self) {
        let request = self.show_request();
        if request != self.request {
            // The request we were waiting for is for a panel that has
            // since changed size, so there is nothing left for it to fill.
            if let Some(stale) = &self.request
                && request
                    .as_ref()
                    .is_some_and(|request| request.differs_only_in_width(stale))
            {
                self.background_tasks
                    .cancel(&TaskSlot::CommitShow(self.owner, stale.clone()));
            }
            self.request = request;
            self.wait.end();
        }

        let Some(request) = self.request.clone() else {
            return;
        };
        if self.cache.is_fresh(&request) {
            self.wait.end();
            return;
        }

        self.wait.begin();
        let slot = TaskSlot::CommitShow(self.owner, request.clone());
        self.background_tasks
            .submit(slot, move |cancel| request.run_jj_show(cancel));
    }

    /// Take the output of a finished 'jj show' into the cache.
    ///
    /// A result for a change the user has already scrolled past is cached
    /// too: it is what makes coming back to that change instant.
    pub fn task_done(&mut self, request: CommitShowRequest, output: TaskOutput) {
        // A rendering of the change at a width the panel has since left
        // would replace the one it is waiting for.
        if self
            .request
            .as_ref()
            .is_some_and(|wanted| wanted.differs_only_in_width(&request))
        {
            return;
        }

        let text = match output {
            Ok(output) => output,
            // A failing 'jj show' has nothing useful to display, so cache
            // the error instead of an empty document for the commit.
            Err(err) => {
                error!("'jj show' failed: {err}");
                format!("jj show failed:\n\n{err}")
            }
        };

        if self.request.as_ref() == Some(&request) {
            self.wait.end();
        }
        self.cache.insert_document(request.into_value(text));
    }

    /// Whether the panel is still waiting for content it wants.
    pub fn is_waiting(&self) -> bool {
        self.wait.is_waiting()
    }

    /// Declare which changes are worth keeping in the cache. Whatever the
    /// panel may come to show belongs here, so that the output for a
    /// change that has been rewritten stands in for it until the new one
    /// is ready.
    pub fn set_active(&mut self, heads: Vec<Head>) {
        self.cache.set_active(heads, &self.diff_format);
    }

    /// Produce every change the panel comes to show again, the repo
    /// having moved on since they were last rendered.
    pub fn mark_dirty(&mut self) {
        self.cache.mark_dirty();
    }

    pub fn handle_event(&mut self, event: DetailsPanelEvent) {
        match event {
            // The next update asks for the change in the new format
            DetailsPanelEvent::ToggleDiffFormat => {
                self.diff_format = self.diff_format.get_next(get_env().jj_config.diff_tool())
            }
            event => self.panel.handle_event(event),
        }
    }

    pub fn draw(&mut self, f: &mut Frame<'_>, area: Rect) {
        let shown = self.to_render();
        let title = match &shown {
            Some(shown) => shown.title.clone(),
            None => self.title.clone(),
        };

        if let Some(shown) = shown
            && let Some(value) = self.cache.get(&shown.key)
        {
            // Read a change from its top, but stay put while it is only
            // being rewritten under us
            let change_id = shown.key.change_id();
            if self.shown.as_ref().map(|shown| shown.key.change_id()) != Some(change_id) {
                self.panel.scroll_to(0);
            }
            self.panel
                .render_context::<LargeStringContent>(value.value())
                .title(title)
                .draw(f, area);
            self.shown = Some(shown);
            return;
        }

        // Say that we are waiting, once the wait is worth saying
        self.panel
            .render_context::<TextContent>(self.wait.message("jj show"))
            .title(title)
            .draw(f, area);
    }

    /// The 'jj show' the panel wants for the change it is to show, at the
    /// width it got in the last frame.
    fn show_request(&self) -> Option<CommitShowRequest> {
        let key = CommitShowKey::new(self.head.clone()?, self.diff_format.clone());
        Some(CommitShowRequest::new(key, self.panel.columns() as usize))
    }

    /// The content to render: the change the panel is to show as soon as
    /// the cache can serve it, and the change it already renders while we
    /// briefly wait for 'jj show'.
    fn to_render(&self) -> Option<Shown> {
        let key = self.request.as_ref()?.key();
        if self.cache.get(key).is_some() {
            return Some(Shown {
                key: key.clone(),
                title: self.title.clone(),
            });
        }
        if self.wait.within_grace() {
            return self.shown.clone();
        }
        None
    }
}

impl PanelMouseInput for CommitShowPanel {
    fn input_mouse(&mut self, mouse: MouseEvent) -> MouseInput {
        self.panel.input_mouse(mouse)
    }
}
