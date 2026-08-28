/*! The parents of a change, as the choices for moving the log's
selection onto one of them.
*/

use std::sync::mpsc::Sender;

use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

use crate::commander::log::Head;
use crate::commander::log::Parent;
use crate::env::JjConfig;
use crate::ui::dialog::ChoicePopup;

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

/// A popup offering the `parents` to pick from. Those `out_of_view` are
/// listed underneath so that the merge is shown in full, but cannot be
/// picked.
pub fn parent_select(
    config: JjConfig,
    tx: Sender<Head>,
    parents: &[Parent],
    out_of_view: &[Parent],
) -> ChoicePopup<Head> {
    let items = parents
        .iter()
        .map(|parent| (parent_line(parent, false), parent.head.clone()))
        .collect();
    let popup = ChoicePopup::new(config, tx, "Select parent", items);

    if out_of_view.is_empty() {
        return popup;
    }

    let mut footnote = vec![
        Line::default(),
        Line::from(Span::styled(
            " ── Not in the log view ──",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    footnote.extend(out_of_view.iter().map(|parent| parent_line(parent, true)));

    popup.footnote(footnote)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;
    use std::sync::mpsc::channel;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::Event;
    use ratatui::crossterm::event::KeyCode;

    use super::*;
    use crate::commander::ids::ChangeId;
    use crate::commander::ids::CommitId;
    use crate::ui::Component;

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

    fn popup(in_view: usize, out_of_view: usize) -> (ChoicePopup<Head>, Receiver<Head>) {
        let (tx, rx) = channel();
        let all = parents(0..in_view + out_of_view);

        (
            parent_select(JjConfig::default(), tx, &all[..in_view], &all[in_view..]),
            rx,
        )
    }

    /// What the popup puts on a 100x40 screen
    fn render(popup: &mut ChoicePopup<Head>) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("the test backend");
        terminal
            .draw(|f| popup.draw(f, f.area()).expect("the popup draws"))
            .expect("the frame is drawn");

        terminal.backend().buffer().clone()
    }

    fn rows(buffer: &Buffer) -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn press(popup: &mut ChoicePopup<Head>, key: KeyCode) {
        popup
            .input(Event::Key(key.into()))
            .expect("the popup handles key presses");
    }

    #[test]
    fn picking_a_parent_sends_its_head() {
        let (mut popup, rx) = popup(3, 0);

        press(&mut popup, KeyCode::Char('j'));
        press(&mut popup, KeyCode::Enter);

        assert_eq!(
            rx.try_recv().map(|head| head.change_id.0),
            Ok("change1".into())
        );
    }

    #[test]
    fn parents_out_of_view_are_listed_below_a_separator() {
        let (mut popup, _rx) = popup(2, 1);

        let buffer = render(&mut popup);
        let listed: Vec<String> = rows(&buffer)
            .into_iter()
            .filter(|row| row.contains("change") || row.contains("Not in the log view"))
            .collect();

        assert!(listed[0].contains("change0"), "{listed:?}");
        assert!(listed[1].contains("change1"), "{listed:?}");
        assert!(
            listed[2].contains("── Not in the log view ──"),
            "{listed:?}"
        );
        assert!(listed[3].contains("change2"), "{listed:?}");
    }

    #[test]
    fn parents_out_of_view_cannot_be_picked() {
        let (mut popup, rx) = popup(2, 2);

        for _ in 0..5 {
            press(&mut popup, KeyCode::Char('j'));
        }
        press(&mut popup, KeyCode::Enter);

        assert_eq!(
            rx.try_recv().map(|head| head.change_id.0),
            Ok("change1".into())
        );
    }

    #[test]
    fn nothing_is_listed_below_the_parents_when_they_are_all_in_view() {
        let (mut popup, _rx) = popup(2, 0);

        let buffer = render(&mut popup);

        assert!(
            !rows(&buffer)
                .iter()
                .any(|row| row.contains("Not in the log view"))
        );
    }

    #[test]
    fn a_parent_without_a_description_says_so() {
        let (tx, _rx) = channel();
        let mut parent = parents(0..1);
        parent[0].description = String::new();
        let mut popup = parent_select(JjConfig::default(), tx, &parent, &[]);

        let buffer = render(&mut popup);

        assert!(
            rows(&buffer)
                .iter()
                .any(|row| row.contains("change0 (no description)"))
        );
    }
}
