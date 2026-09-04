/*! The context menus tab lists what every menu can hold in the main
panel and what the selected item is in the details panel.

It is the settings tab's, opened from the row for
`blazingjj.context-menu` and left again for it, so it has no place of
its own in the tab bar. What it writes are the keys under that table, in
the user's own config file, just as the settings tab writes the options
beside it.

A menu is one key holding the whole of it in order, so what the tab
writes is every item the menu holds every time. A menu written out with
`defaults` in it is written back with those spelled out, that being what
the tab has to put an item between them.
*/

use anyhow::Result;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyEventKind;
use ratatui::prelude::*;
use ratatui::widgets::*;
use tracing::instrument;

use crate::app::TabId;
use crate::app::command::Command;
use crate::commander::config::config_value;
use crate::commander::new_commander;
use crate::env::JjConfig;
use crate::env::get_env;
use crate::event::Mouse;
use crate::keybinds::Binding;
use crate::keybinds::MenusTabEvent;
use crate::keybinds::MenusTabKeybinds;
use crate::menus::Menu;
use crate::menus::held_by;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::ChoicePopup;
use crate::ui::panel::ListPane;
use crate::ui::panel::MouseInput;
use crate::ui::panel::PanelMouseInput;
use crate::ui::panel::Row as SectionRow;
use crate::ui::panel::Sections;
use crate::ui::panel::copy_marked;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::error_text;

/// What every row of the list is indented by, headings apart.
const INDENT: &str = "   ";

/// What marks the place of an item the menu does not hold, where the
/// ones it holds are numbered.
const NOT_HELD: &str = "-";

/// One row of the list: an item one menu can hold, and where in that
/// menu it sits.
struct Listed {
    menu: Menu,
    id: String,
    /// Where in the menu it sits, counting from zero, or None when the
    /// menu does not hold it.
    place: Option<usize>,
    /// What it does, as the keybinding running it or the command it
    /// names says.
    what: String,
    /// Whether it names a command of your own rather than one of the
    /// app's own items.
    is_yours: bool,
}

/// What the menus hold, as the configuration has them.
#[derive(Default)]
struct Menus {
    /// The items under the menu they belong to.
    rows: Sections<Listed>,
    /// What every menu holds, in order, which is what the tab rewrites.
    held: Vec<(Menu, Vec<String>)>,
    /// The menus the user's own config file is what sets, which are the
    /// only ones the tab can take back out.
    users: Vec<Menu>,
}

impl Menus {
    /// Every item every menu can hold, gathered under the menu, the ones
    /// it holds first and in the order it holds them.
    fn read(config: &JjConfig, user: &toml::Table) -> Self {
        let held: Vec<(Menu, Vec<String>)> = Menu::ALL
            .into_iter()
            .map(|menu| (menu, held_by(config, menu)))
            .collect();
        let users = Menu::ALL
            .into_iter()
            .filter(|menu| config_value(user, &key_of(*menu)).is_some())
            .collect();

        let listed = held.iter().flat_map(|(menu, ids)| {
            // What the menu holds comes first, in the order it holds it,
            // and what it could hold after that: the list is read to see
            // what a menu is, and only then to change it.
            let rest = candidates(config, *menu)
                .into_iter()
                .filter(|id| !ids.contains(id));

            ids.iter()
                .cloned()
                .chain(rest)
                .map(|id| Listed {
                    menu: *menu,
                    place: ids.iter().position(|held| *held == id),
                    what: what_it_does(config, *menu, &id),
                    is_yours: config.commands().get(&id).is_some(),
                    id,
                })
                .collect::<Vec<_>>()
        });

        Self {
            rows: Sections::new(listed, |listed| listed.menu.context().title()),
            held,
            users,
        }
    }

    fn selected(&self) -> Option<&Listed> {
        self.rows.selected()
    }

    /// What `menu` holds, in order.
    fn held(&self, menu: Menu) -> &[String] {
        self.held
            .iter()
            .find(|(held, _)| *held == menu)
            .map_or(&[], |(_, ids)| ids.as_slice())
    }

    /// Whether the user's own config file is what sets `menu`, which is
    /// what makes it the tab's to take back out.
    fn is_users(&self, menu: Menu) -> bool {
        self.users.contains(&menu)
    }
}

/// The config key `menu` is configured under.
fn key_of(menu: Menu) -> String {
    format!("blazingjj.context-menu.{}", menu.key())
}

/// Every item `menu` could hold: the app's own, and the commands of your
/// own, which go in a menu by being listed in it.
fn candidates(config: &JjConfig, menu: Menu) -> Vec<String> {
    menu.items()
        .iter()
        .map(|id| (*id).to_owned())
        .chain(config.commands().iter().map(|(name, _)| name.clone()))
        .collect()
}

/// What the item `id` does in `menu`: what the keybinding running the
/// same action says, or what a command of your own is called.
fn what_it_does(config: &JjConfig, menu: Menu, id: &str) -> String {
    if let Some(command) = config.commands().get(id) {
        return command.label(id);
    }

    menu.context()
        .bindings()
        .into_iter()
        .find(|binding| binding.name == Some(id))
        .map_or_else(
            || "no such action".to_owned(),
            |binding| binding.description.to_owned(),
        )
}

pub struct MenusTab {
    /// What the menus hold, or why the configuration could not be read.
    menus: Result<Menus>,

    menus_pane: ListPane,
    menus_list_state: ListState,

    keybinds: MenusTabKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

impl MenusTab {
    /// A stale tab, holding nothing of what the menus hold yet.
    #[instrument(level = "info", name = "Initializing context menus tab", parent = None)]
    pub fn new() -> Self {
        Self {
            menus: Ok(Menus::default()),

            menus_pane: ListPane::default(),
            menus_list_state: ListState::default(),

            keybinds: MenusTabKeybinds::new(),
            pane_divider: PaneDivider::default(),

            stale: true,
        }
    }

    fn selected(&self) -> Option<&Listed> {
        self.menus.as_ref().ok()?.selected()
    }

    /// Which row is selected, of the headings and the items alike.
    fn selected_row(&self) -> usize {
        self.menus
            .as_ref()
            .map_or(0, |menus| menus.rows.selected_row())
    }

    fn scroll_items(&mut self, scroll: isize) {
        if let Ok(menus) = self.menus.as_mut() {
            menus.rows.scroll(scroll);
        }
    }

    fn select_row(&mut self, index: usize) {
        if let Ok(menus) = self.menus.as_mut() {
            menus.rows.select_row(index);
        }
    }

    /// Write the selected item's menu out as holding `ids`.
    fn write(&self, ids: Vec<String>) -> Option<AppAction> {
        let listed = self.selected()?;

        Some(AppAction::Run(Command::SetSetting {
            key: key_of(listed.menu),
            value: Menu::value_of(&ids),
        }))
    }

    /// Put the selected item in its menu, at the end, or take it out
    /// again.
    fn toggle(&self) -> Option<AppAction> {
        let menus = self.menus.as_ref().ok()?;
        let listed = self.selected()?;
        let mut ids = menus.held(listed.menu).to_vec();

        match listed.place {
            Some(place) => {
                ids.remove(place);
            }
            None => ids.push(listed.id.clone()),
        }

        self.write(ids)
    }

    /// Move the selected item `by` places along its menu, which an item
    /// the menu does not hold has no place in and one at the end it is
    /// going towards has nowhere to go.
    fn move_by(&self, by: isize) -> Option<AppAction> {
        let menus = self.menus.as_ref().ok()?;
        let listed = self.selected()?;
        let place = listed.place?;
        let mut ids = menus.held(listed.menu).to_vec();

        let to = place.checked_add_signed(by).filter(|to| *to < ids.len())?;
        ids.swap(place, to);

        self.write(ids)
    }

    /// Take the selected item's menu out of the user's config file,
    /// leaving it holding whatever the rest of the configuration says.
    fn unset(&self) -> Option<AppAction> {
        let menus = self.menus.as_ref().ok()?;
        let listed = self.selected()?;

        menus.is_users(listed.menu).then(|| {
            AppAction::Run(Command::UnsetSetting {
                key: key_of(listed.menu),
            })
        })
    }

    /// The menu of what can be done to the selected item, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> Option<AppAction> {
        let listed = self.selected()?;
        let says = if listed.place.is_some() {
            "Take out of the menu"
        } else {
            "Put in the menu"
        };

        let mut items = vec![(Line::raw(says), self.toggle()?)];
        if let Some(up) = self.move_by(-1) {
            items.push((Line::raw("Move up the menu"), up));
        }
        if let Some(down) = self.move_by(1) {
            items.push((Line::raw("Move down the menu"), down));
        }
        if let Some(unset) = self.unset() {
            items.push((Line::raw("Take the whole menu out of your config"), unset));
        }

        Some(AppAction::SetPopup(Box::new(ChoicePopup::new(
            get_env().jj_config.clone(),
            anchor,
            "Menu item actions",
            items,
        ))))
    }

    fn handle_event(&mut self, event: MenusTabEvent) -> Option<AppAction> {
        match event {
            MenusTabEvent::Toggle => self.toggle(),
            MenusTabEvent::MoveUp => self.move_by(-1),
            MenusTabEvent::MoveDown => self.move_by(1),
            MenusTabEvent::Unset => self.unset(),
            MenusTabEvent::Back => Some(AppAction::ViewTab(TabId::Settings)),
            // Not an operation of its own; the key handler deals with it.
            MenusTabEvent::Unbound => None,
        }
    }

    /// One row per item: where in its menu it sits, what it is named by
    /// and what it does, under the heading of the menu it belongs to, in
    /// a panel `width` columns wide.
    fn item_lines(&self, menus: &Menus, width: u16) -> Vec<Line<'static>> {
        let id_width = menus
            .rows
            .rows()
            .iter()
            .filter_map(|row| match row {
                SectionRow::Item(listed) => Some(listed.id.chars().count()),
                SectionRow::Heading(_) => None,
            })
            .max()
            .unwrap_or(0);
        // What an item does is what the list is read by, so it takes
        // what is left of the row once the places and the names have
        // their columns.
        let what_width = (width as usize).saturating_sub(INDENT.len() + 4 + id_width + 2);

        menus
            .rows
            .rows()
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let line = match row {
                    // The indent is no part of the heading, so it is no
                    // part of what is underlined either.
                    SectionRow::Heading(heading) => Line::from(vec![
                        Span::raw(" "),
                        Span::raw(*heading).bold().underlined(),
                    ]),
                    SectionRow::Item(listed) => {
                        let place = match listed.place {
                            Some(place) => format!("{:>2}. ", place + 1),
                            None => format!("{NOT_HELD:>2}  "),
                        };
                        let what: String = listed.what.chars().take(what_width).collect();
                        let id = Span::raw(format!("{:id_width$}  ", listed.id));

                        Line::from(vec![
                            Span::raw(format!("{INDENT}{place}")).fg(Color::DarkGray),
                            // An item the menu does not hold is one that
                            // is not there to be picked, whatever it is.
                            if listed.place.is_some() {
                                id
                            } else {
                                id.fg(Color::DarkGray)
                            },
                            Span::raw(what).fg(Color::DarkGray),
                        ])
                    }
                };

                if index == menus.rows.selected_row() {
                    line.bg(get_env().jj_config.highlight_color())
                } else {
                    line
                }
            })
            .collect()
    }

    /// What the selected item is, which menu holds it and where that
    /// comes from.
    fn details_text(&self, menus: &Menus) -> Text<'static> {
        let Some(listed) = self.selected() else {
            return Text::default();
        };

        // A menu the app did not come with and your own config does not
        // hold comes from a layer of the configuration the tab does not
        // write, the repo's among them.
        let source = if menus.is_users(listed.menu) {
            "  (in your config)"
        } else if menus.held(listed.menu) == listed.menu.items() {
            "  (default)"
        } else {
            "  (elsewhere in your configuration)"
        };
        let place = match listed.place {
            Some(place) => format!("{} of {}", place + 1, menus.held(listed.menu).len()),
            None => "the menu does not hold it".to_owned(),
        };
        let comes_from = if listed.is_yours {
            "a command of your own"
        } else {
            "an action the app has"
        };

        Text::from(vec![
            Line::raw(listed.what.clone()).bold(),
            Line::raw(""),
            Line::raw(format!("Named by:       {}", listed.id)),
            Line::raw(format!("It is:          {comes_from}")),
            Line::raw(format!("In the menu:    {place}")),
            Line::raw(""),
            Line::from(vec![
                Span::raw(format!("Menu:           {}", listed.menu.context().title())),
                Span::raw(source).fg(Color::DarkGray),
            ]),
            Line::raw(format!("Configured as:  {}", key_of(listed.menu))),
        ])
    }
}

impl Tab for MenusTab {
    fn refresh(&mut self) -> Result<()> {
        // Reading the menus again is what a change to one asks for, and
        // what was changed is what is selected, so the list comes back
        // with the selection where it was left.
        let selected = self.selected_row();
        self.menus = new_commander()
            .get_user_config()
            .map(|user| Menus::read(&get_env().jj_config, &user));
        if let Ok(menus) = self.menus.as_mut() {
            menus.rows.select_row(selected);
        }
        self.stale = false;

        Ok(())
    }

    /// What the tab shows is the configuration, which a repo that has
    /// moved says nothing about.
    fn mark_stale(&mut self) {}

    fn is_stale(&self) -> bool {
        self.stale
    }

    fn config_changed(&mut self) {
        self.stale = true;
        self.keybinds = MenusTabKeybinds::new();
    }

    fn toggle_layout(&mut self) {
        self.pane_divider.toggle_layout();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        self.scroll_items(scroll.distance(self.menus_pane.visible_items()));

        Ok(())
    }

    fn open_context_menu(&self) -> Result<Option<AppAction>> {
        Ok(self.context_menu(self.menus_pane.item_anchor(self.selected_row(), 1)))
    }

    fn main_panel_bindings(&self) -> Vec<Binding> {
        self.keybinds.bindings()
    }

    /// The details panel only says what the selected item is, so there
    /// is nothing to do to it.
    fn details_panel_bindings(&self) -> Vec<Binding> {
        Vec::new()
    }
}

impl Component for MenusTab {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = self.pane_divider.split(area);

        let (rows, details) = match self.menus.as_ref() {
            Ok(menus) => (
                self.item_lines(menus, chunks[0].width.saturating_sub(2)),
                self.details_text(menus),
            ),
            Err(err) => (
                error_text("Error getting the configuration", err)?.lines,
                Text::default(),
            ),
        };

        // The hint goes between the corners, with a space to either side.
        let hint_width = chunks[0].width.saturating_sub(4) as usize;
        let block = Block::bordered()
            .title(" Settings / Context menus ")
            .title_bottom(
                Line::raw(format!(" {} ", self.keybinds.hint(hint_width)))
                    .centered()
                    .fg(Color::DarkGray),
            )
            .border_type(BorderType::Rounded);
        *self.menus_list_state.selected_mut() = Some(self.selected_row());
        self.menus_pane.render(
            f,
            chunks[0],
            block,
            List::new(rows).scroll_padding(3),
            &mut self.menus_list_state,
        );

        f.render_widget(
            Paragraph::new(details).wrap(Wrap { trim: false }).block(
                Block::bordered()
                    .title(" About ")
                    .border_type(BorderType::Rounded)
                    .padding(Padding::horizontal(1)),
            ),
            chunks[1],
        );

        Ok(())
    }

    fn input(&mut self, event: Event) -> Result<ComponentInputResult> {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                return Ok(ComponentInputResult::Handled);
            }

            return match self.keybinds.match_event(key) {
                // Not the tab's to act on, so whoever else wants the key
                // is welcome to it.
                MenusTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event).into()),
            };
        }

        Ok(ComponentInputResult::Handled)
    }

    fn input_mouse(&mut self, mouse: Mouse) -> Result<ComponentInputResult> {
        if self.pane_divider.handle_mouse(mouse) {
            return Ok(ComponentInputResult::Handled);
        }
        match self.menus_pane.input_mouse(mouse) {
            MouseInput::Scroll(delta) => self.scroll_items(delta),
            MouseInput::Select(index) => self.select_row(index),
            MouseInput::Context(index) => {
                self.select_row(index);
                return Ok(self.context_menu(Some(mouse.position())).into());
            }
            MouseInput::Copy(text) => return Ok(copy_marked(text)),
            MouseInput::Activate => return Ok(self.toggle().into()),
            MouseInput::Handled => {}
            MouseInput::NotHandled => return Ok(ComponentInputResult::NotHandled),
        }
        Ok(ComponentInputResult::Handled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_test_env;
    use crate::ui::utils::drawn;

    /// A files menu holding only what is restored, and a command of your
    /// own that no menu holds
    const CONFIG: &str = "blazingjj.context-menu.files = [\"restore\"]\n\
                          blazingjj.commands.blame = [\"jj\", \"file\", \"annotate\", \"$file\"]\n";

    /// A tab holding the menus `config` has, all of them the user's own,
    /// which the tests have in place of a repo to read from.
    fn tab(config: &str) -> MenusTab {
        set_test_env();
        let mut tab = MenusTab::new();
        tab.menus = Ok(Menus::read(
            &toml::from_str(config).expect("the configuration parses"),
            &config.parse().expect("the configuration parses"),
        ));
        tab
    }

    /// What the main panel says, as one string per row.
    fn rows(tab: &MenusTab) -> Vec<String> {
        let menus = tab.menus.as_ref().expect("the configuration was read");

        tab.item_lines(menus, 100)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// The rows of the files menu, which is the second heading and the
    /// rows under it.
    fn files_rows(tab: &MenusTab) -> Vec<String> {
        rows(tab)
            .into_iter()
            .skip_while(|row| !row.contains("Files tab"))
            .skip(1)
            .take_while(|row| !row.contains("tab"))
            .collect()
    }

    /// Where `key` is set to, for a tab whose selection was just acted
    /// on.
    fn written(action: Option<AppAction>) -> Option<(String, String)> {
        match action? {
            AppAction::Run(Command::SetSetting { key, value }) => Some((key, value)),
            _ => None,
        }
    }

    /// Every menu is a heading with what it can hold under it, so that
    /// an item is found by the menu it belongs to.
    #[test]
    fn every_menu_is_a_heading_of_its_own() {
        let rows = rows(&tab(CONFIG));

        for heading in ["Log tab", "Files tab", "Bookmarks tab", "Evolog tab"] {
            assert!(
                rows.iter()
                    .any(|row| row.trim_end() == format!(" {heading}")),
                "{heading} is no heading: {rows:?}"
            );
        }
    }

    /// What a menu holds comes first and numbered, so that the order it
    /// holds it in is what the list says; what it could hold follows.
    #[test]
    fn what_a_menu_holds_is_listed_first_and_in_order() {
        let rows = files_rows(&tab(CONFIG));

        assert!(rows[0].starts_with("    1. restore"), "{rows:?}");
        assert!(rows[1].starts_with("    -  open"), "{rows:?}");
        assert!(rows[2].starts_with("    -  untrack"), "{rows:?}");
    }

    /// A command of your own is an item every menu could hold, or there
    /// would be no putting one in a menu from here.
    #[test]
    fn a_command_of_your_own_is_an_item_a_menu_could_hold() {
        let rows = files_rows(&tab(CONFIG));

        assert!(rows.iter().any(|row| row.contains("blame")), "{rows:?}");
    }

    /// An item says what it does, which is what the keybinding running
    /// the same action says, so that the list reads as more than ids.
    #[test]
    fn an_item_says_what_it_does() {
        let rows = files_rows(&tab(CONFIG));

        assert!(rows[0].contains("restore file"), "{rows:?}");
    }

    /// Putting an item in a menu writes the whole menu out with it at
    /// the end, that being the one key holding all of it.
    #[test]
    fn putting_an_item_in_a_menu_writes_the_whole_menu() {
        let mut tab = tab(CONFIG);
        // Past the log menu, onto the files menu's second item, which
        // is the one it does not hold.
        while !tab
            .selected()
            .is_some_and(|listed| listed.menu == Menu::Files && listed.id == "untrack")
        {
            tab.scroll_items(1);
        }

        assert_eq!(
            written(tab.toggle()),
            Some((
                "blazingjj.context-menu.files".to_owned(),
                r#"["restore", "untrack"]"#.to_owned()
            ))
        );
    }

    #[test]
    fn taking_an_item_out_of_a_menu_writes_the_whole_menu() {
        let mut tab = tab(CONFIG);
        while !tab
            .selected()
            .is_some_and(|listed| listed.menu == Menu::Files && listed.id == "restore")
        {
            tab.scroll_items(1);
        }

        assert_eq!(
            written(tab.toggle()),
            Some(("blazingjj.context-menu.files".to_owned(), "[]".to_owned()))
        );
    }

    /// A menu the configuration says nothing about is written out as
    /// every item it comes holding, with the one that moved moved.
    #[test]
    fn moving_an_item_writes_the_menu_as_the_app_comes_with_it() {
        let mut tab = tab(CONFIG);
        while !tab
            .selected()
            .is_some_and(|listed| listed.menu == Menu::Evolog && listed.id == "duplicate")
        {
            tab.scroll_items(1);
        }

        assert_eq!(
            written(tab.move_by(-1)),
            Some((
                "blazingjj.context-menu.evolog".to_owned(),
                r#"["duplicate", "open-files", "copy-rev"]"#.to_owned()
            ))
        );
    }

    /// An item at the end of a menu has nowhere further to go, and one
    /// the menu does not hold has no place in it to move at all.
    #[test]
    fn an_item_with_nowhere_to_go_does_not_move() {
        let mut tab = tab(CONFIG);
        while !tab
            .selected()
            .is_some_and(|listed| listed.menu == Menu::Files && listed.id == "restore")
        {
            tab.scroll_items(1);
        }

        assert!(tab.move_by(-1).is_none(), "the first item cannot move up");
        assert!(
            tab.move_by(1).is_none(),
            "the only item cannot move down either"
        );

        tab.scroll_items(1);
        assert!(
            tab.move_by(1).is_none(),
            "an item the menu does not hold has no place to move"
        );
    }

    /// Only a menu the user's own config file sets is the tab's to take
    /// back out.
    #[test]
    fn a_menu_that_is_not_in_your_config_is_not_the_tabs_to_take_out() {
        let mut tab = tab(CONFIG);
        while !tab
            .selected()
            .is_some_and(|listed| listed.menu == Menu::Files)
        {
            tab.scroll_items(1);
        }
        assert!(tab.unset().is_some());

        // The log menu is nowhere in the configuration, so it holds what
        // the app comes with and there is nothing to take out.
        tab.scroll_items(-1000);
        assert!(tab.unset().is_none());
    }

    /// The details panel says what the selected item is and where the
    /// menu holding it comes from.
    #[test]
    fn the_details_panel_is_about_the_selected_item() {
        let screen = drawn(&mut tab(CONFIG), 120, 30);

        assert!(
            screen
                .iter()
                .any(|row| row.contains("blazingjj.context-menu.log")),
            "{screen:?}"
        );
        assert!(
            screen.iter().any(|row| row.contains("(default)")),
            "{screen:?}"
        );
    }

    /// The keys the tab answers to are worth saying where the list is,
    /// there being no other list of them in it.
    #[test]
    fn the_tab_says_what_it_answers_to() {
        let screen = drawn(&mut tab(CONFIG), 120, 30);

        assert!(
            screen
                .iter()
                .any(|row| row.contains("Enter: in or out") && row.contains("Esc: back")),
            "{screen:?}"
        );
    }

    /// A repo that has moved says nothing about the configuration, so
    /// reading it again is for a configuration that has changed.
    #[test]
    fn the_tab_reads_the_configuration_again_when_it_changes_rather_than_the_repo() {
        let mut tab = tab(CONFIG);
        tab.stale = false;

        tab.mark_stale();
        assert!(!tab.is_stale());

        tab.config_changed();
        assert!(tab.is_stale());
    }
}
