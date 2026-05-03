use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::List;
use ratatui::widgets::ListState;
use ratatui::widgets::Paragraph;

use crate::commander::log::Parent;
use crate::env::JjConfig;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::styles::create_popup_block;
use crate::ui::utils::centered_rect;

/// The row a parent gets in the list, dimmed when it cannot be selected.
fn parent_line(parent: &Parent, dimmed: bool) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    let (change_id_style, description_style) = if dimmed {
        (dim, dim)
    } else {
        (Style::default().fg(Color::Magenta), Style::default())
    };

    let change_id: String = parent.head.change_id.as_str().chars().take(8).collect();
    let description = if parent.description.is_empty() {
        Span::styled(" (no description)", dim)
    } else {
        Span::styled(format!(" {}", parent.description), description_style)
    };

    Line::from(vec![Span::styled(change_id, change_id_style), description])
}

pub struct ParentSelectPopup {
    parents: Vec<Parent>,
    /// The parents the log does not hold. We list them so that the merge
    /// is shown in full, but they cannot be selected.
    out_of_view: Vec<Parent>,
    list_state: ListState,
    list_height: u16,
    config: JjConfig,
}

impl ParentSelectPopup {
    pub fn new(parents: Vec<Parent>, out_of_view: Vec<Parent>, config: JjConfig) -> Self {
        Self {
            parents,
            out_of_view,
            list_state: ListState::default().with_selected(Some(0)),
            list_height: 0,
            config,
        }
    }

    fn scroll(&mut self, scroll: isize) {
        self.list_state.select(Some(
            self.list_state
                .selected()
                .map(|selected| selected.saturating_add_signed(scroll))
                .unwrap_or(0)
                .min(self.parents.len().saturating_sub(1)),
        ));
    }
}

impl Component for ParentSelectPopup {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = create_popup_block("Select parent");
        let area = centered_rect(area, 50, 60);
        f.render_widget(Clear, area);
        f.render_widget(&block, area);

        let popup_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(2)])
            .split(block.inner(area));

        let mut list_items: Vec<Line<'_>> = self
            .parents
            .iter()
            .map(|parent| parent_line(parent, false))
            .collect();

        if !self.out_of_view.is_empty() {
            list_items.push(Line::default());
            list_items.push(Line::from(Span::styled(
                " ── Not in the log view ──",
                Style::default().fg(Color::DarkGray),
            )));
            list_items.extend(
                self.out_of_view
                    .iter()
                    .map(|parent| parent_line(parent, true)),
            );
        }

        let list = List::new(list_items)
            .scroll_padding(3)
            .highlight_style(Style::default().bg(self.config.highlight_color()));

        f.render_stateful_widget(list, popup_chunks[0], &mut self.list_state);
        self.list_height = popup_chunks[0].height;

        let help = Paragraph::new(vec!["j/k: scroll | Enter: select | Escape: cancel".into()])
            .fg(Color::DarkGray)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );

        f.render_widget(help, popup_chunks[1]);
        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => self.scroll(1),
                KeyCode::Char('k') | KeyCode::Up => self.scroll(-1),
                KeyCode::Char('J') => self.scroll(self.list_height as isize / 2),
                KeyCode::Char('K') => self.scroll((self.list_height as isize / 2).saturating_neg()),
                KeyCode::Enter => {
                    if let Some(parent) = self
                        .list_state
                        .selected()
                        .and_then(|index| self.parents.get(index))
                    {
                        // Moving the selection leaves the repo as it is, so
                        // we take the popup down without a refresh.
                        return Ok(ComponentInputResult::HandledAction(AppAction::Multiple(
                            vec![
                                AppAction::PopupCanceled,
                                AppAction::ViewLog(parent.head.clone()),
                            ],
                        )));
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    return Ok(ComponentInputResult::HandledAction(
                        AppAction::PopupCanceled,
                    ));
                }
                _ => {}
            }
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

    fn parents(range: std::ops::Range<usize>) -> Vec<Parent> {
        range
            .map(|i| Parent {
                head: Head {
                    change_id: ChangeId(format!("change{i}")),
                    commit_id: CommitId(format!("commit{i}")),
                    divergent: false,
                    immutable: false,
                },
                description: format!("parent {i}"),
            })
            .collect()
    }

    fn popup(count: usize, out_of_view: usize) -> ParentSelectPopup {
        ParentSelectPopup::new(
            parents(0..count),
            parents(count..count + out_of_view),
            JjConfig::default(),
        )
    }

    fn press(popup: &mut ParentSelectPopup, key: KeyCode) -> ComponentInputResult {
        popup
            .input(Event::Key(key.into()))
            .expect("the popup handles key presses")
    }

    #[test]
    fn scrolling_is_clamped_to_the_parents_in_view() {
        let mut popup = popup(3, 2);

        for _ in 0..5 {
            press(&mut popup, KeyCode::Char('j'));
        }
        assert_eq!(popup.list_state.selected(), Some(2));

        for _ in 0..5 {
            press(&mut popup, KeyCode::Char('k'));
        }
        assert_eq!(popup.list_state.selected(), Some(0));
    }

    #[test]
    fn enter_shows_the_selected_parent_in_the_log() {
        let mut popup = popup(3, 0);

        press(&mut popup, KeyCode::Char('j'));

        let ComponentInputResult::HandledAction(AppAction::Multiple(actions)) =
            press(&mut popup, KeyCode::Enter)
        else {
            panic!("selecting a parent should take the popup down and show it");
        };
        let [AppAction::PopupCanceled, AppAction::ViewLog(head)] = actions.as_slice() else {
            panic!("selecting a parent should take the popup down and show it");
        };

        assert_eq!(head.commit_id, CommitId("commit1".to_owned()));
    }

    #[test]
    fn cancelling_selects_no_parent() {
        let mut popup = popup(3, 0);

        assert!(matches!(
            press(&mut popup, KeyCode::Esc),
            ComponentInputResult::HandledAction(AppAction::PopupCanceled)
        ));
    }
}
