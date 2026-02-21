//! The loader popup presents a cute little animation and an operation name and should be used for
//! operations known to possibly take some time.

use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::mpsc::{self};
use std::thread;
use std::time::Duration;
use std::time::Instant;

use ansi_to_tui::IntoText;
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

use crate::ComponentInputResult;
use crate::commander::CommandError;
use crate::commander::Commander;
use crate::ui::Component;
use crate::ui::ComponentAction;
use crate::ui::message_popup::MessagePopup;
use crate::ui::utils::centered_rect_fixed;

type OperationResult = Result<String, CommandError>;

/// A transient popup to be shown during possibly time consuming actions
pub struct LoaderPopup {
    operation_name: String,
    result_rx: Receiver<OperationResult>,
    throbber_state: ThrobberState,
    last_animation_update: Instant,
}

impl LoaderPopup {
    /// Create a new loader popup for the given operation
    ///
    /// The operation is started immediately and runs in a background thread.
    pub fn new<F>(operation_name: String, operation: F) -> Self
    where
        F: FnOnce() -> OperationResult + Send + 'static,
    {
        let (tx, rx): (Sender<OperationResult>, Receiver<OperationResult>) = mpsc::channel();

        // Spawn thread to run the operation
        thread::spawn(move || {
            let result = operation();
            tx.send(result)
        });

        Self {
            operation_name,
            result_rx: rx,
            throbber_state: ThrobberState::default(),
            last_animation_update: Instant::now(),
        }
    }
}

impl Component for LoaderPopup {
    /// Update the state of the popup
    ///
    /// This updates the animation and also polls the running operation to see if the popup may be
    /// closed. In case of an error, that will be displayed in a new popup.
    fn update(&mut self, _commander: &mut Commander) -> Result<Option<ComponentAction>> {
        if self.last_animation_update.elapsed() >= Duration::from_millis(100) {
            self.throbber_state.calc_next();
            self.last_animation_update = Instant::now();
        }

        let Ok(result) = self.result_rx.try_recv() else {
            return Ok(None);
        };

        let action = match result {
            Ok(output) if !output.is_empty() => ComponentAction::Multiple(vec![
                ComponentAction::SetPopup(Some(Box::new(MessagePopup {
                    title: format!("{} message", self.operation_name).into(),
                    messages: output.into_text()?,
                    text_align: None,
                }))),
                ComponentAction::RefreshTab(),
            ]),
            Ok(_) => ComponentAction::Multiple(vec![
                ComponentAction::SetPopup(None),
                ComponentAction::RefreshTab(),
            ]),
            Err(err) => ComponentAction::SetPopup(Some(Box::new(MessagePopup {
                title: format!("{} error", self.operation_name).into(),
                messages: err.into_text("")?,
                text_align: None,
            }))),
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
    fn input(&mut self, _commander: &mut Commander, _event: Event) -> Result<ComponentInputResult> {
        // Block all input while loading
        Ok(ComponentInputResult::Handled)
    }
}
