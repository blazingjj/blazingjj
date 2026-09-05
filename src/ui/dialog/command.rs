use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui_textarea::TextArea;
use shell_words::split;

use crate::commander::JjCommand;
use crate::commander::NO_EDITOR;
use crate::commander::new_commander;
use crate::keybinds::PopupEvent;
use crate::keybinds::PopupKeybinds;
use crate::selection::Selection;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Interactive;
use crate::ui::dialog::MessagePopup;
use crate::ui::utils::centered_rect_line_height;

/// What to tell someone whose command turned out to want an editor.
const NEEDS_INTERACTIVE: &str = "This command wants an editor, which it cannot have while \
                                 blazingjj holds the terminal.";

/// How the command popup runs what is typed into it.
#[derive(Clone, Copy)]
pub enum CommandMode {
    /// Capture the output and put it up in a popup.
    Capture,
    /// Hand the terminal over, so that the command can be interactive.
    Interactive,
}

pub struct CommandPopup<'a> {
    mode: CommandMode,
    command_textarea: TextArea<'a>,
    /// What the tab the popup was opened over has selected, which the
    /// placeholders of the command stand for.
    selection: Selection,
    keybinds: PopupKeybinds,
}

impl CommandPopup<'_> {
    pub fn new(mode: CommandMode, selection: Selection) -> Self {
        Self {
            mode,
            command_textarea: TextArea::new(vec![]),
            selection,
            keybinds: PopupKeybinds::text_line(),
        }
    }

    /// What was typed, without the `jj` it may have been started with.
    fn command(&self) -> String {
        let typed = self.command_textarea.lines().join(" ");
        let command = match typed.strip_prefix("jj") {
            Some(rest) if rest.is_empty() || rest.starts_with(' ') => rest.trim_start(),
            _ => &typed,
        };

        command.to_owned()
    }
}

/// Run the command and put its output up. It is left without an editor, as
/// it cannot have the terminal while we hold it.
fn run_captured(title: String, args: &[String]) -> ComponentInputResult {
    let said = match new_commander().jj(args).no_editor().color().verbose().run() {
        Ok(output) if output.trim().is_empty() => AppAction::ClosePopup,
        Ok(output) => popup(title, output),
        Err(err) => {
            // Having named the editor ourselves, we know a command that
            // wanted one when we see it, and can offer the way out.
            let wants_editor = err.to_string().contains(NO_EDITOR);
            let report = format!("Failed to execute jj command: {title}\n\n{err}");
            if wants_editor {
                AppAction::SetPopup(Box::new(RetryInteractivelyPopup::new(
                    title,
                    report,
                    new_commander().jj(args),
                )))
            } else {
                popup(title, report)
            }
        }
    };

    // Even a command that came back unhappy may have moved the repo, jj
    // snapshotting the working copy before it gets that far.
    ComponentInputResult::HandledAction(AppAction::Multiple(vec![said, AppAction::MarkTabsStale]))
}

/// Put `output` up under `title`.
fn popup(title: String, output: String) -> AppAction {
    AppAction::SetPopup(Box::new(
        MessagePopup::new(title, output).text_align(Alignment::Left),
    ))
}

/// Run `command` with the terminal handed over to it, holding it once the
/// command is done so that what it printed can be read.
fn run_interactively(command: JjCommand) -> AppAction {
    AppAction::RunInteractive(Interactive {
        program: command.foreground(),
        hold_screen: true,
    })
}

/// Offers to hand the terminal to a command that could not be run with its
/// output captured.
struct RetryInteractivelyPopup<'a> {
    message: MessagePopup<'a>,
    keybinds: PopupKeybinds,
    /// Taken when the offer is accepted, which happens at most once.
    command: Option<JjCommand>,
}

impl RetryInteractivelyPopup<'_> {
    fn new(title: String, report: String, command: JjCommand) -> Self {
        let keybinds = PopupKeybinds::dialog();
        let offer = keybinds.hint("run it interactively");

        Self {
            message: MessagePopup::new(
                title,
                format!("{report}\n\n{NEEDS_INTERACTIVE}\n\n{offer}"),
            )
            .text_align(Alignment::Left),
            keybinds,
            command: Some(command),
        }
    }
}

impl Component for RetryInteractivelyPopup<'_> {
    fn draw(&mut self, f: &mut ratatui::Frame<'_>, area: ratatui::prelude::Rect) -> Result<()> {
        self.message.draw(f, area)
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && self.keybinds.match_event(key) == PopupEvent::Accept
            && let Some(command) = self.command.take()
        {
            return Ok(ComponentInputResult::HandledAction(AppAction::Multiple(
                vec![AppAction::ClosePopup, run_interactively(command)],
            )));
        }

        self.message.input(event)
    }
}

impl Component for CommandPopup<'_> {
    fn draw(
        &mut self,
        f: &mut ratatui::Frame<'_>,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        let title = match self.mode {
            CommandMode::Capture => " Command ",
            CommandMode::Interactive => " Interactive command ",
        };
        let block = Block::bordered()
            .title(Span::styled(title, Style::new().bold().cyan()))
            .title_alignment(Alignment::Center)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green));
        let area = centered_rect_line_height(area, 60, 5);
        f.render_widget(Clear, area);
        f.render_widget(&block, area);

        let popup_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(2)])
            .split(block.inner(area));

        f.render_widget(&self.command_textarea, popup_chunks[0]);

        let help = Paragraph::new(vec![self.keybinds.hint("run").into()])
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

    fn input(&mut self, event: Event) -> anyhow::Result<ComponentInputResult> {
        if let Event::Key(key) = event {
            let cancel = Ok(ComponentInputResult::HandledAction(AppAction::ClosePopup));
            match self.keybinds.match_event(key) {
                PopupEvent::Accept => {
                    let command = self.command();
                    if command.trim().is_empty() {
                        return cancel;
                    }

                    let typed = format!("jj {command}");
                    let args = match split(&command) {
                        Ok(args) => args,
                        Err(err) => {
                            let report = format!("Failed to split command input\n\n{err}");
                            return Ok(ComponentInputResult::HandledAction(popup(typed, report)));
                        }
                    };
                    // What ran is what the command came to once the
                    // selection was put in it, which is what there is to
                    // read the output against.
                    let args = match self.selection.substitute(&args) {
                        Ok(args) => args,
                        Err(missing) => {
                            let report = format!("Nothing to run the command against\n\n{missing}");
                            return Ok(ComponentInputResult::HandledAction(popup(typed, report)));
                        }
                    };
                    let title = format!("jj {}", args.join(" "));

                    return Ok(match self.mode {
                        CommandMode::Capture => run_captured(title, &args),
                        CommandMode::Interactive => {
                            ComponentInputResult::HandledAction(AppAction::Multiple(vec![
                                AppAction::ClosePopup,
                                run_interactively(new_commander().jj(args)),
                            ]))
                        }
                    });
                }
                PopupEvent::Cancel => return cancel,
                _ => {}
            }
        };
        self.command_textarea.input(event);
        Ok(ComponentInputResult::Handled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_for(typed: &str) -> String {
        CommandPopup {
            mode: CommandMode::Capture,
            command_textarea: TextArea::new(vec![typed.to_owned()]),
            selection: Selection::default(),
            keybinds: PopupKeybinds::text_line(),
        }
        .command()
    }

    #[test]
    fn the_jj_one_may_type_is_not_passed_on_to_jj() {
        assert_eq!(command_for("status"), "status");
        assert_eq!(command_for("jj status"), "status");
        assert_eq!(command_for("jj"), "");
        // Only the one we put there ourselves, and only where a command
        // would otherwise stand.
        assert_eq!(command_for("jj jj status"), "jj status");
        assert_eq!(command_for("jjdescribe"), "jjdescribe");
    }
}
