/*! What the terminal says about the mouse, and what it does not.

A terminal reports every press on its own, so telling a double or a
triple click from the clicks it is made of is up to us. [Clicks] does
that counting, once, for every event on its way into the app, and hands
on a [Mouse] that says how many presses in a row it belongs to.
*/

use std::time::Duration;
use std::time::Instant;

use ratatui::crossterm::event::MouseButton;
use ratatui::crossterm::event::MouseEvent;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Position;

/// How long apart two presses are still one double click. Whoever acts
/// on a click rather than on the drag that may follow it has to let this
/// much go by before it can tell that no further click widens it.
pub const CLICK_PAUSE: Duration = Duration::from_millis(400);

/// What the mouse did, with the presses before it counted.
#[derive(Clone, Copy, Debug)]
pub struct Mouse {
    kind: MouseEventKind,
    position: Position,
    /// How many presses in a row the button is on: 1 for a click of its
    /// own, 2 for a double click, 3 for a triple click, and 0 for an
    /// event that is not about a button at all
    clicks: u8,
}

/// The press a mouse event belongs to, as far as counting goes.
#[derive(Clone, Copy)]
struct Press {
    /// Cell the button went down on
    at: Position,
    /// Button that went down
    button: MouseButton,
    /// When it went down
    when: Instant,
    /// How many presses in a row it was
    count: u8,
    /// Whether the mouse moved with the button down, which ends the run
    /// of clicks: what follows a drag is a press of its own
    dragged: bool,
}

/// Counts the presses that come on one cell, close enough together in
/// time, to be one double or triple click. A fourth press starts over.
#[derive(Default)]
pub struct Clicks {
    press: Option<Press>,
}

impl Mouse {
    /// A mouse event of a kind on a cell, as the `clicks`th press in a
    /// row, for a test that has no terminal event to hand.
    #[cfg(test)]
    pub fn new(kind: MouseEventKind, position: Position, clicks: u8) -> Self {
        Self {
            kind,
            position,
            clicks,
        }
    }

    pub fn kind(&self) -> MouseEventKind {
        self.kind
    }

    pub fn position(&self) -> Position {
        self.position
    }

    /// How many presses in a row the button this event is about is on.
    /// An event about no button, like the wheel turning, is on none.
    pub fn clicks(&self) -> u8 {
        self.clicks
    }

    /// What `event` says the mouse did, as the `clicks`th press in a row.
    fn of(event: MouseEvent, clicks: u8) -> Self {
        Self {
            kind: event.kind,
            position: Position::new(event.column, event.row),
            clicks,
        }
    }
}

impl Clicks {
    /// The event with the presses before it counted. What a press is the
    /// second or third of counts for the release and the drag that
    /// follow it as well, so that whoever acts on those knows what they
    /// are part of.
    pub fn count(&mut self, event: MouseEvent) -> Mouse {
        self.count_at(event, Instant::now())
    }

    /// The same, for a caller that says when the event came in.
    fn count_at(&mut self, event: MouseEvent, now: Instant) -> Mouse {
        let at = Position::new(event.column, event.row);
        let clicks = match event.kind {
            MouseEventKind::Down(button) => {
                let count = match self.press {
                    Some(press)
                        if press.button == button
                            && press.at == at
                            && now.duration_since(press.when) < CLICK_PAUSE
                            && press.count < 3
                            && !press.dragged =>
                    {
                        press.count + 1
                    }
                    _ => 1,
                };
                self.press = Some(Press {
                    at,
                    button,
                    when: now,
                    count,
                    dragged: false,
                });
                count
            }
            MouseEventKind::Drag(button) => {
                let Some(press) = self.press.filter(|press| press.button == button) else {
                    return Mouse::of(event, 0);
                };
                self.press = Some(Press {
                    dragged: true,
                    ..press
                });
                press.count
            }
            MouseEventKind::Up(button) => self
                .press
                .filter(|press| press.button == button)
                .map_or(0, |press| press.count),
            _ => 0,
        };

        Mouse::of(event, clicks)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyModifiers;

    use super::*;

    const AT: Position = Position { x: 4, y: 2 };

    fn event(kind: MouseEventKind, at: Position) -> MouseEvent {
        MouseEvent {
            kind,
            column: at.x,
            row: at.y,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn press(clicks: &mut Clicks, at: Position) -> u8 {
        let down = clicks.count(event(MouseEventKind::Down(MouseButton::Left), at));
        clicks.count(event(MouseEventKind::Up(MouseButton::Left), at));
        down.clicks()
    }

    #[test]
    fn presses_in_a_row_on_a_cell_are_one_click_after_another() {
        let mut clicks = Clicks::default();

        assert_eq!(press(&mut clicks, AT), 1);
        assert_eq!(press(&mut clicks, AT), 2);
        assert_eq!(press(&mut clicks, AT), 3);
    }

    /// Three clicks is as much as anything asks for, so the fourth is a
    /// new click rather than a fourth of a kind.
    #[test]
    fn a_fourth_press_starts_over() {
        let mut clicks = Clicks::default();

        for _ in 0..3 {
            press(&mut clicks, AT);
        }

        assert_eq!(press(&mut clicks, AT), 1);
    }

    /// Two presses with a pause between them are two clicks, however
    /// little the mouse moved between them.
    #[test]
    fn a_press_after_the_pause_is_a_click_of_its_own() {
        let mut clicks = Clicks::default();
        let when = Instant::now();
        clicks.count_at(event(MouseEventKind::Down(MouseButton::Left), AT), when);
        clicks.count_at(event(MouseEventKind::Up(MouseButton::Left), AT), when);

        let later = when + CLICK_PAUSE;
        let down = clicks.count_at(event(MouseEventKind::Down(MouseButton::Left), AT), later);

        assert_eq!(down.clicks(), 1);
    }

    #[test]
    fn a_press_elsewhere_is_a_click_of_its_own() {
        let mut clicks = Clicks::default();
        press(&mut clicks, AT);

        assert_eq!(press(&mut clicks, Position { x: 5, y: 2 }), 1);
    }

    #[test]
    fn a_press_of_another_button_is_a_click_of_its_own() {
        let mut clicks = Clicks::default();
        press(&mut clicks, AT);

        let right = clicks.count(event(MouseEventKind::Down(MouseButton::Right), AT));

        assert_eq!(right.clicks(), 1);
    }

    /// A drag is a gesture of its own, so what comes after it is the
    /// first press of whatever comes next rather than the third of what
    /// went before.
    #[test]
    fn a_press_after_a_drag_is_a_click_of_its_own() {
        let mut clicks = Clicks::default();
        press(&mut clicks, AT);
        clicks.count(event(MouseEventKind::Down(MouseButton::Left), AT));
        clicks.count(event(MouseEventKind::Drag(MouseButton::Left), AT));
        clicks.count(event(MouseEventKind::Up(MouseButton::Left), AT));

        assert_eq!(press(&mut clicks, AT), 1);
    }

    /// The release belongs to the press it ends, so what acts on it
    /// knows whether it ends a double click.
    #[test]
    fn a_release_is_part_of_the_press_it_ends() {
        let mut clicks = Clicks::default();
        press(&mut clicks, AT);
        clicks.count(event(MouseEventKind::Down(MouseButton::Left), AT));

        let up = clicks.count(event(MouseEventKind::Up(MouseButton::Left), AT));
        let drag = clicks.count(event(MouseEventKind::Drag(MouseButton::Left), AT));

        assert_eq!(up.clicks(), 2);
        assert_eq!(drag.clicks(), 2);
    }

    #[test]
    fn the_wheel_is_about_no_button() {
        let mut clicks = Clicks::default();
        press(&mut clicks, AT);

        let wheel = clicks.count(event(MouseEventKind::ScrollDown, AT));

        assert_eq!(wheel.clicks(), 0);
    }
}
