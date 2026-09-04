/*! A popup taking the one thing adding or renaming a workspace needs
typed: where the new workspace goes, or what the selected one is to be
called.
*/

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::prelude::Stylize;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui_textarea::CursorMove;
use ratatui_textarea::TextArea;

use crate::app::command::Command;
use crate::keybinds::PopupEvent;
use crate::keybinds::PopupKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::styles::refusal;
use crate::ui::utils::centered_rect_line_height;

/// What the popup is asking for.
pub enum WorkspaceMode {
    /// Where to make a workspace, which jj names after the directory.
    Add,
    /// What to call the workspace at this root.
    Rename { root: String },
}

pub struct WorkspacePopup<'a> {
    mode: WorkspaceMode,
    textarea: TextArea<'a>,
    /// What was said about what was typed, if it was refused.
    error: Option<anyhow::Error>,
    keybinds: PopupKeybinds,
}

impl WorkspacePopup<'_> {
    /// Ask where to make a workspace.
    pub fn new_add() -> WorkspacePopup<'static> {
        Self::typed_into(WorkspaceMode::Add, String::new())
    }

    /// Ask what to call the workspace at `root`, which is called `name`
    /// now.
    pub fn new_rename(root: String, name: String) -> WorkspacePopup<'static> {
        Self::typed_into(WorkspaceMode::Rename { root }, name)
    }

    /// The same question again, with what was refused and what was said
    /// about it.
    pub fn refused(
        mode: WorkspaceMode,
        typed: String,
        err: impl Into<anyhow::Error>,
    ) -> WorkspacePopup<'static> {
        WorkspacePopup {
            error: Some(err.into()),
            ..Self::typed_into(mode, typed)
        }
    }

    fn typed_into(mode: WorkspaceMode, typed: String) -> WorkspacePopup<'static> {
        let mut textarea = TextArea::new(vec![typed]);
        textarea.move_cursor(CursorMove::End);

        WorkspacePopup {
            mode,
            textarea,
            error: None,
            keybinds: PopupKeybinds::text_line(),
        }
    }

    fn title(&self) -> &'static str {
        match self.mode {
            WorkspaceMode::Add => " Add workspace: directory to make it in ",
            WorkspaceMode::Rename { .. } => " Rename workspace ",
        }
    }

    /// What is missing when nothing has been typed.
    fn missing(&self) -> &'static str {
        match self.mode {
            WorkspaceMode::Add => "A workspace needs a directory to be made in",
            WorkspaceMode::Rename { .. } => "A workspace needs a name",
        }
    }

    /// The operation what has been typed asks for.
    fn command(&self, typed: String) -> Command {
        match &self.mode {
            WorkspaceMode::Add => Command::AddWorkspace { destination: typed },
            WorkspaceMode::Rename { root } => Command::RenameWorkspace {
                root: root.clone(),
                new_name: typed,
            },
        }
    }
}

impl Component for WorkspacePopup<'_> {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = Block::bordered()
            .title(Span::styled(self.title(), Style::new().bold().cyan()))
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green));

        // The width the popup is about to get, which the answer has to
        // be wrapped to before we know how tall to make it.
        let width = block.inner(centered_rect_line_height(area, 50, 0)).width;
        let error = self
            .error
            .as_ref()
            .map(|error| refusal(&format!("{error:#}"), width));
        let error_height = error.as_ref().map_or(0, |(_, height)| *height);

        let area = centered_rect_line_height(area, 50, 5 + error_height);
        f.render_widget(Clear, area);
        f.render_widget(&block, area);

        let popup_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(error_height),
                Constraint::Length(2),
            ])
            .split(block.inner(area));

        f.render_widget(&self.textarea, popup_chunks[0]);

        if let Some((error, _)) = error {
            f.render_widget(error, popup_chunks[1]);
        }

        f.render_widget(
            Paragraph::new(vec![self.keybinds.hint("accept").into()])
                .fg(Color::DarkGray)
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::TOP)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::DarkGray)),
                ),
            popup_chunks[2],
        );

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match self.keybinds.match_event(key) {
                PopupEvent::Accept => {
                    let typed = self.textarea.lines().join("\n").trim().to_owned();
                    if typed.is_empty() {
                        self.error = Some(anyhow::anyhow!(self.missing()));
                        return Ok(ComponentInputResult::Handled);
                    }

                    return Ok(ComponentInputResult::HandledAction(AppAction::Multiple(
                        vec![AppAction::ClosePopup, AppAction::Run(self.command(typed))],
                    )));
                }
                PopupEvent::Cancel => {
                    return Ok(ComponentInputResult::HandledAction(AppAction::ClosePopup));
                }
                _ => {}
            }
        }

        self.textarea.input(event);
        Ok(ComponentInputResult::Handled)
    }
}
