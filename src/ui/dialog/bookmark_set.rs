use std::fmt::Display;

use ansi_to_tui::IntoText;
use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::List;
use ratatui::widgets::ListState;
use ratatui::widgets::Paragraph;
use ratatui_textarea::CursorMove;
use ratatui_textarea::TextArea;

use crate::app::command::BookmarkSetDialog;
use crate::app::command::Command;
use crate::commander::bookmarks::Bookmark;
use crate::commander::ids::ChangeId;
use crate::commander::ids::CommitId;
use crate::commander::new_commander;
use crate::env::JjConfig;
use crate::keybinds::BookmarkSetPopupEvent;
use crate::keybinds::BookmarkSetPopupKeybinds;
use crate::keybinds::PopupEvent;
use crate::keybinds::PopupKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::styles::create_popup_block;
use crate::ui::styles::refusal;
use crate::ui::utils::centered_rect;
use crate::ui::utils::centered_rect_line_height;
use crate::ui::utils::mark_key;

enum BookmarkSetOption {
    CreateBookmark,
    // Name, exists
    GeneratedName(String, bool),
    Bookmark(Bookmark),
    Error(String),
}

pub struct BookmarkSetPopup<'a> {
    pub change_id: Option<ChangeId>,
    commit_id: CommitId,
    options: Vec<BookmarkSetOption>,
    list_state: ListState,
    list_height: u16,
    config: JjConfig,
    creating: Option<TextArea<'a>>,
    /// What was said about the name that was refused, shown alongside it.
    error: Option<String>,
    keybinds: PopupKeybinds,
    name_keybinds: PopupKeybinds,
    own_keybinds: BookmarkSetPopupKeybinds,
}

fn generate_options(change_id: Option<&ChangeId>, commit_id: &CommitId) -> Vec<BookmarkSetOption> {
    let bookmarks = new_commander()
        .get_bookmarks_list(false, commit_id)
        .map(|bookmarks| {
            bookmarks
                .into_iter()
                .filter(|bookmark| bookmark.remote.is_none())
                .collect::<Vec<Bookmark>>()
        });
    let mut options = vec![BookmarkSetOption::CreateBookmark];

    if let Some(change_id) = change_id {
        let generated_name = generate_name(change_id);
        let exists = bookmarks.as_ref().is_ok_and(|bookmarks| {
            bookmarks
                .iter()
                .any(|bookmark| bookmark.name == generated_name)
        });
        options.push(BookmarkSetOption::GeneratedName(generated_name, exists));
    }

    match bookmarks.as_ref() {
        Ok(bookmarks) => {
            for bookmark in bookmarks
                .iter()
                .filter(|bookmark| bookmark.remote.is_none())
            {
                options.push(BookmarkSetOption::Bookmark(bookmark.clone()))
            }
        }
        Err(err) => options.push(BookmarkSetOption::Error(err.to_string())),
    }

    options
}

fn generate_name(change_id: &ChangeId) -> String {
    new_commander()
        .generate_bookmark_name(change_id)
        .unwrap_or_else(|_| format!("error-{change_id}"))
}

impl BookmarkSetPopup<'_> {
    pub fn new(config: JjConfig, change_id: Option<ChangeId>, commit_id: CommitId) -> Self {
        Self {
            options: generate_options(change_id.as_ref(), &commit_id),
            change_id,
            list_state: ListState::default().with_selected(Some(0)),
            list_height: 0,
            config,
            commit_id,
            creating: None,
            error: None,
            keybinds: PopupKeybinds::dialog(),
            name_keybinds: PopupKeybinds::text_line(),
            own_keybinds: BookmarkSetPopupKeybinds::new(),
        }
    }

    /// The name field again with the name that was refused and what was
    /// said about it, whether it was typed or picked.
    pub fn refused(
        config: JjConfig,
        change_id: Option<ChangeId>,
        commit_id: CommitId,
        name: String,
        err: impl Display,
    ) -> Self {
        let mut creating = TextArea::new(vec![name]);
        creating.move_cursor(CursorMove::End);

        Self {
            creating: Some(creating),
            error: Some(format!("{err:#}")),
            ..Self::new(config, change_id, commit_id)
        }
    }

    fn scroll(&mut self, scroll: isize) {
        self.list_state.select(Some(
            self.list_state
                .selected()
                .map(|selected| selected.saturating_add_signed(scroll))
                .unwrap_or(0)
                .min(self.options.len().saturating_sub(1)),
        ));
    }

    /// The name typed into the field, when there is a field.
    fn name_typed(&self) -> String {
        self.creating
            .as_ref()
            .map(|creating| creating.lines().join("\n"))
            .unwrap_or_default()
    }

    fn on_creating(&mut self) {
        self.creating = Some(TextArea::default());
    }

    /// Taking the popup down and putting the bookmark of this name on
    /// the commit it was opened for.
    fn set_bookmark(&self, name: String) -> ComponentInputResult {
        ComponentInputResult::HandledAction(AppAction::Multiple(vec![
            AppAction::ClosePopup,
            AppAction::Run(Command::SetBookmark {
                name,
                commit_id: self.commit_id.clone(),
                dialog: Some(Box::new(BookmarkSetDialog {
                    config: self.config.clone(),
                    change_id: self.change_id.clone(),
                })),
            }),
        ]))
    }

    /// An option's line, marking the key that picks it.
    fn label(&self, option: &str, event: BookmarkSetPopupEvent) -> String {
        mark_key(option, self.own_keybinds.shortcut(event))
    }

    /// The name generated for the change the popup was opened for, if
    /// it was opened for one at all.
    fn generated_name(&self) -> Option<String> {
        self.options.iter().find_map(|option| match option {
            BookmarkSetOption::GeneratedName(name, _) => Some(name.clone()),
            _ => None,
        })
    }
}

impl Component for BookmarkSetPopup<'_> {
    fn draw(&mut self, f: &mut ratatui::prelude::Frame<'_>, area: Rect) -> Result<()> {
        if let Some(creating) = self.creating.as_ref() {
            let block = create_popup_block("Create bookmark");
            // The width the popup is about to get, which the answer has
            // to be wrapped to before we know how tall to make it.
            let width = block.inner(centered_rect_line_height(area, 30, 0)).width;
            let error = self.error.as_ref().map(|error| refusal(error, width));
            let error_height = error.as_ref().map_or(0, |(_, height)| *height);

            let area = centered_rect_line_height(area, 30, 5 + error_height);
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

            f.render_widget(creating, popup_chunks[0]);

            if let Some((error, _)) = error {
                f.render_widget(error, popup_chunks[1]);
            }

            let help = Paragraph::new(vec![self.name_keybinds.hint("accept").into()])
                .fg(Color::DarkGray)
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::TOP)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::DarkGray)),
                );

            f.render_widget(help, popup_chunks[2]);
        } else {
            let block = Block::bordered()
                .title(Span::styled(
                    " Select bookmark ",
                    Style::new().bold().cyan(),
                ))
                .title_alignment(Alignment::Center)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green));
            let area = centered_rect(area, 40, 60);
            f.render_widget(Clear, area);
            f.render_widget(&block, area);

            let popup_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Fill(1), Constraint::Length(2)])
                .split(block.inner(area));

            let create = self.label("Create bookmark", BookmarkSetPopupEvent::CreateBookmark);
            let generate = self.label("Generate bookmark", BookmarkSetPopupEvent::UseGeneratedName);
            let list_items = self.options.iter().map(|option| match option {
                BookmarkSetOption::CreateBookmark => Text::raw(create.clone()).fg(Color::Yellow),
                BookmarkSetOption::GeneratedName(generated_name, exists) => {
                    let mut text = format!("{generate}: {generated_name}");
                    if *exists {
                        text.push_str(" (exists)");
                    }
                    Text::raw(text).fg(Color::Yellow)
                }
                BookmarkSetOption::Bookmark(bookmark) => {
                    Text::raw(bookmark.to_string()).fg(Color::Magenta)
                }
                BookmarkSetOption::Error(err) => err.into_text().unwrap(),
            });

            let list = List::new(list_items)
                .scroll_padding(3)
                .highlight_style(Style::default().bg(self.config.highlight_color()));

            f.render_stateful_widget(list, popup_chunks[0], &mut self.list_state);
            self.list_height = popup_chunks[0].height;

            let help = Paragraph::new(vec![self.keybinds.scroll_hint("select").into()])
                .fg(Color::DarkGray)
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::TOP)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::DarkGray)),
                );

            f.render_widget(help, popup_chunks[1]);
        }

        Ok(())
    }

    /// Handle input. Returns bool of if to close
    fn input(&mut self, event: Event) -> anyhow::Result<ComponentInputResult> {
        if self.creating.is_some() {
            if let Event::Key(key) = event {
                match self.name_keybinds.match_event(key) {
                    PopupEvent::Accept => {
                        let name = self.name_typed();
                        if name.trim().is_empty() {
                            return Ok(ComponentInputResult::Handled);
                        }

                        return Ok(self.set_bookmark(name));
                    }
                    PopupEvent::Cancel => {
                        return Ok(ComponentInputResult::HandledAction(AppAction::ClosePopup));
                    }
                    _ => {}
                }
            }

            if let Some(creating) = self.creating.as_mut() {
                creating.input(event);
            }
            return Ok(ComponentInputResult::Handled);
        }

        if let Event::Key(key) = event {
            match self.keybinds.match_event(key) {
                PopupEvent::ScrollDown => {
                    self.scroll(1);
                }
                PopupEvent::ScrollUp => {
                    self.scroll(-1);
                }
                PopupEvent::ScrollDownHalf => {
                    self.scroll(self.list_height as isize / 2);
                }
                PopupEvent::ScrollUpHalf => {
                    self.scroll((self.list_height as isize / 2).saturating_neg());
                }
                PopupEvent::ScrollDownPage => {
                    self.scroll(self.list_height as isize);
                }
                PopupEvent::ScrollUpPage => {
                    self.scroll((self.list_height as isize).saturating_neg());
                }
                PopupEvent::Accept => {
                    if let Some(action) = self
                        .list_state
                        .selected()
                        .and_then(|index| self.options.get(index))
                    {
                        match action {
                            BookmarkSetOption::CreateBookmark => {
                                self.on_creating();
                            }
                            BookmarkSetOption::GeneratedName(name, _) => {
                                return Ok(self.set_bookmark(name.clone()));
                            }
                            BookmarkSetOption::Bookmark(bookmark) => {
                                return Ok(self.set_bookmark(bookmark.name.clone()));
                            }
                            BookmarkSetOption::Error(_) => {
                                self.options =
                                    generate_options(self.change_id.as_ref(), &self.commit_id);
                            }
                        }
                    }
                }
                PopupEvent::Cancel => {
                    return Ok(ComponentInputResult::HandledAction(AppAction::ClosePopup));
                }
                // The options carry the letter they are picked by, so
                // those keys are the popup's own.
                PopupEvent::Unbound => match self.own_keybinds.match_event(key) {
                    BookmarkSetPopupEvent::UseGeneratedName => {
                        if let Some(name) = self.generated_name() {
                            return Ok(self.set_bookmark(name));
                        }
                    }
                    BookmarkSetPopupEvent::CreateBookmark => self.on_creating(),
                    BookmarkSetPopupEvent::Unbound => {
                        return Ok(ComponentInputResult::NotHandled);
                    }
                },
            }

            return Ok(ComponentInputResult::Handled);
        }

        Ok(ComponentInputResult::NotHandled)
    }
}
