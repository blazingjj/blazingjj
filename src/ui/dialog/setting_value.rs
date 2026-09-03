/*! A popup taking one line of the configuration, which it hands on as
whatever the caller makes of what was typed: the operation writing an
option to the user's config, for the value of one of those.

A key holding more than any one thing to type, as a command of your own
does, is written whole from the one part of it that was asked for. What
was typed may also be no more than the next thing to ask about, as the
name of a command to add is.
*/

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui_textarea::CursorMove;
use ratatui_textarea::TextArea;

use crate::app::command::Command;
use crate::keybinds::PopupEvent;
use crate::keybinds::PopupKeybinds;
use crate::settings::Setting;
use crate::theme::Role;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::styles::AnsiText;
use crate::ui::styles::POPUP_WIDTH_PERCENT;
use crate::ui::styles::clear;
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
    /// The config key clearing the field takes out, where there is one
    /// to take out. It is the key asked about unless what is written is
    /// only part of a value that was set as a whole.
    taken_out: Option<String>,
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
            taken_out: None,
            accept: Box::new(accept),
            textarea,
            error: None,
            keybinds: PopupKeybinds::text_line(),
        }
    }

    /// Ask for the value of `key`, starting from `value` as it reads on
    /// screen rather than as the TOML it is written as, which `value_of`
    /// turns it back into. `taken_out` is the key clearing the field
    /// takes out, if any.
    pub fn of_key(
        key: impl Into<String>,
        taken_out: Option<String>,
        value: String,
        value_of: impl Fn(&str) -> Result<String> + 'static,
    ) -> Self {
        let key = key.into();
        let asked = key.clone();

        Self {
            taken_out,
            ..Self::new(key, value, move |text| {
                Ok(AppAction::Run(Command::SetSetting {
                    key: asked.clone(),
                    value: value_of(text)?,
                }))
            })
        }
    }

    /// Ask for the value of `setting`, starting from `value` as it reads
    /// on screen.
    pub fn of_setting(setting: &'static Setting, value: String) -> Self {
        Self::of_key(setting.key, Some(setting.key.to_owned()), value, |text| {
            setting.value_of(text)
        })
    }
}

impl Component for SettingValuePopup<'_> {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = create_popup_block(&self.title);

        let error_lines = self
            .error
            .as_ref()
            .map(|err| format!("{err:#}").owned_ansi_text())
            .transpose()?
            .map(|text| text.lines);
        // What jj says about a value is a sentence rather than a line,
        // so the popup grows by however many rows it wraps into.
        let error_height = error_lines
            .as_ref()
            .map_or(0, |lines| wrapped_height(lines, popup_text_width(area)) + 1);

        let area = centered_rect_line_height(area, POPUP_WIDTH_PERCENT, 5 + error_height);
        clear(f, area);
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
            popup_footer(vec![
                format!("{} | empty: take out", self.keybinds.hint("accept")).into(),
            ])
            .style(Role::Hint.style())
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
                    let typed = self.textarea.lines().join("\n");

                    // Clearing the field is asking for the option to be
                    // taken out rather than for it to be set to nothing:
                    // there is no value that stands for "as if it were
                    // never set", so what says so is saying nothing. An
                    // option nothing has set is already out.
                    let asked = if typed.trim().is_empty() {
                        let Some(key) = self.taken_out.clone() else {
                            return Ok(ComponentInputResult::HandledAction(AppAction::ClosePopup));
                        };

                        AppAction::Run(Command::UnsetSetting { key })
                    } else {
                        // A text that cannot be read is one to correct
                        // rather than one to give up on, so the question
                        // stays up with what was said about it.
                        match (self.accept)(&typed) {
                            Ok(asked) => asked,
                            Err(err) => {
                                self.error = Some(err);
                                return Ok(ComponentInputResult::Handled);
                            }
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

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyCode;
    use ratatui::crossterm::event::KeyEvent;

    use super::*;
    use crate::env::set_test_env;
    use crate::settings::SETTINGS;

    fn setting(key: &str) -> &'static Setting {
        SETTINGS
            .iter()
            .find(|setting| setting.key == key)
            .expect("the setting is one the app has")
    }

    /// What accepting the popup asks for, having started from `value`.
    fn accepted(key: &str, value: &str) -> Command {
        set_test_env();
        let mut popup = SettingValuePopup::of_setting(setting(key), value.to_owned());

        let result = popup
            .input(Event::Key(KeyEvent::from(KeyCode::Enter)))
            .expect("the key is handled");
        let ComponentInputResult::HandledAction(AppAction::Multiple(actions)) = result else {
            panic!("accepting asks for something to be done");
        };
        let Some(AppAction::Run(command)) = actions.into_iter().nth(1) else {
            panic!("what it asks for is an operation");
        };

        command
    }

    #[test]
    fn what_is_typed_is_what_the_option_is_set_to() {
        let Command::SetSetting { key, value } = accepted("blazingjj.layout", "vertical") else {
            panic!("a value is set");
        };

        assert_eq!(key, "blazingjj.layout");
        assert_eq!(value, "\"vertical\"");
    }

    /// There is no value that stands for "as if it were never set", so
    /// clearing the field is what says it: the option goes out of the
    /// config rather than being set to nothing.
    #[test]
    fn clearing_the_field_takes_the_option_out_of_the_config() {
        let Command::UnsetSetting { key } = accepted("blazingjj.layout", "") else {
            panic!("the option is taken out");
        };

        assert_eq!(key, "blazingjj.layout");
    }

    /// Including for an option that would otherwise refuse an empty
    /// field for not being the kind of value it takes.
    #[test]
    fn clearing_the_field_takes_out_an_option_that_takes_a_number() {
        assert!(setting("blazingjj.layout-percent").value_of("  ").is_err());

        let Command::UnsetSetting { key } = accepted("blazingjj.layout-percent", "  ") else {
            panic!("the option is taken out");
        };

        assert_eq!(key, "blazingjj.layout-percent");
    }
}
