use std::vec;

use ansi_to_tui::IntoText;
use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::prelude::*;
use ratatui::widgets::*;
use tracing::instrument;

use crate::app::TabId;
use crate::app::command::Command;
use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskOutput;
use crate::background_tasks::TaskResult;
use crate::background_tasks::TaskSlot;
use crate::commander::CommandError;
use crate::commander::cancel::CancelToken;
use crate::commander::files::Conflict;
use crate::commander::files::File;
use crate::commander::ids::ChangeId;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::env::DiffFormat;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::keybinds::DetailsPanelEvent;
use crate::keybinds::DetailsPanelKeybinds;
use crate::keybinds::FilesTabEvent;
use crate::keybinds::FilesTabKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::files_context_menu;
use crate::ui::panel::ListPane;
use crate::ui::panel::MouseInput;
use crate::ui::panel::OutputKey;
use crate::ui::panel::OutputPanel;
use crate::ui::panel::OutputRequest;
use crate::ui::panel::route_mouse;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::error_text;

/// Files tab. Shows files in selected change in main panel and selected file diff in details panel
pub struct FilesTab {
    head: Head,
    is_current_head: bool,

    /// Whether `head` stays on its commit as the change it belongs to is
    /// rewritten, rather than moving to the newest version of it
    pinned: bool,

    files_output: Result<Vec<File>, CommandError>,
    conflicts_output: Vec<Conflict>,
    files_pane: ListPane,
    files_list_state: ListState,

    pub file: Option<File>,
    diff_panel: FileDiffPanel,

    config: JjConfig,
    keybinds: FilesTabKeybinds,
    details_keybinds: DetailsPanelKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

/// A details panel showing what 'jj diff' says about a file.
type FileDiffPanel = OutputPanel<FileDiffKey>;

/// 'jj diff' output for a single file depends on all these values
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileDiffKey {
    /// Change the file is diffed in
    head: Head,
    /// File to diff
    file: File,
    /// Formatting used to render the diff
    format: DiffFormat,
}

impl OutputKey for FileDiffKey {
    type Subject = (Head, File);
    type Identity = (ChangeId, File);

    const COMMAND: &'static str = "jj diff";

    fn new((head, file): (Head, File), format: DiffFormat) -> Self {
        Self { head, file, format }
    }

    fn format(&self) -> &DiffFormat {
        &self.format
    }

    /// A change that has been rewritten still shows the diff it had,
    /// since what the panel shows of it is the file the user selected.
    fn identity(&self) -> Option<(ChangeId, File)> {
        (!self.head.divergent).then(|| (self.head.change_id.clone(), self.file.clone()))
    }

    fn render_width(&self, panel_width: usize) -> usize {
        self.format.render_width(panel_width)
    }

    fn run(&self, width: usize, cancel: &CancelToken) -> TaskOutput {
        let mut commander = new_commander();
        commander.limit_width(width);
        // A file listed without a path has no diff, which is an empty
        // document rather than a failure.
        let Some(command) = commander.build_file_diff(&self.head, &self.file, &self.format, true)
        else {
            return Ok(String::new());
        };
        Ok(command.run_cancellable(cancel)?)
    }

    fn slot(owner: TabId, request: OutputRequest<Self>) -> TaskSlot {
        TaskSlot::FileDiff(owner, request)
    }
}

fn get_current_file_index(
    current_file: Option<&File>,
    files_output: Result<&Vec<File>, &CommandError>,
) -> Option<usize> {
    if let (Some(current_file), Ok(files_output)) = (current_file, files_output)
        && let Some(path) = current_file.path.as_ref()
    {
        return files_output
            .iter()
            .position(|file| file.path.as_ref() == Some(path));
    }

    None
}

impl FilesTab {
    /// A stale tab at `current_head`, holding no files yet.
    #[instrument(level = "info", name = "Initializing files tab", parent = None, skip(background_tasks))]
    pub fn new(current_head: &Head, background_tasks: BackgroundTasks) -> Self {
        let config = get_env().jj_config.clone();
        let pane_divider = PaneDivider::new(config.layout_percent());
        let keybinds = FilesTabKeybinds::new();
        let details_keybinds = DetailsPanelKeybinds::new();

        Self {
            head: current_head.clone(),
            is_current_head: true,
            pinned: false,

            files_output: Ok(Vec::new()),
            file: None,
            files_pane: ListPane::default(),
            files_list_state: ListState::default(),

            conflicts_output: Vec::new(),

            diff_panel: FileDiffPanel::new(TabId::Files, background_tasks),

            config,
            keybinds,
            details_keybinds,
            pane_divider,

            stale: true,
        }
    }

    /// Show the files of `head`, following the change it belongs to as it
    /// is rewritten.
    pub fn set_head(&mut self, head: &Head) {
        self.show_head(head, false);
    }

    /// Show the files of one version of a change, which stays where it is
    /// however the change moves on.
    pub fn set_version(&mut self, version: &Head) {
        self.show_head(version, true);
    }

    /// Records what to show, leaving the tab stale so that the files are
    /// read the next time it is drawn.
    fn show_head(&mut self, head: &Head, pinned: bool) {
        self.head = head.clone();
        self.pinned = pinned;
        // The selection belongs to the change we were on, so the next
        // read starts at the top of the new one.
        self.file = None;
        self.stale = true;
    }

    pub fn get_current_file_index(&self) -> Option<usize> {
        get_current_file_index(self.file.as_ref(), self.files_output.as_ref())
    }

    pub fn refresh_files(&mut self) -> Result<()> {
        self.files_output = new_commander().get_files(&self.head);
        self.conflicts_output = new_commander().get_conflicts(&self.head.commit_id)?;

        if self.file.is_none() {
            self.file = self
                .files_output
                .as_ref()
                .ok()
                .and_then(|files_output| files_output.first())
                .map(|file| file.to_owned());
        }
        self.set_active_diffs();

        Ok(())
    }

    /// Have the details panel show the diff of the selected file.
    fn show_diff(&mut self) {
        let subject = self.file.clone().map(|file| (self.head.clone(), file));
        self.diff_panel.show(subject, " Diff ".to_owned());
    }

    /// Every file the change lists is one the panel may come to show
    fn set_active_diffs(&mut self) {
        let subjects = self
            .files_output
            .iter()
            .flatten()
            .map(|file| (self.head.clone(), file.clone()))
            .collect();
        self.diff_panel.set_active(subjects);
    }

    /// The menu of what can be done to the selected file, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> Option<AppAction> {
        let file = self.file.as_ref()?;

        Some(AppAction::SetPopup(Box::new(files_context_menu(
            self.config.clone(),
            anchor,
            file,
        ))))
    }

    fn handle_event(&mut self, event: FilesTabEvent) -> Result<Option<AppAction>> {
        match event {
            FilesTabEvent::Untrack => Ok(self
                .file
                .clone()
                .map(|file| AppAction::Run(Command::UntrackFile(file)))),
            FilesTabEvent::Restore => Ok(self
                .file
                .clone()
                .map(|file| AppAction::Run(Command::RestoreFile(file)))),
            // Not an operation of its own; the key handler deals with it.
            FilesTabEvent::Unbound => Ok(None),
        }
    }

    fn scroll_files(&mut self, scroll: isize) {
        if let Ok(files) = self.files_output.as_ref() {
            let current_file_index = self.get_current_file_index();
            let next_file = match current_file_index {
                Some(current_file_index) => files.get(
                    current_file_index
                        .saturating_add_signed(scroll)
                        .min(files.len() - 1),
                ),
                None => files.first(),
            }
            .map(|x| x.to_owned());
            if let Some(next_file) = next_file {
                self.file = Some(next_file.to_owned());
                self.show_diff();
            }
        }
    }
}

impl Tab for FilesTab {
    fn refresh(&mut self) -> Result<()> {
        self.is_current_head = self.head == new_commander().get_current_head()?;
        if !self.pinned {
            self.head = new_commander().get_head_latest(&self.head)?;
        }
        self.refresh_files()?;
        // The change may have moved on, which the key notices. A
        // selection that still means the same diff keeps the one it
        // has, scroll position and all.
        self.show_diff();
        self.stale = false;

        Ok(())
    }

    fn mark_stale(&mut self) {
        self.stale = true;
    }

    fn is_stale(&self) -> bool {
        self.stale
    }

    fn drop_caches(&mut self) {
        self.diff_panel.mark_dirty();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        let half_page = self.files_pane.visible_items() / 2;
        self.scroll_files(match scroll {
            Scroll::Down => 1,
            Scroll::Up => -1,
            Scroll::DownHalfPage => half_page,
            Scroll::UpHalfPage => half_page.saturating_neg(),
        });
        Ok(())
    }

    fn focus_current(&mut self) -> Result<()> {
        self.set_head(&new_commander().get_current_head()?);
        Ok(())
    }

    fn open_context_menu(&self) -> Result<Option<AppAction>> {
        Ok(self.context_menu(
            self.get_current_file_index()
                .and_then(|index| self.files_pane.item_anchor(index, 1)),
        ))
    }

    fn make_main_panel_help(&self) -> Vec<(String, String)> {
        self.keybinds.make_help()
    }

    fn make_details_panel_help(&self) -> Vec<(String, String)> {
        self.details_keybinds.make_help()
    }
}

impl Component for FilesTab {
    fn update(&mut self) -> Result<Option<AppAction>> {
        self.diff_panel.update();

        Ok(None)
    }

    fn task_done(&mut self, result: TaskResult) -> Result<Option<AppAction>> {
        if let TaskSlot::FileDiff(_, request) = result.slot {
            self.diff_panel.task_done(request, result.output);
        }
        Ok(None)
    }

    fn is_waiting(&self) -> bool {
        self.diff_panel.is_waiting()
    }

    fn draw(
        &mut self,
        f: &mut ratatui::prelude::Frame<'_>,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        let chunks = self.pane_divider.split(area, self.config.layout());

        // Draw files
        {
            let current_file_index = self.get_current_file_index();

            let mut lines: Vec<Line> = match self.files_output.as_ref() {
                Ok(files_output) => {
                    let files_lines = files_output
                        .iter()
                        .enumerate()
                        .flat_map(|(i, file)| {
                            file.line
                                .to_text()
                                .unwrap()
                                .iter()
                                .map(|line| {
                                    let mut line = line.to_owned();

                                    // Add padding at start
                                    line.spans.insert(0, Span::from(" "));

                                    if let Some(diff_type) = file.diff_type.as_ref() {
                                        line.spans = line
                                            .spans
                                            .iter_mut()
                                            .map(|span| span.to_owned().fg(diff_type.color()))
                                            .collect();
                                    }

                                    if current_file_index == Some(i) {
                                        line = line.bg(self.config.highlight_color());

                                        line.spans = line
                                            .spans
                                            .iter_mut()
                                            .map(|span| {
                                                span.to_owned().bg(self.config.highlight_color())
                                            })
                                            .collect();
                                    }

                                    line
                                })
                                .collect::<Vec<Line>>()
                        })
                        .collect::<Vec<Line>>();

                    if files_lines.is_empty() {
                        vec![
                            Line::from(" No changed files in change")
                                .fg(Color::DarkGray)
                                .italic(),
                        ]
                    } else {
                        files_lines
                    }
                }
                Err(err) => error_text("Error getting files", err)?.lines,
            };

            let title_change = if self.is_current_head {
                format!("@ ({})", self.head.change_id)
            } else if self.pinned {
                format!("{} {}", self.head.change_id, self.head.commit_id.short())
            } else {
                self.head.change_id.as_string()
            };

            if !self.conflicts_output.is_empty() {
                lines.push(Line::default());

                for conflict in &self.conflicts_output {
                    lines.push(Line::raw(format!("C {}", conflict.path)).fg(Color::Red));
                }
            }

            let block = Block::bordered()
                .title(" Files for ".to_owned() + &title_change + " ")
                .border_type(BorderType::Rounded);
            let files = List::new(lines).scroll_padding(3);
            *self.files_list_state.selected_mut() = current_file_index;
            self.files_pane
                .render(f, chunks[0], block, files, &mut self.files_list_state);
        }

        // Draw diff
        self.diff_panel.draw(f, chunks[1]);

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                return Ok(ComponentInputResult::Handled);
            }

            match self.details_keybinds.match_event(key) {
                DetailsPanelEvent::Unbound => {}
                ev => {
                    self.diff_panel.handle_event(ev);
                    return Ok(ComponentInputResult::Handled);
                }
            }

            return match self.keybinds.match_event(key) {
                // Not the tab's to act on, so whoever else wants the key
                // is welcome to it.
                FilesTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event)?.into()),
            };
        }

        if let Event::Mouse(mouse) = event {
            if self.pane_divider.handle_mouse(mouse, self.config.layout()) {
                return Ok(ComponentInputResult::Handled);
            }
            match route_mouse(mouse, &mut [&mut self.files_pane, &mut self.diff_panel]) {
                MouseInput::Scroll(delta) => self.scroll_files(delta),
                MouseInput::Select(index) => {
                    if let Ok(files) = self.files_output.as_ref()
                        && let Some(file) = files.get(index).cloned()
                    {
                        self.file = Some(file);
                        self.show_diff();
                    }
                }
                // The conflicts are listed below the files, and name no
                // file for a menu to act on.
                MouseInput::Context(index) => {
                    if let Ok(files) = self.files_output.as_ref()
                        && let Some(file) = files.get(index).cloned()
                    {
                        self.file = Some(file);
                        self.show_diff();
                        let anchor = Position::new(mouse.column, mouse.row);
                        return Ok(self.context_menu(Some(anchor)).into());
                    }
                }
                MouseInput::Handled => {}
                MouseInput::NotHandled => return Ok(ComponentInputResult::NotHandled),
            }
            return Ok(ComponentInputResult::Handled);
        }

        Ok(ComponentInputResult::Handled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander::files::DiffType;
    use crate::commander::ids::CommitId;
    use crate::env::set_test_env;

    fn head(change_id: &str, commit_id: &str) -> Head {
        Head {
            change_id: ChangeId(change_id.to_owned()),
            commit_id: CommitId(commit_id.to_owned()),
            divergent: false,
            immutable: false,
        }
    }

    fn file(path: &str) -> File {
        File {
            line: format!("M {path}"),
            path: Some(path.to_owned()),
            diff_type: Some(DiffType::Modified),
        }
    }

    fn key(change_id: &str, commit_id: &str, path: &str) -> FileDiffKey {
        FileDiffKey::new((head(change_id, commit_id), file(path)), DiffFormat::Git)
    }

    #[test]
    fn a_rewritten_change_still_shows_the_same_diff() {
        let wanted = key("change", "commit", "a.txt");
        let rewritten = key("change", "rewritten", "a.txt");

        assert_ne!(wanted, rewritten);
        assert_eq!(wanted.identity(), rewritten.identity());
    }

    #[test]
    fn another_file_or_change_is_a_diff_of_its_own() {
        let wanted = key("change", "commit", "a.txt");

        assert_ne!(
            wanted.identity(),
            key("change", "commit", "b.txt").identity()
        );
        assert_ne!(
            wanted.identity(),
            key("other", "commit", "a.txt").identity()
        );
    }

    #[test]
    fn a_divergent_change_never_shows_the_diff_of_a_sibling() {
        let divergent = |commit_id| {
            let mut key = key("change", commit_id, "a.txt");
            key.head.divergent = true;
            key
        };

        assert_eq!(divergent("commit").identity(), None);
        assert_eq!(divergent("sibling").identity(), None);
    }

    #[test]
    fn only_a_format_that_wraps_its_output_asks_for_a_width() {
        const PANEL_WIDTH: usize = 80;
        let git = key("change", "commit", "a.txt");
        let tool = FileDiffKey::new(
            (head("change", "commit"), file("a.txt")),
            DiffFormat::DiffTool(None),
        );

        assert_eq!(git.render_width(PANEL_WIDTH), 0);
        assert_eq!(tool.render_width(PANEL_WIDTH), PANEL_WIDTH);
    }

    fn tab() -> FilesTab {
        set_test_env();
        let (sender, _receiver) = std::sync::mpsc::channel();

        FilesTab::new(&head("change", "commit"), BackgroundTasks::new(sender))
    }

    #[test]
    fn being_told_what_to_show_leaves_the_reading_for_when_it_is_shown() {
        let mut tab = tab();
        tab.stale = false;

        tab.set_head(&head("other", "other"));

        assert_eq!(tab.head, head("other", "other"));
        assert!(tab.is_stale());
    }

    #[test]
    fn the_selection_starts_over_in_the_change_that_is_now_shown() {
        let mut tab = tab();
        tab.file = Some(file("a.txt"));

        tab.set_head(&head("other", "other"));

        assert_eq!(tab.file, None);
    }

    #[test]
    fn a_version_is_shown_as_it_stands_however_the_change_moves_on() {
        let mut tab = tab();

        tab.set_version(&head("change", "old"));
        assert!(tab.pinned);

        tab.set_head(&head("change", "commit"));
        assert!(!tab.pinned);
    }
}
