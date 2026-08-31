//! The mouse as the app takes it, rather than as the terminal reports it.

use ratatui::crossterm::event::MouseEventKind;
use ratatui::layout::Position;

/// What the mouse did, and the cell it did it on.
#[derive(Clone, Copy, Debug)]
pub struct Mouse {
    kind: MouseEventKind,
    position: Position,
}

impl Mouse {
    pub fn new(kind: MouseEventKind, position: Position) -> Self {
        Self { kind, position }
    }

    pub fn kind(&self) -> MouseEventKind {
        self.kind
    }

    pub fn position(&self) -> Position {
        self.position
    }
}
