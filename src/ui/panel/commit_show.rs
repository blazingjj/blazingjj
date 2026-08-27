/*! A details panel showing what 'jj show' says about a change.

The panel keeps the change it is to show, and produces the output for it
in a child process, so that a change jj takes a while to render leaves the
UI responsive. What it has rendered goes into a
[cache](super::commit_show_cache), so that coming back to a change is
instant.
*/

use std::io::Read;
use std::process::Child;
use std::sync::mpsc::Sender;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use ratatui::crossterm::event::MouseEvent;
use ratatui::layout::Rect;
use ratatui::prelude::Frame;
use tracing::debug;
use tracing::error;
use tracing::warn;

use super::DetailsPanel;
use super::LargeStringContent;
use super::MouseInput;
use super::PanelMouseInput;
use super::TextContent;
use super::commit_show_cache::CommitShowCache;
use super::commit_show_cache::CommitShowKey;
use super::commit_show_cache::CommitShowRequest;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::env::DiffFormat;
use crate::env::get_env;
use crate::event::AppEvent;
use crate::keybinds::DetailsPanelEvent;
use crate::ui::utils::Timer;
use crate::ui::utils::tabs_to_spaces;
use crate::ui::utils::waiting_message;

/// How long the panel hides that 'jj show' is running: it keeps showing
/// the change it already has and, if it has none, stays empty until the
/// request it waits for has taken this long. Each request gets its own
/// grace, so moving on keeps the panel quiet.
const LOADING_GRACE: Duration = Duration::from_secs(1);

/// A background process for fetching 'jj show' and a thread for signalling
/// the UI while waiting
struct PendingJjShow {
    /// The 'jj show' the child was launched for
    request: CommitShowRequest,
    /// The child process executing 'jj show'
    child: Child,
    /// The thread reading stdout from child process
    stdout_reader: JoinHandle<Vec<u8>>,
    /// The thread reading stderr from child process
    stderr_reader: JoinHandle<Vec<u8>>,
    /// A timer used to signal the application while child is running
    timer: Timer<AppEvent>,
}

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

    /// Child process for computing 'jj show'
    pending: Option<PendingJjShow>,

    /// The format changes are rendered in
    diff_format: DiffFormat,

    /// Channel for app events
    app_event_sender: Sender<AppEvent>,
}

impl CommitShowPanel {
    /// An empty panel, showing no change yet.
    pub fn new(app_event_sender: Sender<AppEvent>) -> Self {
        Self {
            panel: DetailsPanel::new(),
            head: None,
            title: String::new(),
            shown: None,
            cache: CommitShowCache::new(),
            pending: None,
            diff_format: get_env().jj_config.diff_format(),
            app_event_sender,
        }
    }

    /// Show `head` under `title`. Without a head the panel stays empty
    /// under the title.
    pub fn show(&mut self, head: Option<Head>, title: String) {
        self.head = head;
        self.title = title;
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
            // The next draw asks for the change in the new format
            DetailsPanelEvent::ToggleDiffFormat => {
                self.diff_format = self.diff_format.get_next(get_env().jj_config.diff_tool())
            }
            event => self.panel.handle_event(event),
        }
    }

    pub fn draw(&mut self, f: &mut Frame<'_>, area: Rect) {
        self.try_read_jj_show_output();

        // The panel reports the width it got in the last frame, so the
        // first request for a change goes out at no width at all
        if let Some(request) = self.show_request()
            && !self.cache.is_fresh(&request)
        {
            self.request_jj_show(request);
        }

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
            let change_id = &shown.key.id.change_id;
            if self.shown.as_ref().map(|shown| &shown.key.id.change_id) != Some(change_id) {
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
        let waited = self.pending.as_ref().map(|pjs| pjs.timer.elapsed());
        let message = waiting_message(waited, "jj show", LOADING_GRACE);
        self.panel
            .render_context::<TextContent>(message)
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
        let key = CommitShowKey::new(self.head.clone()?, self.diff_format.clone());
        if self.cache.get(&key).is_some() {
            return Some(Shown {
                key,
                title: self.title.clone(),
            });
        }
        let pending = self.pending.as_ref()?;
        if pending.timer.elapsed() < LOADING_GRACE {
            return self.shown.clone();
        }
        None
    }

    /// Launch of a child process for 'jj show'
    fn request_jj_show(&mut self, request: CommitShowRequest) {
        // Ignore request for already pending key
        if let Some(pjs) = self.pending.as_ref()
            && pjs.request == request
        {
            return;
        }
        // Kill old child process
        if let Some(mut pjs) = self.pending.take() {
            // TODO implement std::fmt::Display for CommitShowKey
            //debug!("Kill 'jj show' that was too slow. key={}", &key);
            if let Err(err) = pjs.child.kill() {
                error!("Kill failed on 'jj show' child process: {err}");
            }
            let _ = pjs.child.wait();
        }

        // Lanuch new child process that runs 'jj show'
        let launch_key = request.key().clone();
        let mut commander = new_commander();
        commander.limit_width(request.width());
        let launch_child =
            commander.spawn_commit_show(&launch_key.id.commit_id, &launch_key.format, true);
        let mut launch_child = match launch_child {
            Ok(child) => child,
            Err(err) => {
                error!("Unable to spawn 'jj show': {err}");
                self.show_error(request, err.to_string());
                return;
            }
        };

        let stdout_reader = spawn_pipe_reader("stdout", launch_child.stdout.take().unwrap());
        let stderr_reader = spawn_pipe_reader("stderr", launch_child.stderr.take().unwrap());

        // Check for updates from child
        let launch_timer = Timer::new(self.app_event_sender.clone());
        launch_timer.signal_in(AppEvent::Refresh, Duration::from_millis(100));

        let pjs = PendingJjShow {
            request,
            child: launch_child,
            stdout_reader,
            stderr_reader,
            timer: launch_timer,
        };
        self.pending = Some(pjs);
    }

    /// Update the cache with data from the child process, if it is
    /// ready.
    fn try_read_jj_show_output(&mut self) {
        let Some(mut pjs) = self.pending.take() else {
            return;
        };

        let wait_result = pjs.child.try_wait();
        if let Err(err) = wait_result {
            // Abort on error, but log what happended
            error!(
                "Unable to get result from 'jj show'. try_wait on child failed with message: {err}"
            );
            // TODO: Maybe we want to kill the child process here?
            self.pending = Some(pjs);
            return;
        }

        let Some(status) = wait_result.unwrap() else {
            // Child not done yet
            let next_check_in = if pjs.timer.elapsed() < Duration::from_secs(1) {
                Duration::from_millis(100)
            } else {
                Duration::from_millis(1000)
            };
            pjs.timer.signal_in(AppEvent::Refresh, next_check_in);
            self.pending = Some(pjs);
            return;
        };
        debug!("jj show child process exited with status {status}");

        // Read data from child process into cache
        let stdout = pjs.stdout_reader.join().unwrap_or_default();
        let stderr_bytes = pjs.stderr_reader.join().unwrap_or_default();
        let stderr = String::from_utf8_lossy(&stderr_bytes);
        pjs.timer.stop();

        // A failing 'jj show' has nothing useful on stdout, so show the
        // error instead of caching an empty document for the commit.
        if !status.success() {
            error!("'jj show' exited with status {status}:\n{stderr}");
            self.show_error(pjs.request, format!("jj show failed:\n\n{stderr}"));
            return;
        }
        if !stderr.is_empty() {
            warn!("Ignoring stderr from child process:\n{stderr}");
        }

        let text = tabs_to_spaces(&String::from_utf8_lossy(&stdout));
        let value = pjs.request.into_value(text);
        self.cache.insert_document(value);

        // Note: self.pending.take() has already cleared the child handle,
        // which indicates room for the next child process
    }

    /// Cache `message` as the content the request asked for, so the panel
    /// shows it instead of staying blank.
    fn show_error(&mut self, request: CommitShowRequest, message: String) {
        self.cache.insert_document(request.into_value(message));
    }
}

impl PanelMouseInput for CommitShowPanel {
    fn input_mouse(&mut self, mouse: MouseEvent) -> MouseInput {
        self.panel.input_mouse(mouse)
    }
}

/// Drain a child process pipe from its own thread. The pipe buffer would
/// otherwise fill up on large diffs and block the child before it exits.
fn spawn_pipe_reader<R: Read + Send + 'static>(
    name: &'static str,
    mut pipe: R,
) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        if let Err(err) = pipe.read_to_end(&mut output) {
            error!("Failed to read {name} from 'jj show': {err}");
        }
        output
    })
}
