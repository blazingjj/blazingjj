//! The loader popup presents a cute little animation and an operation name and should be used for
//! operations known to possibly take some time.

use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Clear;
use throbber_widgets_tui::Throbber;
use throbber_widgets_tui::ThrobberState;

use crate::background_tasks::TaskResult;
use crate::background_tasks::TaskSlot;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::dialog::MessagePopup;
use crate::ui::utils::centered_rect_fixed;

/// A transient popup to be shown during possibly time consuming actions
///
/// The operation itself runs as a background task; the popup shows that
/// it is running and closes when its result arrives.
pub struct LoaderPopup {
    operation_name: String,
    /// The task slot this popup is waiting for
    slot: TaskSlot,
    /// What the output the operation had to say is to become, or None to
    /// put it up as a message
    on_output: Option<Box<dyn FnOnce(String) -> AppAction>>,
    throbber_state: ThrobberState,
    last_animation_update: Instant,
}

impl LoaderPopup {
    /// Create a new loader popup for the operation running in `slot`
    pub fn new(operation_name: String, slot: TaskSlot) -> Self {
        Self {
            operation_name,
            slot,
            on_output: None,
            throbber_state: ThrobberState::default(),
            last_animation_update: Instant::now(),
        }
    }

    /// Hand the output to `on_output` rather than showing it, which an
    /// operation run to be reported on rather than for its own sake asks
    /// for.
    pub fn on_output(mut self, on_output: impl FnOnce(String) -> AppAction + 'static) -> Self {
        self.on_output = Some(Box::new(on_output));
        self
    }
}

impl Component for LoaderPopup {
    fn wants_tick(&self) -> bool {
        true
    }

    /// Advance the animation
    fn update(&mut self) -> Result<Option<AppAction>> {
        if self.last_animation_update.elapsed() >= Duration::from_millis(100) {
            self.throbber_state.calc_next();
            self.last_animation_update = Instant::now();
        }

        Ok(None)
    }

    /// Close the popup, showing whatever the operation had to say. In
    /// case of an error, that is displayed in a new popup.
    fn task_done(&mut self, result: TaskResult) -> Result<Option<AppAction>> {
        // Another operation's result says nothing about this one, and
        // closing on it would leave the popup's own task unattended.
        if result.slot != self.slot {
            return Ok(None);
        }

        let action = match result.output {
            // What the caller makes of the output stands on its own: an
            // operation reported on rather than run for its own sake has
            // left the tabs as they were.
            Ok(output) if self.on_output.is_some() => {
                let on_output = self.on_output.take().expect("the output is asked for");
                on_output(output)
            }
            Ok(output) if !output.is_empty() => AppAction::Multiple(vec![
                AppAction::SetPopup(Box::new(MessagePopup::new(
                    format!("{} message", self.operation_name),
                    output,
                ))),
                AppAction::MarkTabsStale,
            ]),
            Ok(_) => AppAction::Multiple(vec![AppAction::ClosePopup, AppAction::MarkTabsStale]),
            Err(err) => AppAction::SetPopup(Box::new(MessagePopup::new(
                format!("{} error", self.operation_name),
                err.to_string(),
            ))),
        };

        Ok(Some(action))
    }

    /// Render the popup
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green));

        let label = format!("{}...", self.operation_name);
        let content_width = 2 + label.len() as u16;
        let content_height = 1;

        let popup_width = content_width + 2;
        let popup_height = content_height + 2;

        let popup_area = centered_rect_fixed(area, popup_width, popup_height);
        f.render_widget(Clear, popup_area);
        f.render_widget(&block, popup_area);

        let inner = block.inner(popup_area);

        let throbber = Throbber::default().label(label).style(Style::default());
        f.render_stateful_widget(throbber, inner, &mut self.throbber_state);

        Ok(())
    }

    /// Process input
    ///
    /// As of now, all input is ignored as we don't supporting cancelling operations yet.
    fn input(&mut self, _event: Event) -> Result<ComponentInputResult> {
        // Block all input while loading
        Ok(ComponentInputResult::Handled)
    }
}
