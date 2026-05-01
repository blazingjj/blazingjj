use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::mpsc::channel;
use std::thread;
use std::vec;

type DiffKey = (Option<String>, DiffFormat, usize);
type DiffMsg = (u64, DiffKey, Result<Option<String>, CommandError>);

use ansi_to_tui::IntoText;
use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::crossterm::event::KeyModifiers;
use ratatui::prelude::*;
use ratatui::widgets::*;
use tracing::instrument;

use crate::ComponentInputResult;
use crate::commander::CommandError;
use crate::commander::files::Conflict;
use crate::commander::files::File;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::env::DiffFormat;
use crate::env::JJLayout;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::ui::Component;
use crate::ui::ComponentAction;
use crate::ui::help_popup::HelpPopup;
use crate::ui::message_popup::MessagePopup;
use crate::ui::panel::DetailsPanel;
use crate::ui::panel::ListPane;
use crate::ui::panel::MouseInput;
use crate::ui::panel::TextContent;
use crate::ui::panel::route_mouse;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::tabs_to_spaces;

/// Files tab. Shows files in selected change in main panel and selected file diff in details panel
pub struct FilesTab {
    head: Head,
    is_current_head: bool,

    files_output: Result<Vec<File>, CommandError>,
    conflicts_output: Vec<Conflict>,
    files_pane: ListPane,
    files_list_state: ListState,

    pub file: Option<File>,
    diff_panel: DetailsPanel,
    diff_output: Result<Option<String>, CommandError>,
    diff_cache: HashMap<DiffKey, String>,
    diff_inflight: HashSet<DiffKey>,
    diff_tx: Sender<DiffMsg>,
    diff_rx: Receiver<DiffMsg>,
    diff_generation: u64,
    diff_format: DiffFormat,

    config: JjConfig,
    layout: JJLayout,
    pane_divider: PaneDivider,
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
    #[instrument(level = "info", name = "Initializing files tab", parent = None, skip())]
    pub fn new(head: &Head) -> Result<Self> {
        let head = head.clone();
        let is_current_head = head == new_commander().get_current_head()?;

        let diff_format = get_env().jj_config.diff_format();

        let files_output = new_commander().get_files(&head);
        let conflicts_output = new_commander().get_conflicts(&head.commit_id)?;
        let current_file = files_output
            .as_ref()
            .ok()
            .and_then(|files_output| files_output.first())
            .map(|file| file.to_owned());
        let diff_output = current_file
            .as_ref()
            .map(|current_change| {
                new_commander().get_file_diff(&head, current_change, &diff_format, true)
            })
            .map_or(Ok(None), |r| {
                r.map(|diff| diff.map(|diff| tabs_to_spaces(&diff)))
            });

        let files_list_state = ListState::default().with_selected(get_current_file_index(
            current_file.as_ref(),
            files_output.as_ref(),
        ));
        let files_pane = ListPane::default();

        let config = get_env().jj_config.clone();
        let layout = config.layout();
        let pane_divider = PaneDivider::new(config.layout_percent());

        let mut diff_cache = HashMap::new();
        if let (Ok(Some(s)), Some(file)) = (&diff_output, &current_file) {
            diff_cache.insert((file.path.clone(), diff_format.clone(), 0usize), s.clone());
        }

        let (diff_tx, diff_rx) = channel();

        let mut tab = Self {
            head,
            is_current_head,

            files_output,
            file: current_file,
            files_pane,
            files_list_state,

            conflicts_output,

            diff_output,
            diff_cache,
            diff_inflight: HashSet::new(),
            diff_tx,
            diff_rx,
            diff_generation: 0,
            diff_format,
            diff_panel: DetailsPanel::new(),

            config,
            layout,
            pane_divider,
        };
        tab.preload_nearby_diffs();
        Ok(tab)
    }

    pub fn set_head(&mut self, head: &Head) -> Result<()> {
        self.head = head.clone();
        self.is_current_head = self.head == new_commander().get_current_head()?;

        self.refresh_files()?;
        self.file = self
            .files_output
            .as_ref()
            .ok()
            .and_then(|files_output| files_output.first())
            .map(|file| file.to_owned());
        self.invalidate_diff_cache();
        self.load_file_diff();
        self.preload_nearby_diffs();

        Ok(())
    }

    pub fn get_current_file_index(&self) -> Option<usize> {
        get_current_file_index(self.file.as_ref(), self.files_output.as_ref())
    }

    pub fn refresh_files(&mut self) -> Result<()> {
        self.files_output = new_commander().get_files(&self.head);
        self.conflicts_output = new_commander().get_conflicts(&self.head.commit_id)?;
        Ok(())
    }

    fn diff_key_for(&self, file: &File) -> DiffKey {
        let width = if let DiffFormat::DiffTool(_) = &self.diff_format {
            self.diff_panel.columns() as usize
        } else {
            0
        };
        (file.path.clone(), self.diff_format.clone(), width)
    }

    fn invalidate_diff_cache(&mut self) {
        self.diff_generation += 1;
        self.diff_inflight.clear();
        self.diff_cache.clear();
    }

    /// Spawn a background diff load for `file` unless it is already cached or in flight.
    fn spawn_diff_for(&mut self, file: File) {
        let key = self.diff_key_for(&file);
        if self.diff_cache.contains_key(&key) || self.diff_inflight.contains(&key) {
            return;
        }
        self.diff_inflight.insert(key.clone());

        let tx = self.diff_tx.clone();
        let head = self.head.clone();
        let diff_format = self.diff_format.clone();
        let inner_width = key.2;
        let generation = self.diff_generation;

        thread::spawn(move || {
            let mut commander = new_commander();
            commander.limit_width(inner_width);
            let result = commander
                .get_file_diff(&head, &file, &diff_format, true)
                .map(|diff| diff.map(|d| tabs_to_spaces(&d)));
            let _ = tx.send((generation, key, result));
        });
    }

    /// Check the cache and display immediately, or start a background load.
    fn load_file_diff(&mut self) {
        let Some(file) = self.file.clone() else {
            return;
        };
        let key = self.diff_key_for(&file);

        if let Some(cached) = self.diff_cache.get(&key) {
            self.diff_output = Ok(Some(cached.clone()));
            self.diff_panel.scroll_to(0);
            return;
        }

        self.diff_panel.scroll_to(0);
        self.spawn_diff_for(file);
    }

    /// Kick off background loads for up to PRELOAD_AHEAD files after the current
    /// selection. Runs them sequentially in one thread so we don't saturate the
    /// system, and stops early when the generation is invalidated.
    fn preload_nearby_diffs(&mut self) {
        const PRELOAD_AHEAD: usize = 5;

        let start = self.get_current_file_index().map_or(0, |i| i + 1);
        let to_preload: Vec<(DiffKey, File)> = match self.files_output.as_ref() {
            Ok(files) => files[start..]
                .iter()
                .take(PRELOAD_AHEAD)
                .filter_map(|f| {
                    let key = self.diff_key_for(f);
                    if self.diff_cache.contains_key(&key) || self.diff_inflight.contains(&key) {
                        None
                    } else {
                        Some((key, f.clone()))
                    }
                })
                .collect(),
            Err(_) => return,
        };

        if to_preload.is_empty() {
            return;
        }

        for (key, _) in &to_preload {
            self.diff_inflight.insert(key.clone());
        }

        let tx = self.diff_tx.clone();
        let head = self.head.clone();
        let diff_format = self.diff_format.clone();
        let generation = self.diff_generation;

        thread::spawn(move || {
            for (key, file) in to_preload {
                let mut commander = new_commander();
                commander.limit_width(key.2);
                let result = commander
                    .get_file_diff(&head, &file, &diff_format, true)
                    .map(|diff| diff.map(|d| tabs_to_spaces(&d)));
                if tx.send((generation, key, result)).is_err() {
                    break;
                }
            }
        });
    }

    pub fn untrack_file(&mut self) -> Result<()> {
        self.file
            .as_ref()
            .map(|current_file| new_commander().untrack_file(current_file))
            .transpose()?;
        Ok(())
    }

    pub fn restore_file(&mut self) -> Result<()> {
        self.file
            .as_ref()
            .map(|current_file| new_commander().restore_file(current_file))
            .transpose()?;
        Ok(())
    }

    fn scroll_files(&mut self, scroll: isize) -> Result<()> {
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
                self.load_file_diff();
                self.preload_nearby_diffs();
            }
        }
        Ok(())
    }

    pub fn has_pending_diff(&self) -> bool {
        !self.diff_inflight.is_empty()
    }
}

impl Component for FilesTab {
    fn update(&mut self) -> Result<Option<ComponentAction>> {
        while let Ok((generation, key, result)) = self.diff_rx.try_recv() {
            if generation != self.diff_generation {
                continue;
            }
            self.diff_inflight.remove(&key);
            if let Ok(Some(ref diff)) = result {
                self.diff_cache.insert(key.clone(), diff.clone());
            }
            let is_current = self
                .file
                .as_ref()
                .is_some_and(|f| self.diff_key_for(f) == key);
            if is_current {
                self.diff_output = result;
            }
        }
        Ok(None)
    }

    fn focus(&mut self) -> Result<()> {
        self.is_current_head = self.head == new_commander().get_current_head()?;
        self.head = new_commander().get_head_latest(&self.head)?;
        self.refresh_files()?;
        self.invalidate_diff_cache();
        self.load_file_diff();
        self.preload_nearby_diffs();
        Ok(())
    }

    fn draw(
        &mut self,
        f: &mut ratatui::prelude::Frame<'_>,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        let chunks = self.pane_divider.split(area, self.layout);

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
                Err(err) => err.into_text("Error getting files")?.lines,
            };

            let title_change = if self.is_current_head {
                format!("@ ({})", self.head.change_id)
            } else {
                self.head.change_id.as_string()
            };

            if !self.conflicts_output.is_empty() {
                lines.push(Line::default());

                for conflict in &self.conflicts_output {
                    lines.push(Line::raw(format!("C {}", &conflict.path)).fg(Color::Red));
                }
            }

            let files = List::new(lines)
                .block(
                    Block::bordered()
                        .title(" Files for ".to_owned() + &title_change + " ")
                        .border_type(BorderType::Rounded),
                )
                .scroll_padding(3);
            *self.files_list_state.selected_mut() = current_file_index;
            self.files_pane
                .render(f, chunks[0], files, &mut self.files_list_state);
        }

        // Draw diff
        {
            let diff_content = match self.diff_output.as_ref() {
                Ok(Some(diff_content)) => diff_content.into_text()?,
                Ok(None) => Text::default(),
                Err(err) => err.into_text("Error getting diff")?,
            };
            self.diff_panel
                .render_context::<TextContent>(diff_content)
                .title(" Diff ")
                .draw(f, chunks[1]);
        }

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                return Ok(ComponentInputResult::Handled);
            }

            let is_toggle_layout = self
                .config
                .keybinds()
                .and_then(|k| k.toggle_layout.as_ref())
                .map(|kb| kb.matches(key))
                .unwrap_or(
                    key.code == KeyCode::Char('w') && key.modifiers.contains(KeyModifiers::CONTROL),
                );
            if is_toggle_layout {
                self.layout = match self.layout {
                    JJLayout::Horizontal => JJLayout::Vertical,
                    JJLayout::Vertical => JJLayout::Horizontal,
                };
                self.pane_divider.reset();
                return Ok(ComponentInputResult::Handled);
            }

            if self.diff_panel.input(key) {
                return Ok(ComponentInputResult::Handled);
            }

            match key.code {
                KeyCode::Char('j') | KeyCode::Down => self.scroll_files(1)?,
                KeyCode::Char('k') | KeyCode::Up => self.scroll_files(-1)?,
                KeyCode::Char('J') => {
                    self.scroll_files(self.files_pane.half_page_delta())?;
                }
                KeyCode::Char('K') => {
                    self.scroll_files(self.files_pane.half_page_delta().saturating_neg())?;
                }
                KeyCode::Char('w') => {
                    self.diff_format = self.diff_format.get_next(self.config.diff_tool());
                    self.invalidate_diff_cache();
                    self.load_file_diff();
                    self.preload_nearby_diffs();
                }
                KeyCode::Char('x') => {
                    // this works even for deleted files because jj doesn't return error in that case
                    if self.untrack_file().is_err() {
                        return Ok(ComponentInputResult::HandledAction(
                            ComponentAction::SetPopup(Some(Box::new(MessagePopup::new(
                                "Can't untrack file",
                                "Make sure that file is ignored",
                            )))),
                        ));
                    }
                    self.set_head(&new_commander().get_current_head()?)?;
                }
                KeyCode::Char('r') => {
                    if let Err(err) = self.restore_file() {
                        return Ok(ComponentInputResult::HandledAction(
                            ComponentAction::SetPopup(Some(Box::new(MessagePopup::new(
                                "Can't restore file",
                                err.to_string(),
                            )))),
                        ));
                    }
                    self.set_head(&new_commander().get_current_head()?)?;
                }
                KeyCode::Char('R') | KeyCode::F(5) => {
                    self.head = new_commander().get_head_latest(&self.head)?;
                    self.refresh_files()?;
                    self.invalidate_diff_cache();
                    self.load_file_diff();
                    self.preload_nearby_diffs();
                }
                KeyCode::Char('@') => {
                    let head = &new_commander().get_current_head()?;
                    self.set_head(head)?;
                }
                KeyCode::Char('?') => {
                    return Ok(ComponentInputResult::HandledAction(
                        ComponentAction::SetPopup(Some(Box::new(HelpPopup::new(
                            vec![
                                ("j/k".to_owned(), "scroll down/up".to_owned()),
                                ("J/K".to_owned(), "scroll down by ½ page".to_owned()),
                                ("x".to_owned(), "untrack file".to_owned()),
                                ("r".to_owned(), "restore file".to_owned()),
                                ("@".to_owned(), "view current change files".to_owned()),
                            ],
                            vec![
                                ("Ctrl+e/Ctrl+y".to_owned(), "scroll down/up".to_owned()),
                                (
                                    "Ctrl+d/Ctrl+u".to_owned(),
                                    "scroll down/up by ½ page".to_owned(),
                                ),
                                (
                                    "Ctrl+f/Ctrl+b".to_owned(),
                                    "scroll down/up by page".to_owned(),
                                ),
                                ("w".to_owned(), "toggle diff format".to_owned()),
                                ("W".to_owned(), "toggle wrapping".to_owned()),
                            ],
                        )))),
                    ));
                }
                _ => return Ok(ComponentInputResult::NotHandled),
            };
        }

        if let Event::Mouse(mouse) = event {
            if self.pane_divider.handle_mouse(mouse, self.layout) {
                return Ok(ComponentInputResult::Handled);
            }
            match route_mouse(mouse, &mut [&mut self.files_pane, &mut self.diff_panel]) {
                MouseInput::Scroll(delta) => self.scroll_files(delta)?,
                MouseInput::Select(index) => {
                    if let Ok(files) = self.files_output.as_ref()
                        && let Some(file) = files.get(index).cloned()
                    {
                        self.file = Some(file);
                        self.load_file_diff();
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
