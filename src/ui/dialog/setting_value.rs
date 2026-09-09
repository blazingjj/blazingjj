/*! A popup taking the value of one setting, which it hands on as the
operation that writes it to the user's config.
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

type ValueOf = Box<dyn Fn(&str) -> Result<String>>;

pub struct SettingValuePopup<'a> {
    /// The config key the value is written to, which the popup is also
    /// titled by.
    key: String,
    /// The config key clearing the field takes out, where there is one
    /// to take out. It is the key written to unless what is written is
    /// only part of a value that was set as a whole.
    taken_out: Option<String>,
    /// What the typed text is written as, or why it cannot be.
    value_of: ValueOf,
    textarea: TextArea<'a>,
    /// What was said about the value that was typed, if it was refused.
    error: Option<anyhow::Error>,
    keybinds: PopupKeybinds,
}

impl SettingValuePopup<'static> {
    /// Ask for the value of `setting`, starting from `value` as it reads
    /// on screen rather than as the TOML it is written as.
    pub fn new(setting: &'static Setting, value: String) -> Self {
        let key = setting.key.to_owned();

        Self::for_key(key.clone(), Some(key), value, |input| {
            setting.value_of(input)
        })
    }

    /// Ask for what `key` is to be set to, starting from `value`, with
    /// `value_of` saying what the typed text is written as and
    /// `taken_out` the key clearing the field takes out, if any. For a
    /// key that is no option of the settings tab's.
    pub fn for_key(
        key: String,
        taken_out: Option<String>,
        value: String,
        value_of: impl Fn(&str) -> Result<String> + 'static,
    ) -> Self {
        let mut textarea = TextArea::new(vec![value]);
        textarea.move_cursor(CursorMove::End);

        Self {
            key,
            taken_out,
            value_of: Box::new(value_of),
            textarea,
            error: None,
            keybinds: PopupKeybinds::text_line(),
        }
    }
}

impl Component for SettingValuePopup<'_> {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = create_popup_block(&self.key);

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
                    let key = self.key.clone();

                    // Clearing the field is asking for the option to be
                    // taken out rather than for it to be set to nothing:
                    // there is no value that stands for "as if it were
                    // never set", so what says so is saying nothing. An
                    // option nothing has set is already out.
                    let command = if typed.trim().is_empty() {
                        let Some(key) = self.taken_out.clone() else {
                            return Ok(ComponentInputResult::HandledAction(AppAction::ClosePopup));
                        };

                        Command::UnsetSetting { key }
                    } else {
                        // A value the setting cannot be read from is one
                        // to correct rather than one to give up on, so
                        // the question stays up with what was said about
                        // it.
                        match (self.value_of)(&typed) {
                            Ok(value) => Command::SetSetting { key, value },
                            Err(err) => {
                                self.error = Some(err);
                                return Ok(ComponentInputResult::Handled);
                            }
                        }
                    };

                    return Ok(ComponentInputResult::HandledAction(AppAction::Multiple(
                        vec![AppAction::ClosePopup, AppAction::Run(command)],
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
        let mut popup = SettingValuePopup::new(setting(key), value.to_owned());

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
