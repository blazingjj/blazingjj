/*! A details panel showing the output of a jj command.

The panel keeps what it is to show, and has the output for it produced by
a background task, so that a command jj takes a while to answer leaves the
UI responsive. What it has rendered goes into a
[cache](super::output_cache), so that coming back to it is instant.
*/

use ratatui::crossterm::event::MouseEvent;
use ratatui::layout::Rect;
use ratatui::prelude::Frame;
use ratatui::text::Text;
use tracing::error;

use super::DetailsPanel;
use super::LargeStringContent;
use super::MouseInput;
use super::PanelMouseInput;
use super::TextContent;
use super::output_cache::OutputCache;
use super::output_cache::OutputKey;
use super::output_cache::OutputRequest;
use crate::app::TabId;
use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskOutput;
use crate::env::DiffFormat;
use crate::env::get_env;
use crate::keybinds::DetailsPanelEvent;
use crate::ui::utils::PanelWait;
use crate::ui::utils::error_text;

/// What the panel renders, and the title it renders it under. The title
/// travels with the content, so that what is left on screen while the
/// next thing is produced keeps the title it went up with.
#[derive(Clone)]
struct Shown<K> {
    key: K,
    title: String,
}

/// A details panel showing jj output, as described in the [module
/// documentation](self).
pub struct OutputPanel<K: OutputKey> {
    /// The tab this panel belongs to, so that the results of the tasks it
    /// submits find their way back to it
    owner: TabId,

    /// The panel the output is rendered into
    panel: DetailsPanel,

    /// What to show, if there is anything
    subject: Option<K::Subject>,

    /// The title to show `subject` under
    title: String,

    /// What the panel renders. This lags behind `subject` while we wait
    /// for the command
    shown: Option<Shown<K>>,

    /// Cached output
    cache: OutputCache<K>,

    /// The output the panel wants, as it wants it rendered
    request: Option<OutputRequest<K>>,

    /// The wait for the output of `request`
    wait: PanelWait,

    /// The format changes are rendered in
    diff_format: DiffFormat,

    /// Where the command is run, so it does not block the UI thread
    background_tasks: BackgroundTasks,
}

impl<K: OutputKey> OutputPanel<K> {
    /// An empty panel, showing nothing yet.
    pub fn new(owner: TabId, background_tasks: BackgroundTasks) -> Self {
        Self {
            owner,
            panel: DetailsPanel::new(),
            subject: None,
            title: String::new(),
            shown: None,
            cache: OutputCache::new(),
            request: None,
            wait: PanelWait::default(),
            diff_format: get_env().jj_config.diff_format(),
            background_tasks,
        }
    }

    /// Show `subject` under `title`. Without a subject the panel stays
    /// empty under the title.
    pub fn show(&mut self, subject: Option<K::Subject>, title: String) {
        self.subject = subject;
        self.title = title;
        // A tab syncs its panels after the main loop has called
        // [Self::update], so waiting for the next one would leave the
        // panel a frame behind.
        self.update();
    }

    /// Make sure what the panel is to show is on its way. This is where
    /// the width the panel got in the last frame enters the request, and
    /// asking is a no-op once the output is cached or already being
    /// produced.
    pub fn update(&mut self) {
        let panel_width = self.panel.columns() as usize;
        let request = self.subject.clone().map(|subject| {
            OutputRequest::new(K::new(subject, self.diff_format.clone()), panel_width)
        });

        if request != self.request {
            // The request we were waiting for is for a panel that has
            // since changed size, so there is nothing left for it to fill.
            if let Some(stale) = &self.request
                && request
                    .as_ref()
                    .is_some_and(|request| request.differs_only_in_width(stale))
            {
                self.background_tasks
                    .cancel(&K::slot(self.owner, stale.clone()));
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

        // The panel reports the width it was drawn at last, so before the
        // first frame there is no width to produce for yet. The wait keeps
        // the main loop coming back until there is.
        if panel_width == 0 {
            return;
        }

        self.background_tasks
            .submit(K::slot(self.owner, request.clone()), move |cancel| {
                request.run(cancel)
            });
    }

    /// Take the output of a finished command into the cache.
    ///
    /// A result for something the user has already moved on from is cached
    /// too: it is what makes coming back to it instant.
    pub fn task_done(&mut self, request: OutputRequest<K>, output: TaskOutput) {
        // A rendering at a width the panel has since left would replace
        // the one it is waiting for.
        if self
            .request
            .as_ref()
            .is_some_and(|wanted| wanted.differs_only_in_width(&request))
        {
            return;
        }

        // A failing command has nothing useful to display, so the error is
        // cached in place of the output and shown instead of it.
        if let Err(err) = &output {
            error!("'{}' failed: {err}", K::COMMAND);
        }

        if self.request.as_ref() == Some(&request) {
            self.wait.end();
        }
        self.cache.insert_document(request.into_value(output));
    }

    /// Whether the panel is still waiting for output it wants.
    pub fn is_waiting(&self) -> bool {
        self.wait.is_waiting()
    }

    /// Declare what is worth keeping in the cache. Whatever the panel may
    /// come to show belongs here, so that the output for a change that has
    /// been rewritten stands in for it until the new one is ready.
    pub fn set_active(&mut self, subjects: Vec<K::Subject>) {
        self.cache.set_active(subjects, &self.diff_format);
    }

    /// Produce everything the panel comes to show again, the repo having
    /// moved on since it was last rendered.
    pub fn mark_dirty(&mut self) {
        self.cache.mark_dirty();
    }

    pub fn handle_event(&mut self, event: DetailsPanelEvent) {
        match event {
            // The next update asks for the output in the new format
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
            // Read new content from its top, but stay put while what is
            // on screen is only being produced again
            let on_screen = self.shown.as_ref().map(|shown| &shown.key);
            if !stands_in_for(on_screen, &shown.key) {
                self.panel.scroll_to(0);
            }

            match value.output() {
                Ok(document) => self
                    .panel
                    .render_context::<LargeStringContent>(document)
                    .title(title)
                    .draw(f, area),
                Err(message) => {
                    let failed = format!("'{}' failed", K::COMMAND);
                    let text = error_text(&failed, &message)
                        .unwrap_or_else(|_| Text::raw(message.to_owned()));
                    self.panel
                        .render_context::<TextContent>(text)
                        .title(title)
                        .draw(f, area);
                }
            }
            self.shown = Some(shown);
            return;
        }

        // Say that we are waiting, once the wait is worth saying
        self.panel
            .render_context::<TextContent>(self.wait.message(K::COMMAND))
            .title(title)
            .draw(f, area);
    }

    /// The content to render: what the panel is to show as soon as the
    /// cache can serve it, and what it already renders while we briefly
    /// wait for the command.
    fn to_render(&self) -> Option<Shown<K>> {
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

/// Whether what is on screen is the same content as `wanted`, only
/// produced again. Content that may stand in for nothing never is.
fn stands_in_for<K: OutputKey>(on_screen: Option<&K>, wanted: &K) -> bool {
    let Some(identity) = wanted.identity() else {
        return false;
    };
    on_screen.and_then(K::identity) == Some(identity)
}

impl<K: OutputKey> PanelMouseInput for OutputPanel<K> {
    fn input_mouse(&mut self, mouse: MouseEvent) -> MouseInput {
        self.panel.input_mouse(mouse)
    }
}
