use ratatui::Frame;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::MouseEventKind;
use ratatui::crossterm::event::{self};
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Row;
use ratatui::widgets::Table;

use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::styles::create_popup_block;
use crate::ui::utils::centered_rect_fixed;

/// Space between the description and the key column of a table
const COLUMN_SPACING: u16 = 4;

/// Width of the gap holding the vertical rule between two columns
const SEPARATOR_WIDTH: u16 = 3;

/// A titled list of keybindings shown in the popup
#[derive(Clone, Copy)]
struct Section<'a> {
    title: &'static str,
    items: &'a [(String, String)],
}

impl Section<'_> {
    /// Rows the section takes, the first of which holds its title
    fn height(&self) -> u16 {
        self.items.len() as u16 + 1
    }

    fn key_width(&self) -> u16 {
        self.items
            .iter()
            .map(|(key, _)| key.len())
            .max()
            .unwrap_or(0) as u16
    }

    fn description_width(&self) -> u16 {
        self.items
            .iter()
            .map(|(_, description)| description.len())
            .max()
            .unwrap_or(0) as u16
    }
}

/// Rows a column of `sections` takes, with a rule between them
fn stack_height(sections: &[Section]) -> u16 {
    sections.iter().map(Section::height).sum::<u16>() + sections.len().saturating_sub(1) as u16
}

/// The width of the key column shared by the `sections` of a column, and the
/// width of the column as a whole. Sharing the key column lines up the rows of
/// the sections stacked in it.
fn stack_width(sections: &[Section]) -> (u16, u16) {
    let key = sections.iter().map(Section::key_width).max().unwrap_or(0);
    let description = sections
        .iter()
        .map(Section::description_width)
        .max()
        .unwrap_or(0);

    (key, key + COLUMN_SPACING + description)
}

/// Width the `columns` take side by side, with a separator between them
fn contents_width(columns: &[Vec<Section>]) -> u16 {
    let columns_width: u16 = columns.iter().map(|column| stack_width(column).1).sum();

    columns_width + SEPARATOR_WIDTH * columns.len().saturating_sub(1) as u16
}

/// How to arrange the popup's sections in the `width` available for them: the
/// main panel bindings beside the details panel and global ones, or, when that
/// does not fit, all of them stacked in a single column, which only needs the
/// width of the widest of them.
fn columns<'a>([main, details, global]: [Section<'a>; 3], width: u16) -> Vec<Vec<Section<'a>>> {
    let halves = vec![vec![main], vec![details, global]];
    if contents_width(&halves) <= width {
        return halves;
    }

    vec![vec![main, details, global]]
}

/// How much wider and taller than its contents a `block` is, in the `area` it
/// has to draw itself in. Its border is not all of it, it also pads.
fn chrome(block: &Block, area: Rect) -> [u16; 2] {
    let inner = block.inner(area);

    [
        area.width.saturating_sub(inner.width),
        area.height.saturating_sub(inner.height),
    ]
}

/// Renders the `sections` stacked in `area`, with a rule between them and
/// `scroll` rows taken off the top of the stack as a whole.
fn render_stack(f: &mut Frame<'_>, area: Rect, scroll: u16, sections: &[Section]) {
    let (key_width, _) = stack_width(sections);
    let mut top = 0;

    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            if let Some((rule, _)) = place(area, scroll, top, 1) {
                f.render_widget(Block::new().borders(Borders::TOP), rule);
            }
            top += 1;
        }

        if let Some((table, skipped)) = place(area, scroll, top, section.height()) {
            f.render_widget(create_table(section, key_width, skipped), table);
        }
        top += section.height();
    }
}

/// The table of a `section`, with `skipped` of its rows taken off the top. Its
/// title is the first of those rows.
fn create_table<'a>(section: &Section<'a>, key_width: u16, skipped: u16) -> Table<'a> {
    let rows: Vec<Row> = section
        .items
        .iter()
        .skip(skipped.saturating_sub(1) as usize)
        .map(|(key, description)| Row::new([description.clone(), key.clone()]))
        .collect();
    let widths = [Constraint::Fill(1), Constraint::Length(key_width)];
    let table = Table::new(rows, widths).column_spacing(COLUMN_SPACING);

    if skipped > 0 {
        return table;
    }

    table.block(
        Block::new()
            .title(Span::from(section.title).bold().underlined())
            .title_alignment(Alignment::Center),
    )
}

/// Where a part of the popup's contents that is `height` rows tall and starts
/// at row `top` of them lands in `viewport`, and how many of its rows `scroll`
/// has taken off the top. `None` once it has scrolled out of view entirely.
fn place(viewport: Rect, scroll: u16, top: u16, height: u16) -> Option<(Rect, u16)> {
    let skipped = scroll.saturating_sub(top);
    let y = viewport.y + top.saturating_sub(scroll);
    let height = height
        .saturating_sub(skipped)
        .min(viewport.bottom().saturating_sub(y));

    (height > 0).then_some((
        Rect {
            y,
            height,
            ..viewport
        },
        skipped,
    ))
}

pub struct HelpPopup {
    pub main_items: Vec<(String, String)>,
    pub details_items: Vec<(String, String)>,
    pub global_items: Vec<(String, String)>,
    max_scroll: u16,
    scroll: u16,
}

impl HelpPopup {
    pub fn new(
        main_items: Vec<(String, String)>,
        details_items: Vec<(String, String)>,
        global_items: Vec<(String, String)>,
    ) -> Self {
        Self {
            main_items,
            details_items,
            global_items,
            max_scroll: 0,
            // Can't use TableState as it's broken: https://github.com/ratatui-org/ratatui/issues/1179
            scroll: 0,
        }
    }

    fn do_scroll(&mut self, delta: i32) {
        let max = i32::from(self.max_scroll);
        self.scroll = (i32::from(self.scroll) + delta).clamp(0, max) as u16;
    }
}

impl Component for HelpPopup {
    fn draw(
        &mut self,
        f: &mut ratatui::prelude::Frame<'_>,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        let block = create_popup_block("Help");
        let [extra_width, extra_height] = chrome(&block, area);

        let columns = columns(
            [
                Section {
                    title: "Main panel",
                    items: &self.main_items,
                },
                Section {
                    title: "Details panel",
                    items: &self.details_items,
                },
                Section {
                    title: "Global",
                    items: &self.global_items,
                },
            ],
            area.width.saturating_sub(extra_width),
        );

        let contents_height = columns
            .iter()
            .map(|column| stack_height(column))
            .max()
            .unwrap_or(0);
        let width = (contents_width(&columns) + extra_width).min(area.width);
        let height = (contents_height + extra_height).min(area.height);

        let area = centered_rect_fixed(area, width, height);
        f.render_widget(Clear, area);

        let block_inner = block.inner(area);
        f.render_widget(&block, area);

        self.max_scroll = contents_height.saturating_sub(block_inner.height);
        self.scroll = self.scroll.min(self.max_scroll);

        // A separator goes in front of every column but the first
        let constraints: Vec<Constraint> = columns
            .iter()
            .enumerate()
            .flat_map(|(index, column)| {
                let separator = (index > 0).then_some(Constraint::Length(SEPARATOR_WIDTH));
                separator
                    .into_iter()
                    .chain([Constraint::Length(stack_width(column).1)])
            })
            .collect();
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(block_inner);

        for (index, column) in columns.iter().enumerate() {
            let area = chunks[2 * index];
            if index > 0 {
                let separator = chunks[2 * index - 1];
                f.render_widget(
                    Block::new().borders(Borders::LEFT),
                    Rect {
                        x: separator.x + separator.width / 2,
                        ..separator
                    },
                );
            }

            render_stack(f, area, self.scroll, column);
        }

        Ok(())
    }

    fn input(&mut self, event: Event) -> anyhow::Result<ComponentInputResult> {
        match event {
            Event::Key(key) if key.kind == event::KeyEventKind::Press => {
                let delta = match key.code {
                    KeyCode::Char('j') => 1,
                    KeyCode::Char('k') => -1,
                    _ => return Ok(ComponentInputResult::NotHandled),
                };

                self.do_scroll(delta);
                Ok(ComponentInputResult::Handled)
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => {
                    self.do_scroll(3);
                    Ok(ComponentInputResult::Handled)
                }
                MouseEventKind::ScrollUp => {
                    self.do_scroll(-3);
                    Ok(ComponentInputResult::Handled)
                }
                _ => Ok(ComponentInputResult::NotHandled),
            },
            _ => Ok(ComponentInputResult::NotHandled),
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::KeyModifiers;
    use ratatui::crossterm::event::MouseEvent;

    use super::*;

    fn area(height: u16) -> Rect {
        Rect::new(0, 0, 40, height)
    }

    fn rect(y: u16, height: u16) -> Rect {
        Rect::new(0, y, 40, height)
    }

    /// A section whose rows are `key_width` and `description_width` wide
    fn section(items: usize, key_width: usize, description_width: usize) -> Vec<(String, String)> {
        vec![("k".repeat(key_width), "d".repeat(description_width)); items]
    }

    fn wheel(kind: MouseEventKind) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        })
    }

    #[test]
    fn test_the_halves_go_side_by_side_only_while_they_fit() {
        let main = section(2, 1, 10);
        let details = section(2, 2, 8);
        let global = section(3, 2, 6);
        let sections = [
            Section {
                title: "Main panel",
                items: &main,
            },
            Section {
                title: "Details panel",
                items: &details,
            },
            Section {
                title: "Global",
                items: &global,
            },
        ];

        // The main panel bindings need 15 columns, the other two 14 together
        let halves = columns(sections, 32);
        assert_eq!(halves.len(), 2);
        assert_eq!(contents_width(&halves), 32);

        // A column short of that, everything stacks up in a single column,
        // which only has to hold the widest keys next to the widest
        // description.
        let stacked = columns(sections, 31);
        assert_eq!(stacked.len(), 1);
        assert_eq!(contents_width(&stacked), 16);
        // Every section spends a row on its title, with a rule between them
        assert_eq!(stack_height(&stacked[0]), 12);
    }

    #[test]
    fn test_unscrolled_contents_keep_their_place() {
        assert_eq!(place(area(10), 0, 0, 6), Some((rect(0, 6), 0)));
        // What does not fit is cut off at the bottom of the viewport.
        assert_eq!(place(area(10), 0, 8, 5), Some((rect(8, 2), 0)));
    }

    #[test]
    fn test_scrolling_takes_rows_off_the_top() {
        // The scroll eats into a part of the contents ...
        assert_eq!(place(area(10), 3, 0, 6), Some((rect(0, 3), 3)));
        // ... only once it has consumed everything above it.
        assert_eq!(place(area(10), 3, 5, 4), Some((rect(2, 4), 0)));
    }

    #[test]
    fn test_parts_scrolled_past_are_gone() {
        assert_eq!(place(area(10), 8, 0, 6), None);
    }

    #[test]
    fn test_scroll_is_clamped_to_max() {
        let mut popup = HelpPopup::new(vec![], vec![], vec![]);
        popup.max_scroll = 2;

        for _ in 0..5 {
            popup
                .input(Event::Key(KeyCode::Char('j').into()))
                .expect("scrolling down should be handled");
        }
        assert_eq!(popup.scroll, 2);

        for _ in 0..5 {
            popup
                .input(Event::Key(KeyCode::Char('k').into()))
                .expect("scrolling up should be handled");
        }
        assert_eq!(popup.scroll, 0);
    }

    #[test]
    fn test_the_wheel_scrolls_more_than_a_row_at_a_time() {
        let mut popup = HelpPopup::new(vec![], vec![], vec![]);
        popup.max_scroll = 10;

        popup
            .input(wheel(MouseEventKind::ScrollDown))
            .expect("scrolling down should be handled");
        assert_eq!(popup.scroll, 3);

        popup
            .input(wheel(MouseEventKind::ScrollUp))
            .expect("scrolling up should be handled");
        assert_eq!(popup.scroll, 0);
    }
}
