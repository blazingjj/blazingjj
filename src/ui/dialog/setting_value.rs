/*! A popup taking one line of the configuration, which it hands on as
whatever the caller makes of what was typed: the operation writing an
option to the user's config, for the value of one of those.

A key holding more than any one thing to type, as a command of your own
does, is written whole from the one part of it that was asked for. What
was typed may also be no more than the next thing to ask about, as the
name of a command to add is.
*/

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
use ratatui::widgets::Clear;
use ratatui_textarea::CursorMove;
use ratatui_textarea::TextArea;

use crate::app::command::Command;
use crate::keybinds::PopupEvent;
use crate::keybinds::PopupKeybinds;
use crate::settings::Setting;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::styles::POPUP_WIDTH_PERCENT;
use crate::ui::styles::create_popup_block;
use crate::ui::styles::popup_footer;
use crate::ui::styles::popup_text_width;
use crate::ui::styles::wrapped_height;
use crate::ui::utils::centered_rect_line_height;

/// What the text that was typed asks for, or what is wrong with it.
type Accept = Box<dyn Fn(&str) -> Result<AppAction>>;

pub struct SettingValuePopup<'a> {
    /// What the popup goes up under, which is the config key it is
    /// asking about.
    title: String,
    /// What the text that was typed asks for, which is also what
    /// refuses a text that cannot be read.
    accept: Accept,
    textarea: TextArea<'a>,
    /// What was said about the value that was typed, if it was refused.
    error: Option<anyhow::Error>,
    keybinds: PopupKeybinds,
}

impl SettingValuePopup<'static> {
    /// Ask about `title`, starting from `text`, and do whatever `accept`
    /// makes of what was typed.
    pub fn new(
        title: impl Into<String>,
        text: String,
        accept: impl Fn(&str) -> Result<AppAction> + 'static,
    ) -> Self {
        let mut textarea = TextArea::new(vec![text]);
        textarea.move_cursor(CursorMove::End);

        Self {
            title: title.into(),
            accept: Box::new(accept),
            textarea,
            error: None,
            keybinds: PopupKeybinds::text_line(),
        }
    }

    /// Ask for the value of `key`, starting from `value` as it reads on
    /// screen rather than as the TOML it is written as, which `value_of`
    /// turns it back into.
    pub fn of_key(
        key: impl Into<String>,
        value: String,
        value_of: impl Fn(&str) -> Result<String> + 'static,
    ) -> Self {
        let key = key.into();

        Self::new(key.clone(), value, move |text| {
            Ok(AppAction::Run(Command::SetSetting {
                key: key.clone(),
                value: value_of(text)?,
            }))
        })
    }

    /// Ask for the value of `setting`, starting from `value` as it reads
    /// on screen.
    pub fn of_setting(setting: &'static Setting, value: String) -> Self {
        Self::of_key(setting.key, value, |text| setting.value_of(text))
    }
}

impl Component for SettingValuePopup<'_> {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = create_popup_block(&self.title);

        let error_lines = self
            .error
            .as_ref()
            .map(|err| format!("{err:#}").into_text())
            .transpose()?
            .map(|text| text.lines);
        // What jj says about a value is a sentence rather than a line,
        // so the popup grows by however many rows it wraps into.
        let error_height = error_lines
            .as_ref()
            .map_or(0, |lines| wrapped_height(lines, popup_text_width(area)) + 1);

        let area = centered_rect_line_height(area, POPUP_WIDTH_PERCENT, 5 + error_height);
        f.render_widget(Clear, area);
        f.render_widget(&block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(error_height),
                Constraint::Length(2),
            ])
            .split(block.inner(area));

        f.render_widget(&self.textarea, chunks[0]);

        if let Some(error_lines) = error_lines {
            f.render_widget(popup_footer(error_lines), chunks[1]);
        }

        f.render_widget(
            popup_footer(vec![self.keybinds.hint("accept").into()])
                .fg(Color::DarkGray)
                .alignment(Alignment::Center),
            chunks[2],
        );

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match self.keybinds.match_event(key) {
                PopupEvent::Accept => {
                    // A text that cannot be read is one to correct
                    // rather than one to give up on, so the question
                    // stays up with what was said about it.
                    let asked = match (self.accept)(&self.textarea.lines().join("\n")) {
                        Ok(asked) => asked,
                        Err(err) => {
                            self.error = Some(err);
                            return Ok(ComponentInputResult::Handled);
                        }
                    };

                    return Ok(ComponentInputResult::HandledAction(AppAction::Multiple(
                        vec![AppAction::ClosePopup, asked],
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
