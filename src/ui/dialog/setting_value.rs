/*! A popup taking the value of one setting, which it hands on as the
operation that writes it to the user's config.
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

pub struct SettingValuePopup<'a> {
    setting: &'static Setting,
    textarea: TextArea<'a>,
    /// What was said about the value that was typed, if it was refused.
    error: Option<anyhow::Error>,
    keybinds: PopupKeybinds,
}

impl SettingValuePopup<'static> {
    /// Ask for the value of `setting`, starting from `value` as it reads
    /// on screen rather than as the TOML it is written as.
    pub fn new(setting: &'static Setting, value: String) -> Self {
        let mut textarea = TextArea::new(vec![value]);
        textarea.move_cursor(CursorMove::End);

        Self {
            setting,
            textarea,
            error: None,
            keybinds: PopupKeybinds::text_line(),
        }
    }
}

impl Component for SettingValuePopup<'_> {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = create_popup_block(self.setting.key);

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
                    // A value the setting cannot be read from is one to
                    // correct rather than one to give up on, so the
                    // question stays up with what was said about it.
                    let value = match self.setting.value_of(&self.textarea.lines().join("\n")) {
                        Ok(value) => value,
                        Err(err) => {
                            self.error = Some(err);
                            return Ok(ComponentInputResult::Handled);
                        }
                    };

                    return Ok(ComponentInputResult::HandledAction(AppAction::Multiple(
                        vec![
                            AppAction::ClosePopup,
                            AppAction::Run(Command::SetSetting {
                                key: self.setting.key.to_owned(),
                                value,
                            }),
                        ],
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
