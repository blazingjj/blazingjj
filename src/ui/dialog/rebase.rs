/*! The rebase popup allows the user to pick a rebase configuration and
 start rebase, or cancel the opreation.

 The UI looks like this
 ~~~
    Source   (zsztoxlv)
    ( ) -s this and descendants
    ( ) -b whole branch
    (*) -r only one change moves
    Target @ (umrpslui)
    (*) -d rebase onto @ as new branch
    ( ) -A rebase after @
    ( ) -B rebase before @

    Esc: Cancel    Enter: Rebase
~~~
A radio button is selected by s, b, r, d, shift+a or shift+b, and the
popup is closed by Enter, Esc or q, both as configured.


*/

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::prelude::Buffer;
use ratatui::prelude::Constraint;
use ratatui::prelude::Direction;
use ratatui::prelude::Layout;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::StatefulWidget;

use crate::app::command::Command;
use crate::commander::ids::CommitId;
use crate::commander::jj::RebaseSource;
use crate::commander::jj::RebaseTarget;
use crate::commander::log::Head;
use crate::commander::revset::Revset;
use crate::keybinds::PopupEvent;
use crate::keybinds::PopupKeybinds;
use crate::keybinds::rebase_popup::CutOption;
use crate::keybinds::rebase_popup::PasteOption;
use crate::keybinds::rebase_popup::PopupAction;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::utils::centered_rect_fixed;

type Keybinds = crate::keybinds::rebase_popup::Keybinds;

/// A transient popup for configuring a rebase command
pub struct RebasePopup {
    pub keybinds: Keybinds,
    popup_keybinds: PopupKeybinds,

    pub source_revs: Vec<CommitId>,
    pub target_rev: Head,

    pub source_mode: CutOption,
    pub target_mode: PasteOption,
}

impl RebasePopup {
    pub fn new(source_revs: Vec<CommitId>, target_rev: Head) -> Self {
        Self {
            keybinds: Keybinds::new(),
            popup_keybinds: PopupKeybinds::dialog(),
            source_revs,
            target_rev,
            source_mode: CutOption::SingleRevision,
            target_mode: PasteOption::NewBranch,
        }
    }

    /// The rebase the popup is currently configured to ask for, or [None]
    /// when it has no source to move.
    fn command(&self) -> Option<Command> {
        Some(Command::Rebase {
            source: Revset::union(&self.source_revs)?,
            source_mode: match self.source_mode {
                CutOption::IncludeDescendants => RebaseSource::Descendants,
                CutOption::IncludeBranch => RebaseSource::Branch,
                CutOption::SingleRevision => RebaseSource::SingleRevision,
            },
            target: self.target_rev.clone(),
            target_mode: match self.target_mode {
                PasteOption::NewBranch => RebaseTarget::Onto,
                PasteOption::InsertAfter => RebaseTarget::After,
                PasteOption::InsertBefore => RebaseTarget::Before,
            },
        })
    }
}

impl Component for RebasePopup {
    /// Render the dialog into the area.
    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> Result<()> {
        let area = centered_rect_fixed(area, 32, 12);
        // The border of the dialog
        let block = Block::bordered()
            .title(Span::styled(" Rebase ", Style::new().bold().cyan()))
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green));
        frame.render_widget(Clear, area);
        frame.render_widget(&block, area);

        // Split area into chunks. Even though the area size is constant,
        // we pretend it can change in the future.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .vertical_margin(1)
            .horizontal_margin(2)
            .constraints(
                [
                    Constraint::Length(1), // title "Source"
                    Constraint::Min(3),    // buttons for source mode
                    Constraint::Length(1), // title "Target"
                    Constraint::Min(3),    // buttons for target mode
                    Constraint::Length(2), // help text
                ]
                .as_ref(),
            )
            .split(area);

        // Radio buttons for source
        let src_options = vec![
            "-s this and descendants",
            "-b whole branch",
            "-r only one change moves",
        ];
        let mut src_select: usize = match self.source_mode {
            CutOption::IncludeDescendants => 0,
            CutOption::IncludeBranch => 1,
            CutOption::SingleRevision => 2,
        };
        let src_label = match self.source_revs.as_slice() {
            [only] => {
                let prefix: String = only.as_str().chars().take(8).collect();
                format!("Source {prefix}")
            }
            many => format!("Source: {} tagged changes", many.len()),
        };
        frame.render_widget(Paragraph::new(Span::raw(src_label)), chunks[0]);
        frame.render_stateful_widget(RadioButton::new(src_options), chunks[1], &mut src_select);

        // Radio buttons for target
        let tgt_change_id: String = self.target_rev.change_id.as_str().chars().take(8).collect();
        let tgt_commit_id: String = self.target_rev.commit_id.as_str().chars().take(8).collect();
        let tgt_options = vec![
            "-d rebase as new branch",
            "-A rebase after",
            "-B rebase before",
        ];
        let mut tgt_select: usize = match self.target_mode {
            PasteOption::NewBranch => 0,
            PasteOption::InsertAfter => 1,
            PasteOption::InsertBefore => 2,
        };
        frame.render_widget(
            Paragraph::new(Span::raw(format!("Target {tgt_change_id} {tgt_commit_id}"))),
            chunks[2],
        );
        frame.render_stateful_widget(RadioButton::new(tgt_options), chunks[3], &mut tgt_select);

        // Help on terminating dialog
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::raw(""),
                Line::raw(self.popup_keybinds.hint("rebase")),
            ])),
            chunks[4],
        );

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        let Event::Key(key) = event else {
            return Ok(ComponentInputResult::Handled);
        };

        match self.popup_keybinds.match_event(key) {
            PopupEvent::Accept => {
                let mut actions = vec![AppAction::ClosePopup];
                actions.extend(self.command().map(AppAction::Run));
                return Ok(ComponentInputResult::HandledAction(AppAction::Multiple(
                    actions,
                )));
            }
            PopupEvent::Cancel => {
                return Ok(ComponentInputResult::HandledAction(AppAction::ClosePopup));
            }
            _ => {}
        }

        match self.keybinds.match_event(key) {
            PopupAction::SetSourceMode(m) => self.source_mode = m,
            PopupAction::SetTargetMode(m) => self.target_mode = m,
            PopupAction::None => {}
        }

        Ok(ComponentInputResult::Handled)
    }
}

/****************************************************************/
// TODO(@peso): Move this widget to a separate file

/** A widget for a group of radio buttons.

It is a stateful widget.
The state is an usize number that indicates which label is
selected.

Example:
~~~
( ) apples
( ) bananas
(*) lemons
~~~
*/
struct RadioButton {
    /// Button labels
    pub labels: Vec<String>,
    /// Button style can be modified before drawing
    pub button_style: Style,
    /// Label style can be modified before drawing
    pub label_style: Style,
}

impl RadioButton {
    pub fn new(labels: Vec<&str>) -> Self {
        let button_style = Style::default().fg(Color::White);
        let label_style = Style::default().fg(Color::White);
        Self {
            labels: labels.iter().map(|s| s.to_string()).collect(),
            button_style,
            label_style,
        }
    }
}

impl StatefulWidget for RadioButton {
    type State = usize;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        for (row, label) in self.labels.iter().enumerate() {
            let button = if row == *state { "(*)" } else { "( )" };
            buf.set_string(
                area.left(),
                area.top() + row as u16,
                button,
                self.button_style,
            );
            buf.set_string(
                area.left() + 4_u16,
                area.top() + row as u16,
                label,
                self.label_style,
            );
        }
    }
}
