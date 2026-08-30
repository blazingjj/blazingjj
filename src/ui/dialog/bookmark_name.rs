use ansi_to_tui::IntoText;
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
use crate::ui::utils::centered_rect_line_height;

pub enum BookmarkNameMode {
    Create,
    Rename { old_name: String },
}

pub struct BookmarkNamePopup<'a> {
    mode: BookmarkNameMode,
    textarea: TextArea<'a>,
    error: Option<anyhow::Error>,
    keybinds: PopupKeybinds,
}

impl BookmarkNamePopup<'_> {
    pub fn new_create() -> BookmarkNamePopup<'static> {
        Self::named(BookmarkNameMode::Create, String::new())
    }

    pub fn new_rename(old_name: String) -> BookmarkNamePopup<'static> {
        Self::named(
            BookmarkNameMode::Rename {
                old_name: old_name.clone(),
            },
            old_name,
        )
    }

    /// The same question again, with the name that was refused and what
    /// was said about it.
    pub fn refused(
        mode: BookmarkNameMode,
        name: String,
        err: impl Into<anyhow::Error>,
    ) -> BookmarkNamePopup<'static> {
        BookmarkNamePopup {
            error: Some(err.into()),
            ..Self::named(mode, name)
        }
    }

    fn named(mode: BookmarkNameMode, name: String) -> BookmarkNamePopup<'static> {
        let mut textarea = TextArea::new(vec![name]);
        textarea.move_cursor(CursorMove::End);
        BookmarkNamePopup {
            mode,
            textarea,
            error: None,
            keybinds: PopupKeybinds::text_line(),
        }
    }

    fn title(&self) -> &str {
        match &self.mode {
            BookmarkNameMode::Create => " Create bookmark ",
            BookmarkNameMode::Rename { .. } => " Rename bookmark ",
        }
    }

    /// The operation the name that has been typed asks for.
    fn command(&self, name: String) -> Command {
        match &self.mode {
            BookmarkNameMode::Create => Command::CreateBookmark(name),
            BookmarkNameMode::Rename { old_name } => Command::RenameBookmark {
                old_name: old_name.clone(),
                new_name: name,
            },
        }
    }
}

impl Component for BookmarkNamePopup<'_> {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = Block::bordered()
            .title(Span::styled(self.title(), Style::new().bold().cyan()))
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green));

        let error_lines = self
            .error
            .as_ref()
            .map(|e| e.to_string().into_text().unwrap().lines);
        let error_height = error_lines.as_ref().map_or(0, |l| l.len() + 1);

        let area = centered_rect_line_height(area, 30, 5 + error_height as u16);
        f.render_widget(Clear, area);
        f.render_widget(&block, area);

        let popup_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(error_height as u16),
                Constraint::Length(2),
            ])
            .split(block.inner(area));

        f.render_widget(&self.textarea, popup_chunks[0]);

        if let Some(error_lines) = error_lines {
            f.render_widget(
                Paragraph::new(error_lines).block(
                    Block::default()
                        .borders(Borders::TOP)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::DarkGray)),
                ),
                popup_chunks[1],
            );
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
                    let name = self.textarea.lines().join("\n");
                    if name.trim().is_empty() {
                        self.error = Some(anyhow::anyhow!("Bookmark name cannot be empty"));
                        return Ok(ComponentInputResult::Handled);
                    }
                    return Ok(ComponentInputResult::HandledAction(AppAction::Multiple(
                        vec![AppAction::ClosePopup, AppAction::Run(self.command(name))],
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
