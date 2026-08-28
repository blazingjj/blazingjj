use ansi_to_tui::IntoText;
use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::prelude::*;
use ratatui::widgets::*;
use tracing::instrument;
use tracing::warn;

use crate::app::TabId;
use crate::app::command;
use crate::app::command::Command;
use crate::app::command::NewSource;
use crate::background_tasks::BackgroundTasks;
use crate::background_tasks::TaskResult;
use crate::background_tasks::TaskSlot;
use crate::commander::CommandError;
use crate::commander::bookmarks::Bookmark;
use crate::commander::bookmarks::BookmarkLine;
use crate::commander::log::Head;
use crate::commander::new_commander;
use crate::commander::revset::Revset;
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
use crate::ui::panel::CommitShowPanel;
use crate::ui::panel::ListPane;
use crate::ui::panel::MouseInput;
use crate::ui::panel::route_mouse;
use crate::ui::utils::PaneDivider;

/// Bookmarks tab. Shows bookmarks in main panel and selected bookmark current change in details panel.
pub struct BookmarksTab {
    bookmarks_output: Result<Vec<BookmarkLine>, CommandError>,
    bookmarks_pane: ListPane,
    bookmarks_list_state: ListState,

    show_all: bool,

    bookmark: Option<BookmarkLine>,

    bookmark_panel: CommitShowPanel,

    config: JjConfig,
    keybinds: BookmarksTabKeybinds,
    details_keybinds: DetailsPanelKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

/// Whether both lines are the same bookmark, wherever each points. A
/// bookmark that has moved is still the same one, which is how the
/// selection follows it.
fn is_same_bookmark(current: &BookmarkLine, listed: &BookmarkLine) -> bool {
    match (current, listed) {
        (
            BookmarkLine::Parsed {
                bookmark: current, ..
            },
            BookmarkLine::Parsed { bookmark, .. },
        ) => current.name == bookmark.name && current.remote == bookmark.remote,
        (BookmarkLine::Unparsable(current), BookmarkLine::Unparsable(listed)) => current == listed,
        _ => false,
    }
}

/// Whether both lines are the same bookmark on the same change. A
/// bookmark with more than one target is listed once per target, and
/// only this tells those lines apart.
fn is_same_target(current: &BookmarkLine, listed: &BookmarkLine) -> bool {
    let (
        BookmarkLine::Parsed {
            head: current_head, ..
        },
        BookmarkLine::Parsed { head, .. },
    ) = (current, listed)
    else {
        return is_same_bookmark(current, listed);
    };

    is_same_bookmark(current, listed) && current_head == head
}

fn get_current_bookmark_index(
    current_bookmark: Option<&BookmarkLine>,
    bookmarks_output: &Result<Vec<BookmarkLine>, CommandError>,
) -> Option<usize> {
    let Ok(bookmarks_output) = bookmarks_output else {
        return None;
    };
    let current_bookmark = current_bookmark?;

    bookmarks_output
        .iter()
        .position(|listed| is_same_target(current_bookmark, listed))
        .or_else(|| {
            bookmarks_output
                .iter()
                .position(|listed| is_same_bookmark(current_bookmark, listed))
        })
}

impl BookmarksTab {
    /// A stale tab holding no bookmarks yet.
    #[instrument(level = "info", name = "Initializing bookmarks tab", parent = None, skip(background_tasks))]
    pub fn new(background_tasks: BackgroundTasks) -> Self {
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

            bookmark_panel: CommitShowPanel::new(TabId::Bookmarks, background_tasks),

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

        // Take the selection over to the list we have just read, so that a
        // bookmark that has moved shows the change it points at now. It
        // may also be gone, deleted from here or from anywhere else, so
        // fall back to the first one.
        let selected = self.get_current_bookmark_index().unwrap_or(0);
        self.bookmark = self
            .bookmarks_output
            .as_ref()
            .ok()
            .and_then(|bookmarks| bookmarks.get(selected))
            .map(|bookmark| bookmark.to_owned());

        // Every listed bookmark is one we may come to show
        let heads = self
            .bookmarks_output
            .iter()
            .flatten()
            .filter_map(|line| match line {
                BookmarkLine::Parsed { head, .. } => Some(head.clone()),
                BookmarkLine::Unparsable(_) => None,
            })
            .collect();
        self.bookmark_panel.set_active(heads);

        self.show_bookmark();
    }

    /// Read the bookmarks afresh and move the selection to the one of
    /// this name, which may have only just come into being. Leaves the
    /// selection where it is when there is no such bookmark.
    pub fn select_bookmark(&mut self, name: &str) {
        self.refresh_bookmarks();

        let found = self.bookmarks_output.iter().flatten().find(
            |line| matches!(line, BookmarkLine::Parsed { bookmark, .. } if bookmark.name == name),
        );
        let Some(found) = found.cloned() else {
            // The bookmark was just created or renamed, so a list without
            // it is one we failed to read rather than one it is missing
            // from.
            warn!("The {name} bookmark is not in the list we just read");
            return;
        };

        self.bookmark = Some(found);
        self.show_bookmark();
    }

    /// Have the details panel show the change the selected bookmark
    /// points at.
    fn show_bookmark(&mut self) {
        let (head, title) = match self.bookmark.as_ref() {
            Some(BookmarkLine::Parsed { bookmark, head, .. }) => {
                (Some(head.clone()), format!(" Bookmark {bookmark} "))
            }
            _ => (None, " Bookmark ".to_owned()),
        };
        self.bookmark_panel.show(head, title);
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
            self.show_bookmark();
        }
    }

    /// The bookmark the selection is on, whether or not it is still
    /// there: forgetting is for one that is not.
    fn listed_bookmark(&self) -> Option<&Bookmark> {
        match self.bookmark.as_ref() {
            Some(BookmarkLine::Parsed { bookmark, .. }) => Some(bookmark),
            _ => None,
        }
    }

    /// The bookmark the operations would name and the change the line it
    /// is on points at, if the selection is on one there to be operated
    /// on. A bookmark with more than one target is listed once per
    /// target, so the line says which of them an operation acts on.
    fn selected_target(&self) -> Option<(&Bookmark, &Head)> {
        match self.bookmark.as_ref() {
            Some(BookmarkLine::Parsed { bookmark, head, .. }) if bookmark.present => {
                Some((bookmark, head))
            }
            _ => None,
        }
    }

    /// The bookmark the operations would name, if the selection is on one
    /// that is there to be operated on.
    fn selected_bookmark(&self) -> Option<&Bookmark> {
        self.selected_target().map(|(bookmark, _)| bookmark)
    }

    /// The selected bookmark when it is one on a remote, which is the
    /// only kind tracking applies to.
    fn remote_bookmark(&self) -> Option<&Bookmark> {
        self.selected_bookmark()
            .filter(|bookmark| bookmark.remote.is_some())
    }

    fn handle_event(&mut self, event: BookmarksTabEvent) -> Result<Option<AppAction>> {
        match event {
            BookmarksTabEvent::ToggleShowAll => {
                self.show_all = !self.show_all;
                self.refresh_bookmarks();
            }
            BookmarksTabEvent::CreateBookmark => {
                return Ok(Some(AppAction::SetPopup(Box::new(
                    BookmarkNamePopup::new_create(),
                ))));
            }
            BookmarksTabEvent::RenameBookmark => {
                if let Some(bookmark) = self.listed_bookmark() {
                    let old_name = bookmark.name.clone();
                    return Ok(Some(AppAction::SetPopup(Box::new(
                        BookmarkNamePopup::new_rename(old_name),
                    ))));
                }
            }
            BookmarksTabEvent::DeleteBookmark => {
                if let Some(bookmark) = self.selected_bookmark() {
                    return Ok(Some(command::ask_delete_bookmark(
                        self.config.clone(),
                        &bookmark.name,
                    )));
                }
            }
            BookmarksTabEvent::ForgetBookmark => {
                if let Some(bookmark) = self.listed_bookmark() {
                    return Ok(Some(command::ask_forget_bookmark(
                        self.config.clone(),
                        &bookmark.name,
                    )));
                }
            }
            // TODO: Ask for confirmation?
            BookmarksTabEvent::TrackBookmark => {
                if let Some(bookmark) = self.remote_bookmark() {
                    return Ok(Some(AppAction::Run(Command::TrackBookmark(
                        bookmark.clone(),
                    ))));
                }
            }
            BookmarksTabEvent::UntrackBookmark => {
                if let Some(bookmark) = self.remote_bookmark() {
                    return Ok(Some(AppAction::Run(Command::UntrackBookmark(
                        bookmark.clone(),
                    ))));
                }
            }
            BookmarksTabEvent::SetBookmark => {
                if let Some((bookmark, head)) = self.selected_target() {
                    return Ok(Some(command::ask_set_bookmark(
                        self.config.clone(),
                        bookmark,
                        head,
                    )));
                }
            }
            BookmarksTabEvent::NewChange { describe } => {
                if let Some((bookmark, head)) = self.selected_target() {
                    return Ok(Some(command::ask_new_change(
                        self.config.clone(),
                        Revset::from(&head.commit_id),
                        NewSource::Change,
                        &bookmark.to_string(),
                        describe,
                    )));
                }
            }
            BookmarksTabEvent::EditChange { ignore_immutable } => {
                if let Some((bookmark, head)) = self.selected_target() {
                    return Ok(Some(command::ask_edit(
                        self.config.clone(),
                        head,
                        format!("Bookmark: {bookmark}"),
                        ignore_immutable,
                    )));
                }
            }
            BookmarksTabEvent::ViewInLog => {
                if let Some((_, head)) = self.selected_target() {
                    return Ok(Some(AppAction::ViewLog(head.clone())));
                }
            }
            // Not an operation of its own; the key handler deals with it.
            BookmarksTabEvent::Unbound => {}
        }

        Ok(None)
    }
}

impl Tab for BookmarksTab {
    fn refresh(&mut self) -> Result<()> {
        self.refresh_bookmarks();
        self.stale = false;

        Ok(())
    }

    fn drop_caches(&mut self) {
        self.bookmark_panel.mark_dirty();
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
        self.bookmark_panel.update();

        Ok(None)
    }

    fn task_done(&mut self, result: TaskResult) -> Result<Option<AppAction>> {
        if let TaskSlot::CommitShow(_, request) = result.slot {
            self.bookmark_panel.task_done(request, result.output);
        }
        Ok(None)
    }

    fn is_waiting(&self) -> bool {
        self.bookmark_panel.is_waiting()
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
        self.bookmark_panel.draw(f, chunks[1]);

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
                    self.bookmark_panel.handle_event(ev);
                    return Ok(ComponentInputResult::Handled);
                }
            }

            return match self.keybinds.match_event(key) {
                // Not the tab's to act on, so whoever else wants the key
                // is welcome to it.
                BookmarksTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event)?.into()),
            };
        }

        if let Event::Mouse(mouse) = event {
            if self.pane_divider.handle_mouse(mouse, self.config.layout()) {
                return Ok(ComponentInputResult::Handled);
            }
            match route_mouse(
                mouse,
                &mut [&mut self.bookmarks_pane, &mut self.bookmark_panel],
            ) {
                MouseInput::Scroll(delta) => self.scroll_bookmarks(delta),
                MouseInput::Select(index) => {
                    let bookmarks = self.bookmarks_output.as_deref().unwrap_or_default();
                    if let Some(bookmark) = bookmarks.get(index).cloned() {
                        self.bookmark = Some(bookmark);
                        self.show_bookmark();
                    }
                }
                MouseInput::Context(_) => {}
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
    use crate::commander::ids::ChangeId;
    use crate::commander::ids::CommitId;
    use crate::commander::log::Head;

    /// A listed bookmark of this name pointing at this change.
    fn line(name: &str, commit_id: &str) -> BookmarkLine {
        BookmarkLine::Parsed {
            text: format!("{name}: {commit_id}"),
            bookmark: Bookmark {
                name: name.to_owned(),
                remote: None,
                present: true,
            },
            head: Head {
                change_id: ChangeId(commit_id.to_owned()),
                commit_id: CommitId(commit_id.to_owned()),
                divergent: false,
                immutable: false,
            },
        }
    }

    #[test]
    fn the_selection_stays_on_the_target_it_was_reading() {
        let listed = Ok(vec![line("bm", "one"), line("bm", "two")]);

        assert_eq!(
            get_current_bookmark_index(Some(&line("bm", "two")), &listed),
            Some(1)
        );
    }

    #[test]
    fn the_selection_follows_a_bookmark_that_has_moved() {
        let listed = Ok(vec![line("other", "elsewhere"), line("bm", "moved")]);

        assert_eq!(
            get_current_bookmark_index(Some(&line("bm", "was")), &listed),
            Some(1)
        );
    }

    #[test]
    fn a_bookmark_that_is_gone_leaves_the_selection_to_start_over() {
        let listed = Ok(vec![line("other", "elsewhere")]);

        assert_eq!(
            get_current_bookmark_index(Some(&line("bm", "was")), &listed),
            None
        );
    }
}
