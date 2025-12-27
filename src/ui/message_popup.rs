use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use tui_confirm_dialog::PopupMessage;

use crate::ComponentInputResult;
use crate::commander::Commander;
use crate::ui::Component;

pub struct MessagePopup<'a> {
    pub title: Line<'a>,
    pub messages: Text<'a>,
    pub text_align: Option<Alignment>,
}

impl Component for MessagePopup<'_> {
    /// Render the parent into the area.
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let mut title = self.title.clone();
        title.spans = [vec![Span::raw(" ")], title.spans, vec![Span::raw(" ")]].concat();

        title = title.fg(Color::Cyan).bold();

        let text_align = match self.text_align {
            Some(align) => align,
            None => Alignment::Center,
        };

        // TODO: Support scrolling long messages
        let popup = PopupMessage::new(title, self.messages.clone())
            .title_alignment(Alignment::Center)
            .text_alignment(text_align)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Green));

        f.render_widget(popup, area);

        Ok(())
    }

    fn input(&mut self, _commander: &mut Commander, _event: Event) -> Result<ComponentInputResult> {
        Ok(ComponentInputResult::NotHandled)
    }
}
