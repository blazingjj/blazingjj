/*! The rows of a list whose items are gathered under headings.

A heading is what the rows under it are read by, not something to act
on, so the selection passes over one rather than landing on it.
*/

/// A row of such a list.
pub enum Row<T> {
    Heading(&'static str),
    Item(T),
}

/// The items of a list under the headings they belong to, and which of
/// them is selected.
pub struct Sections<T> {
    rows: Vec<Row<T>>,
    selected: usize,
}

impl<T> Default for Sections<T> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            selected: 0,
        }
    }
}

impl<T> Sections<T> {
    /// The `items` under the heading `heading` gives each of them, of
    /// which the ones sharing a heading come one after another.
    pub fn new(items: impl IntoIterator<Item = T>, heading: impl Fn(&T) -> &'static str) -> Self {
        let mut rows: Vec<Row<T>> = Vec::new();
        let mut under = None;

        for item in items {
            let title = heading(&item);
            if under != Some(title) {
                under = Some(title);
                rows.push(Row::Heading(title));
            }
            rows.push(Row::Item(item));
        }

        let mut sections = Self { rows, selected: 0 };
        // The first row is a heading, so the first item is the one
        // under it.
        sections.scroll(0);

        sections
    }

    pub fn rows(&self) -> &[Row<T>] {
        &self.rows
    }

    /// Which row is selected, for drawing the list.
    pub fn selected_row(&self) -> usize {
        self.selected
    }

    pub fn selected(&self) -> Option<&T> {
        match self.rows.get(self.selected)? {
            Row::Item(item) => Some(item),
            Row::Heading(_) => None,
        }
    }

    /// Move the selection by `scroll` rows, over the headings rather
    /// than onto them.
    pub fn scroll(&mut self, scroll: isize) {
        let last = self.rows.len().saturating_sub(1);
        let landed = self.selected.saturating_add_signed(scroll).min(last);
        let item = |index: &usize| matches!(self.rows.get(*index), Some(Row::Item(_)));

        // A move that ends on a heading carries on the way it was
        // going, and turns back where there is nothing left that way.
        let onwards = (landed..=last).find(item);
        let backwards = (0..=landed).rev().find(item);
        self.selected = if scroll < 0 {
            backwards.or(onwards)
        } else {
            onwards.or(backwards)
        }
        .unwrap_or(self.selected);
    }

    /// Select the row at `index`, a heading leaving the selection where
    /// it is: there is nothing to do to one.
    pub fn select_row(&mut self, index: usize) {
        if matches!(self.rows.get(index), Some(Row::Item(_))) {
            self.selected = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sections() -> Sections<&'static str> {
        Sections::new(["j", "k", "d", "y"], |item| match *item {
            "j" | "k" => "Navigation",
            "d" => "Changes",
            _ => "Clipboard",
        })
    }

    #[test]
    fn every_run_of_items_comes_under_a_heading_of_its_own() {
        let headings: Vec<&str> = sections()
            .rows()
            .iter()
            .filter_map(|row| match row {
                Row::Heading(heading) => Some(*heading),
                Row::Item(_) => None,
            })
            .collect();

        assert_eq!(headings, ["Navigation", "Changes", "Clipboard"]);
    }

    #[test]
    fn the_selection_passes_over_the_headings_rather_than_onto_them() {
        let mut sections = sections();

        for _ in 0..sections.rows().len() {
            assert!(sections.selected().is_some(), "row {}", sections.selected);
            sections.scroll(1);
        }
        for _ in 0..sections.rows().len() {
            assert!(sections.selected().is_some(), "row {}", sections.selected);
            sections.scroll(-1);
        }
    }

    /// Going to either end lands on an item, of which the first is not
    /// the first row: a heading is.
    #[test]
    fn going_to_either_end_lands_on_an_item() {
        let mut sections = sections();

        sections.scroll(isize::MAX);
        assert_eq!(sections.selected(), Some(&"y"));

        sections.scroll(-isize::MAX);
        assert_eq!(sections.selected(), Some(&"j"));
    }

    #[test]
    fn a_heading_is_no_row_to_select() {
        let mut sections = sections();
        sections.select_row(2);
        assert_eq!(sections.selected(), Some(&"k"));

        sections.select_row(3);

        assert_eq!(sections.selected(), Some(&"k"));
    }
}
