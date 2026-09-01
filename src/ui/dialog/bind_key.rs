/*! A popup taking the key an action is to answer to, which it hands on
as the operation that writes it to the user's config.

Every key it is given is one to bind, so there is nothing left to answer
the popup with: it is done as soon as a key that is not a modifier of its
own is pressed.
*/

use std::str::FromStr;

use anyhow::Result;
use anyhow::bail;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEvent;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;

use crate::app::command::Command;
use crate::keybinds::Binding;
use crate::keybinds::Context;
use crate::keybinds::Shortcut;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::styles::POPUP_WIDTH_PERCENT;
use crate::ui::styles::create_popup_block;
use crate::ui::styles::popup_footer;
use crate::ui::styles::popup_text_width;
use crate::ui::styles::wrapped_height;
use crate::ui::utils::centered_rect_line_height;

/// What the key pressed is to become of the keys the action answers to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BindKey {
    /// The one key it is to answer to from now on.
    Only,
    /// Another key beside the ones it answers to already.
    Besides,
}

pub struct BindKeyPopup {
    binding: Binding,
    bind: BindKey,
    /// What was said about the key that was pressed, if it was refused.
    error: Option<anyhow::Error>,
}

impl BindKeyPopup {
    /// Ask for a key `binding` is to answer to.
    pub fn new(binding: Binding, bind: BindKey) -> Self {
        Self {
            binding,
            bind,
            error: None,
        }
    }

    /// What binding `key` to the action comes to, or why it cannot be
    /// bound to it.
    fn bind(&self, key: KeyEvent) -> Result<AppAction> {
        let shortcut = Shortcut::from_event(key);
        let text = shortcut.to_string();
        // The configuration holds a key by the name it is written under,
        // so one we could not read back is one we cannot offer.
        if Shortcut::from_str(&text) != Ok(shortcut) {
            bail!("{text} is not a key that can be bound.");
        }

        if let Some(bound) = self.bound_elsewhere(shortcut) {
            bail!(
                "{text} is already bound to “{}” ({}).",
                bound.description,
                bound.context.title()
            );
        }

        Ok(AppAction::Multiple(vec![
            AppAction::ClosePopup,
            AppAction::Run(Command::SetSetting {
                key: self.config_key(),
                value: self.value_of(shortcut),
            }),
        ]))
    }

    /// The TOML expression for the action answering to `shortcut`,
    /// which for one key beside others is the list of them all.
    fn value_of(&self, shortcut: Shortcut) -> String {
        let keys = match self.bind {
            BindKey::Only => vec![shortcut],
            BindKey::Besides => {
                let mut keys = self.binding.keys.clone();
                // Pressing a key the action already answers to leaves it
                // answering to it, rather than twice over.
                if !keys.contains(&shortcut) {
                    keys.push(shortcut);
                }
                keys
            }
        };

        match keys.as_slice() {
            [key] => toml::Value::String(key.to_string()).to_string(),
            keys => toml::Value::Array(
                keys.iter()
                    .map(|key| toml::Value::String(key.to_string()))
                    .collect(),
            )
            .to_string(),
        }
    }

    /// The action `shortcut` answers to already, of those whose keys are
    /// live alongside the ones being bound.
    fn bound_elsewhere(&self, shortcut: Shortcut) -> Option<Binding> {
        Context::ORDER
            .into_iter()
            .filter(|context| self.binding.context.shares_keys_with(*context))
            .flat_map(Context::bindings)
            .find(|binding| {
                (binding.context, binding.name) != (self.binding.context, self.binding.name)
                    && binding.keys.contains(&shortcut)
            })
    }

    fn config_key(&self) -> String {
        self.binding
            .key()
            .expect("the binding is one the user can change")
    }
}

impl Component for BindKeyPopup {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let key = self.config_key();
        let block = create_popup_block(&key);

        let asked = match self.bind {
            BindKey::Only => "Press the key for",
            BindKey::Besides => "Press another key for",
        };
        let lines = vec![
            Line::raw(format!("{asked} “{}”.", self.binding.description)),
            Line::raw(""),
            match self.error.as_ref() {
                Some(error) => Line::raw(format!("{error:#}")).fg(Color::Red),
                None => Line::raw(format!("It answers to {} now.", self.binding.keys_text()))
                    .fg(Color::DarkGray),
            },
        ];

        // What is said about a key is a sentence rather than a line, so
        // the popup takes however many rows it wraps into, plus the two
        // the hint under it takes and the two of the border.
        let text_height = wrapped_height(&lines, popup_text_width(area));

        let area = centered_rect_line_height(area, POPUP_WIDTH_PERCENT, text_height + 4);
        f.render_widget(Clear, area);
        f.render_widget(&block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(2)])
            .split(block.inner(area));

        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), chunks[0]);
        f.render_widget(
            popup_footer(vec![Line::raw(
                "every key is one to bind, so there is none to leave by",
            )])
            .fg(Color::DarkGray)
            .alignment(Alignment::Center),
            chunks[1],
        );

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        let Event::Key(key) = event else {
            return Ok(ComponentInputResult::Handled);
        };
        if key.kind != KeyEventKind::Press {
            return Ok(ComponentInputResult::Handled);
        }
        // A modifier is only ever half of a key to bind, so holding one
        // down is the popup waiting rather than the popup answered.
        if matches!(key.code, KeyCode::Modifier(_) | KeyCode::Null) {
            return Ok(ComponentInputResult::Handled);
        }

        // A key the action cannot be bound to is one to press again
        // rather than one to give up on, so the popup stays up with what
        // was said about it.
        match self.bind(key) {
            Ok(action) => Ok(ComponentInputResult::HandledAction(action)),
            Err(error) => {
                self.error = Some(error);
                Ok(ComponentInputResult::Handled)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyModifiers;

    use super::*;
    use crate::env::set_test_env;

    fn popup(description: &str) -> BindKeyPopup {
        bind_popup(description, BindKey::Only)
    }

    fn bind_popup(description: &str, bind: BindKey) -> BindKeyPopup {
        set_test_env();
        let binding = Context::LogTab
            .bindings()
            .into_iter()
            .find(|binding| binding.description == description)
            .expect("the action is one the app has");

        BindKeyPopup::new(binding, bind)
    }

    #[test]
    fn a_key_is_written_under_the_name_it_reads_by() {
        let popup = popup("abandon change");

        let action = popup
            .bind(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL))
            .expect("the key can be bound");

        let AppAction::Multiple(actions) = action else {
            panic!("binding a key closes the popup and writes the key");
        };
        assert!(matches!(
            actions.as_slice(),
            [
                AppAction::ClosePopup,
                AppAction::Run(Command::SetSetting { key, value })
            ] if key == "blazingjj.keybinds.log-tab.abandon" && value == "\"Control+k\""
        ));
    }

    /// An action can answer to several keys, so binding one beside the
    /// keys it has writes them all rather than the one.
    #[test]
    fn a_key_bound_besides_is_written_beside_the_keys_it_joins() {
        let only = popup("new change");
        let besides = bind_popup("new change", BindKey::Besides);
        let key = Shortcut::from_str("ctrl+n").expect("shortcut should parse");

        assert_eq!(only.value_of(key), "\"Control+n\"");
        assert_eq!(besides.value_of(key), "[\"n\", \"Control+n\"]");
        // The keys it answers to are the keys it answers to, however
        // many times one of them is pressed.
        let bound = Shortcut::from_str("n").expect("shortcut should parse");
        assert_eq!(besides.value_of(bound), "\"n\"");
    }

    /// A key we cannot write down is one we would lose on the way to the
    /// config file, so it is refused where it is pressed.
    #[test]
    fn a_key_that_cannot_be_written_down_is_refused() {
        let popup = popup("abandon change");

        assert!(
            popup
                .bind(KeyEvent::new(KeyCode::CapsLock, KeyModifiers::empty()))
                .is_err()
        );
    }

    /// Only one tab is up at a time, so a key another tab answers to is
    /// a key this one is free to take.
    #[test]
    fn a_key_only_another_context_answers_to_is_taken() {
        let popup = popup("abandon change");

        // What the files tab untracks a file with, which the log tab
        // has nothing bound to.
        assert!(
            popup
                .bind(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty()))
                .is_ok()
        );
    }

    /// A tab answers to the keys that hold everywhere as well, so one
    /// of its actions taking such a key leaves that key doing only the
    /// one thing, in that tab.
    #[test]
    fn a_key_an_action_beside_the_context_answers_to_is_refused() {
        let popup = popup("abandon change");

        let Err(error) = popup.bind(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()))
        else {
            panic!("the key is the quit binding");
        };
        assert!(error.to_string().contains("quit"), "{error}");
        assert!(error.to_string().contains("Everywhere"), "{error}");
    }

    /// Two actions of one context answering to the same key leaves only
    /// one of them reachable, whichever the app happens to match first.
    #[test]
    fn a_key_another_action_of_the_same_context_answers_to_is_refused() {
        let popup = popup("abandon change");

        let Err(error) = popup.bind(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::empty()))
        else {
            panic!("the key is the describe binding");
        };
        assert!(error.to_string().contains("describe change"), "{error}");
    }
}
