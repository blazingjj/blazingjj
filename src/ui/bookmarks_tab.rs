#![expect(clippy::borrow_interior_mutable_const)]

use ansi_to_tui::IntoText;
use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::prelude::*;
use ratatui::widgets::*;
use tracing::instrument;
use tui_confirm_dialog::ButtonLabel;
use tui_confirm_dialog::ConfirmDialog;
use tui_confirm_dialog::ConfirmDialogState;
use tui_confirm_dialog::Listener;

use crate::commander::CommandError;
use crate::commander::bookmarks::BookmarkLine;
use crate::commander::new_commander;
use crate::commander::revset::Revset;
use crate::env::DiffFormat;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::keybinds::BookmarksTabEvent;
use crate::keybinds::BookmarksTabKeybinds;
use crate::keybinds::DetailsPanelEvent;
use crate::keybinds::DetailsPanelKeybinds;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::BookmarkNamePopup;
use crate::ui::dialog::DescribePopup;
use crate::ui::dialog::MessagePopup;
use crate::ui::panel::DetailsPanel;
use crate::ui::panel::ListPane;
use crate::ui::panel::TextContent;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::tabs_to_spaces;

struct DeleteBookmark {
    name: String,
}

struct ForgetBookmark {
    name: String,
}

const DELETE_BRANCH_POPUP_ID: u16 = 1;
const FORGET_BRANCH_POPUP_ID: u16 = 2;
const NEW_POPUP_ID: u16 = 3;
const EDIT_POPUP_ID: u16 = 4;

/// Bookmarks tab. Shows bookmarks in main panel and selected bookmark current change in details panel.
pub struct BookmarksTab {
    bookmarks_output: Result<Vec<BookmarkLine>, CommandError>,
    bookmarks_pane: ListPane,
    bookmarks_list_state: ListState,

    show_all: bool,

    bookmark: Option<BookmarkLine>,

    bookmark_panel: DetailsPanel,
    bookmark_output: Option<Result<String, CommandError>>,

    delete: Option<DeleteBookmark>,
    forget: Option<ForgetBookmark>,

    describe_after_new: bool,

    edit_ignore_immutable: bool,

    popup: ConfirmDialogState,
    popup_tx: std::sync::mpsc::Sender<Listener>,
    popup_rx: std::sync::mpsc::Receiver<Listener>,

    bookmark_name_popup_tx: std::sync::mpsc::Sender<String>,
    bookmark_name_popup_rx: std::sync::mpsc::Receiver<String>,

    diff_format: DiffFormat,

    config: JjConfig,
    keybinds: BookmarksTabKeybinds,
    details_keybinds: DetailsPanelKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

fn get_current_bookmark_index(
    current_bookmark: Option<&BookmarkLine>,
    bookmarks_output: &Result<Vec<BookmarkLine>, CommandError>,
) -> Option<usize> {
    match bookmarks_output {
        Ok(bookmarks_output) => current_bookmark.as_ref().and_then(|current_bookmark| {
            bookmarks_output
                .iter()
                .position(|bookmark| match (current_bookmark, bookmark) {
                    (
                        BookmarkLine::Parsed {
                            bookmark: current_bookmark,
                            ..
                        },
                        BookmarkLine::Parsed { bookmark, .. },
                    ) => {
                        current_bookmark.name == bookmark.name
                            && current_bookmark.remote == bookmark.remote
                    }
                    (
                        BookmarkLine::Unparsable(current_bookmark),
                        BookmarkLine::Unparsable(bookmark),
                    ) => current_bookmark == bookmark,
                    _ => false,
                })
        }),
        Err(_) => None,
    }
}

impl BookmarksTab {
    /// A stale tab holding no bookmarks yet.
    #[instrument(level = "info", name = "Initializing bookmarks tab", parent = None, skip())]
    pub fn new() -> Self {
        let (popup_tx, popup_rx) = std::sync::mpsc::channel();
        let (bookmark_name_popup_tx, bookmark_name_popup_rx) = std::sync::mpsc::channel();

        let config = get_env().jj_config.clone();
        let pane_divider = PaneDivider::new(config.layout_percent());
        let keybinds = BookmarksTabKeybinds::default();
        let details_keybinds = DetailsPanelKeybinds::default();

        Self {
            bookmarks_output: Ok(Vec::new()),
            bookmark: None,
            bookmarks_pane: ListPane::default(),
            bookmarks_list_state: ListState::default(),

            show_all: false,

            bookmark_panel: DetailsPanel::new(),
            bookmark_output: None,

            delete: None,
            forget: None,

            describe_after_new: false,

            edit_ignore_immutable: false,

            popup: ConfirmDialogState::default(),
            popup_tx,
            popup_rx,

            bookmark_name_popup_tx,
            bookmark_name_popup_rx,

            diff_format: get_env().jj_config.diff_format(),

            config,
            keybinds,
            details_keybinds,
            pane_divider,

            stale: true,
        }
    }

    pub fn get_current_bookmark_index(&self) -> Option<usize> {
        get_current_bookmark_index(self.bookmark.as_ref(), &self.bookmarks_output)
    }

    pub fn refresh_bookmarks(&mut self) {
        self.bookmarks_output = new_commander().get_bookmarks(self.show_all);

        // The selected bookmark may be gone, deleted from here or from
        // anywhere else, so fall back to the first one.
        if self.get_current_bookmark_index().is_none() {
            self.bookmark = self
                .bookmarks_output
                .as_ref()
                .ok()
                .and_then(|bookmarks| bookmarks.first())
                .map(|bookmark| bookmark.to_owned());
        }
    }

    pub fn refresh_bookmark(&mut self) {
        let mut commander = new_commander();
        let inner_width = self.bookmark_panel.columns() as usize;
        commander.limit_width(inner_width);
        self.bookmark_output = self.bookmark.as_ref().and_then(|bookmark| match bookmark {
            BookmarkLine::Parsed { bookmark, .. } => Some(
                commander
                    .get_bookmark_show(bookmark, &self.diff_format, true)
                    .map(|diff| tabs_to_spaces(&diff)),
            ),
            _ => None,
        });

        self.bookmark_panel.scroll_to(0);
    }

    fn scroll_bookmarks(&mut self, scroll: isize) {
        let bookmarks = Vec::new();
        let bookmarks = self.bookmarks_output.as_ref().unwrap_or(&bookmarks);
        let current_bookmark_index = self.get_current_bookmark_index();
        let next_bookmark = match current_bookmark_index {
            Some(current_bookmark_index) => bookmarks.get(
                current_bookmark_index
                    .saturating_add_signed(scroll)
                    .min(bookmarks.len() - 1),
            ),
            None => bookmarks.first(),
        }
        .map(|x| x.to_owned());

        if let Some(next_bookmark) = next_bookmark {
            self.bookmark = Some(next_bookmark);
            self.refresh_bookmark();
        }
    }
}

impl Tab for BookmarksTab {
    fn refresh(&mut self) -> Result<()> {
        self.refresh_bookmarks();
        self.refresh_bookmark();
        self.stale = false;

        Ok(())
    }

    fn mark_stale(&mut self) {
        self.stale = true;
    }

    fn is_stale(&self) -> bool {
        self.stale
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        let half_page = self.bookmarks_pane.visible_items() / 2;
        self.scroll_bookmarks(match scroll {
            Scroll::Down => 1,
            Scroll::Up => -1,
            Scroll::DownHalfPage => half_page,
            Scroll::UpHalfPage => half_page.saturating_neg(),
        });
        Ok(())
    }

    fn make_main_panel_help(&self) -> Vec<(String, String)> {
        self.keybinds.make_help()
    }

    fn make_details_panel_help(&self) -> Vec<(String, String)> {
        self.details_keybinds.make_help()
    }
}

impl Component for BookmarksTab {
    fn update(&mut self) -> Result<Option<AppAction>> {
        // Check for popup action
        if let Ok(res) = self.popup_rx.try_recv()
            && res.1.unwrap_or(false)
        {
            match res.0 {
                DELETE_BRANCH_POPUP_ID => {
                    if let Some(delete) = self.delete.as_ref() {
                        match new_commander().delete_bookmark(&delete.name) {
                            Ok(_) => return Ok(Some(AppAction::RefreshTab)),
                            Err(err) => {
                                return Ok(Some(AppAction::SetPopup(Box::new(MessagePopup::new(
                                    "Delete error",
                                    err.to_string(),
                                )))));
                            }
                        }
                    }
                }
                FORGET_BRANCH_POPUP_ID => {
                    if let Some(forget) = self.forget.as_ref() {
                        match new_commander().forget_bookmark(&forget.name) {
                            Ok(_) => return Ok(Some(AppAction::RefreshTab)),
                            Err(err) => {
                                return Ok(Some(AppAction::SetPopup(Box::new(MessagePopup::new(
                                    "Forget error",
                                    err.to_string(),
                                )))));
                            }
                        }
                    }
                }
                NEW_POPUP_ID => {
                    if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref() {
                        new_commander().run_new(Revset::expression(bookmark.to_string()))?;
                        let head = new_commander().get_current_head()?;
                        if self.describe_after_new {
                            self.describe_after_new = false;
                            return Ok(Some(AppAction::SetPopup(Box::new(DescribePopup::new(
                                head,
                                vec![],
                            )))));
                        } else {
                            return Ok(Some(AppAction::ViewLog(head)));
                        }
                    }
                }
                EDIT_POPUP_ID => {
                    if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref() {
                        new_commander().run_edit(
                            Revset::expression(bookmark.to_string()),
                            self.edit_ignore_immutable,
                        )?;
                        let head = new_commander().get_current_head()?;
                        return Ok(Some(AppAction::ViewLog(head)));
                    }
                }
                _ => {}
            }
        }

        if let Ok(name) = self.bookmark_name_popup_rx.try_recv() {
            self.refresh_bookmarks();
            if let Some(bookmark) = self.bookmarks_output.as_ref().ok().and_then(|list| {
                list.iter().find(|b| match b {
                    BookmarkLine::Unparsable(_) => false,
                    BookmarkLine::Parsed { bookmark, .. } => bookmark.name == name,
                })
            }) {
                self.bookmark = Some(bookmark.clone());
            }
            self.refresh_bookmark();
        }

        Ok(None)
    }

    fn draw(
        &mut self,
        f: &mut ratatui::prelude::Frame<'_>,
        area: ratatui::prelude::Rect,
    ) -> Result<()> {
        let chunks = self.pane_divider.split(area, self.config.layout());

        // Draw bookmarks
        {
            let current_bookmark_index = self.get_current_bookmark_index();

            let bookmark_lines: Vec<Line> = match self.bookmarks_output.as_ref() {
                Ok(bookmarks_output) => bookmarks_output
                    .iter()
                    .enumerate()
                    .map(|(i, bookmark)| -> Result<Vec<Line>, ansi_to_tui::Error> {
                        let bookmark_text = bookmark.to_text()?;
                        Ok(bookmark_text
                            .iter()
                            .map(|line| {
                                let mut line = line.to_owned();

                                // Add padding at start
                                line.spans.insert(0, Span::from(" "));

                                if current_bookmark_index == Some(i) {
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
                            .collect::<Vec<Line>>())
                    })
                    .collect::<Result<Vec<Vec<Line>>, ansi_to_tui::Error>>()?
                    .into_iter()
                    .flatten()
                    .collect(),
                Err(err) => [
                    vec![Line::raw("Error getting bookmarks").bold().fg(Color::Red)],
                    // TODO: Remove when jj 0.20 is released
                    if let CommandError::Status(output, _) = err {
                        if output.contains("unexpected argument '-T' found") {
                            vec![
                                Line::raw(""),
                                Line::raw("Please update jj to >0.18 for -T support to bookmarks")
                                    .bold()
                                    .fg(Color::Red),
                            ]
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    },
                    vec![Line::raw(""), Line::raw("")],
                    err.to_string().into_text()?.lines,
                ]
                .concat(),
            };

            let lines = if bookmark_lines.is_empty() {
                vec![Line::from(" No bookmarks").fg(Color::DarkGray).italic()]
            } else {
                bookmark_lines
            };

            let block = Block::bordered()
                .title(" Bookmarks ")
                .border_type(BorderType::Rounded);
            let bookmarks = List::new(lines).scroll_padding(3);
            *self.bookmarks_list_state.selected_mut() = current_bookmark_index;
            self.bookmarks_pane.render(
                f,
                chunks[0],
                block,
                bookmarks,
                &mut self.bookmarks_list_state,
            );
        }

        // Draw bookmark
        {
            let title = if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref()
            {
                format!(" Bookmark {bookmark} ")
            } else {
                " Bookmark ".to_owned()
            };
            let bookmark_content: Vec<Line> = match self.bookmark_output.as_ref() {
                Some(Ok(bookmark_output)) => bookmark_output.into_text()?.lines,
                Some(Err(err)) => err.into_text("Error getting bookmark")?.lines,
                None => vec![],
            };
            self.bookmark_panel
                .render_context::<TextContent>(bookmark_content)
                .title(title)
                .draw(f, chunks[1]);
        }

        // Draw popup
        if self.popup.is_opened() {
            let popup = ConfirmDialog::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green))
                .selected_button_style(
                    Style::default()
                        .bg(self.config.highlight_color())
                        .underlined(),
                );
            f.render_stateful_widget(popup, area, &mut self.popup);
        }

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                return Ok(ComponentInputResult::Handled);
            }
            if self.popup.is_opened() {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    self.popup = ConfirmDialogState::default();
                } else {
                    self.popup.handle(&key);
                }

                return Ok(ComponentInputResult::Handled);
            }

            match self.details_keybinds.match_event(key) {
                DetailsPanelEvent::Unbound => {}
                DetailsPanelEvent::ToggleDiffFormat => {
                    self.diff_format = self.diff_format.get_next(self.config.diff_tool());
                    self.refresh_bookmark();
                    return Ok(ComponentInputResult::Handled);
                }
                ev => {
                    self.bookmark_panel.handle_event(ev);
                    return Ok(ComponentInputResult::Handled);
                }
            }

            match self.keybinds.match_event(key) {
                BookmarksTabEvent::ToggleShowAll => {
                    self.show_all = !self.show_all;
                    self.refresh_bookmarks();
                }
                BookmarksTabEvent::CreateBookmark => {
                    return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                        Box::new(BookmarkNamePopup::new_create(
                            self.bookmark_name_popup_tx.clone(),
                        )),
                    )));
                }
                BookmarksTabEvent::RenameBookmark => {
                    if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref() {
                        let old_name = bookmark.name.clone();
                        return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                            Box::new(BookmarkNamePopup::new_rename(
                                old_name,
                                self.bookmark_name_popup_tx.clone(),
                            )),
                        )));
                    }
                }
                BookmarksTabEvent::DeleteBookmark => {
                    if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref() {
                        self.delete = Some(DeleteBookmark {
                            name: bookmark.name.clone(),
                        });
                        self.popup = ConfirmDialogState::new(
                            DELETE_BRANCH_POPUP_ID,
                            Span::styled(" Delete ", Style::new().bold().cyan()),
                            Text::from(vec![Line::from(format!(
                                "Are you sure you want to delete the {} bookmark?",
                                bookmark.name
                            ))]),
                        );
                        self.popup
                            .with_yes_button(ButtonLabel::YES.clone())
                            .with_no_button(ButtonLabel::NO.clone())
                            .with_listener(Some(self.popup_tx.clone()))
                            .open();
                    }
                }
                BookmarksTabEvent::ForgetBookmark => {
                    if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref() {
                        self.forget = Some(ForgetBookmark {
                            name: bookmark.name.clone(),
                        });
                        self.popup = ConfirmDialogState::new(
                            FORGET_BRANCH_POPUP_ID,
                            Span::styled(" Forget ", Style::new().bold().cyan()),
                            Text::from(vec![Line::from(format!(
                                "Are you sure you want to forget the {} bookmark?",
                                bookmark.name
                            ))]),
                        );
                        self.popup
                            .with_yes_button(ButtonLabel::YES.clone())
                            .with_no_button(ButtonLabel::NO.clone())
                            .with_listener(Some(self.popup_tx.clone()))
                            .open();
                    }
                }
                // TODO: Ask for confirmation?
                BookmarksTabEvent::TrackBookmark => {
                    if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref()
                        && bookmark.remote.is_some()
                        && bookmark.present
                    {
                        new_commander().track_bookmark(bookmark)?;
                        return Ok(ComponentInputResult::HandledAction(AppAction::RefreshTab));
                    }
                }
                BookmarksTabEvent::UntrackBookmark => {
                    if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref()
                        && bookmark.remote.is_some()
                        && bookmark.present
                    {
                        new_commander().untrack_bookmark(bookmark)?;
                        return Ok(ComponentInputResult::HandledAction(AppAction::RefreshTab));
                    }
                }
                BookmarksTabEvent::NewChange { describe } => {
                    if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref()
                        && bookmark.present
                    {
                        self.popup = ConfirmDialogState::new(
                            NEW_POPUP_ID,
                            Span::styled(" New ", Style::new().bold().cyan()),
                            Text::from(vec![
                                Line::from("Are you sure you want to create a new change?"),
                                Line::from(format!("Bookmark: {bookmark}")),
                            ]),
                        );
                        self.popup
                            .with_yes_button(ButtonLabel::YES.clone())
                            .with_no_button(ButtonLabel::NO.clone())
                            .with_listener(Some(self.popup_tx.clone()))
                            .open();

                        self.describe_after_new = describe;
                    }
                }
                BookmarksTabEvent::EditChange { ignore_immutable } => {
                    if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref()
                        && bookmark.present
                    {
                        if new_commander().check_revision_immutable(&bookmark.to_string())?
                            && !ignore_immutable
                        {
                            return Ok(ComponentInputResult::HandledAction(AppAction::SetPopup(
                                Box::new(MessagePopup::new(
                                    "Edit",
                                    "The change cannot be edited because it is immutable.",
                                )),
                            )));
                        }

                        self.popup = ConfirmDialogState::new(
                            EDIT_POPUP_ID,
                            Span::styled(" Edit ", Style::new().bold().cyan()),
                            Text::from(vec![
                                Line::from("Are you sure you want to edit an existing change?"),
                                Line::from(format!("Bookmark: {bookmark}")),
                            ]),
                        );
                        self.popup
                            .with_yes_button(ButtonLabel::YES.clone())
                            .with_no_button(ButtonLabel::NO.clone())
                            .with_listener(Some(self.popup_tx.clone()))
                            .open();
                        self.edit_ignore_immutable = ignore_immutable;
                    }
                }
                BookmarksTabEvent::ViewInLog => {
                    if let Some(BookmarkLine::Parsed { bookmark, .. }) = self.bookmark.as_ref()
                        && bookmark.present
                    {
                        return Ok(ComponentInputResult::HandledAction(AppAction::ViewLog(
                            new_commander().get_bookmark_head(bookmark)?,
                        )));
                    }
                }
                BookmarksTabEvent::Unbound => return Ok(ComponentInputResult::NotHandled),
            };
        }

        if let Event::Mouse(mouse) = event {
            if self.pane_divider.handle_mouse(mouse, self.config.layout()) {
                return Ok(ComponentInputResult::Handled);
            }
            if self.bookmark_panel.input_mouse(mouse) {
                return Ok(ComponentInputResult::Handled);
            }
            return Ok(ComponentInputResult::NotHandled);
        }

        Ok(ComponentInputResult::Handled)
    }
}
