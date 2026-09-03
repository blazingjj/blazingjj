/*! The context menus, and what the configuration puts in them.

Every item a menu can hold is named by an id, which is the name the
keybinding running the same action is configured under, so that there is
one name per action however it is reached. What a menu holds and the
order it holds it in is `blazingjj.context-menu`, which comes set to
every item the app has; `defaults` in place of an id stands for those,
so that a menu of your own does not have to spell them all out again.

An id naming an item the selection has nothing to offer is left out, the
way the menu leaves out what cannot be done to what is selected anyway.
An id naming nothing at all is listed under the menu, there being
nothing else to say about a name that is set but does not read.
*/

use ratatui::layout::Position;
use ratatui::style::Color;
use ratatui::style::Stylize;
use ratatui::text::Line;
use serde::Deserialize;

use crate::app::command::Command;
use crate::commands::CustomRun;
use crate::env::JjConfig;
use crate::selection::Selection;
use crate::ui::AppAction;
use crate::ui::dialog::Choice;
use crate::ui::dialog::ChoicePopup;

/// What stands for every item the app comes with, wherever an id is
/// listed.
const DEFAULTS: &str = "defaults";

/// One of the context menus, which is the tab whose selection it acts
/// on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Log,
    Files,
    Bookmarks,
    Evolog,
    OpLog,
}

impl Menu {
    /// The key under `blazingjj.context-menu` it is configured by.
    pub fn key(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::Files => "files",
            Self::Bookmarks => "bookmarks",
            Self::Evolog => "evolog",
            Self::OpLog => "op-log",
        }
    }

    /// The title the menu goes up under.
    pub fn title(self) -> &'static str {
        match self {
            Self::Log => "Actions",
            Self::Files => "File actions",
            Self::Bookmarks => "Bookmark actions",
            Self::Evolog => "Version actions",
            Self::OpLog => "Operation actions",
        }
    }

    /// Every item it can hold, in the order it comes holding them, which
    /// is also what `defaults` stands for.
    pub fn items(self) -> &'static [&'static str] {
        match self {
            Self::Log => &[
                "edit-change",
                "create-new",
                "create-new-describe",
                "describe",
                "absorb",
                "abandon",
                "duplicate",
                "squash",
                "rebase",
                "push-menu",
                "set-bookmark",
                "copy-change-id",
                "copy-rev",
            ],
            Self::Files => &["open", "restore", "untrack"],
            Self::Bookmarks => &[
                "create-bookmark",
                "rename-bookmark",
                "delete-bookmark",
                "forget-bookmark",
                "track-bookmark",
                "untrack-bookmark",
                "edit-change",
                "create-new",
                "create-new-describe",
                "view-in-log",
            ],
            Self::Evolog => &["open-files", "duplicate", "copy-rev"],
            Self::OpLog => &["restore", "revert", "copy-id"],
        }
    }
}

/// What the context menus hold, as the configuration says.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct ContextMenus {
    log: Option<Vec<String>>,
    files: Option<Vec<String>>,
    bookmarks: Option<Vec<String>>,
    evolog: Option<Vec<String>>,
    op_log: Option<Vec<String>>,
}

impl ContextMenus {
    /// What `menu` is configured to hold, or None while the
    /// configuration says nothing about it.
    pub fn of(&self, menu: Menu) -> Option<&[String]> {
        match menu {
            Menu::Log => &self.log,
            Menu::Files => &self.files,
            Menu::Bookmarks => &self.bookmarks,
            Menu::Evolog => &self.evolog,
            Menu::OpLog => &self.op_log,
        }
        .as_deref()
    }
}

/// One item a menu can hold: the id it is named by, what it says on
/// screen and what picking it asks for.
pub struct Item {
    id: &'static str,
    label: Line<'static>,
    action: AppAction,
}

impl Item {
    pub fn new(id: &'static str, label: impl Into<Line<'static>>, action: AppAction) -> Self {
        Self {
            id,
            label: label.into(),
            action,
        }
    }
}

/// The ids `menu` is to hold, with `defaults` expanded to every item the
/// app comes with. An id listed more than once is held where it is
/// first asked for, a menu holding nothing twice.
fn ordered(menu: Menu, configured: Option<&[String]>) -> Vec<&str> {
    let Some(configured) = configured else {
        return menu.items().to_vec();
    };

    let mut ids: Vec<&str> = Vec::new();
    for id in configured.iter().flat_map(|id| {
        if id == DEFAULTS {
            menu.items().to_vec()
        } else {
            vec![id.as_str()]
        }
    }) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    ids
}

/// The `menu` of what can be done to what a tab has selected, holding
/// what the configuration puts in it and put at `anchor` or centered
/// when there is nowhere to point at.
///
/// `items` are the items the selection has to offer, of which the ones
/// the configuration asks for are listed, in the order it asks for
/// them. An id that names none of them names a command of your own,
/// which is run against `selection`.
pub fn context_menu(
    config: &JjConfig,
    anchor: Option<Position>,
    menu: Menu,
    selection: &Selection,
    items: Vec<Item>,
) -> ChoicePopup {
    let mut items = items;
    let mut choices = Vec::new();
    let mut unknown = Vec::new();

    for id in ordered(menu, config.context_menu().of(menu)) {
        if let Some(index) = items.iter().position(|item| item.id == id) {
            let item = items.remove(index);
            choices.push(Choice::new(item.label, item.action));
            continue;
        }

        // The app's own items come first, so a command of your own
        // cannot take the name of one and be picked in its place.
        match config.commands().get(id) {
            Some(command) => choices.push(Choice::new(
                Line::raw(command.label(id)),
                AppAction::Run(Command::RunCustom(Box::new(CustomRun {
                    name: id.to_owned(),
                    command: command.clone(),
                    selection: selection.clone(),
                }))),
            )),
            // An id the menu has an item for is one the selection has
            // nothing to offer, which is nothing to say anything about;
            // one it has no item for at all does not read.
            None if !menu.items().contains(&id) => unknown.push(id.to_owned()),
            None => {}
        }
    }

    ChoicePopup::new(config.clone(), anchor, menu.title(), choices).footnote(
        unknown
            .into_iter()
            .map(|id| {
                Line::raw(format!(
                    "blazingjj.context-menu.{}: no action named {id}",
                    menu.key()
                ))
                .fg(Color::Red)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander::ids::ChangeId;
    use crate::commander::ids::CommitId;
    use crate::commander::log::Head;
    use crate::env::set_test_env;

    /// Every menu there is, for the tests that hold for all of them.
    const ALL: [Menu; 5] = [
        Menu::Log,
        Menu::Files,
        Menu::Bookmarks,
        Menu::Evolog,
        Menu::OpLog,
    ];

    /// An item naming itself, so that a test can tell which ones a menu
    /// holds and in which order.
    fn item(id: &'static str) -> Item {
        Item::new(
            id,
            Line::raw(id),
            AppAction::ViewLog(Head {
                change_id: ChangeId(id.to_owned()),
                commit_id: CommitId(id.to_owned()),
                divergent: false,
                immutable: false,
            }),
        )
    }

    /// The ids `configured` puts in the files menu
    fn ids(configured: &[&str]) -> Vec<String> {
        let configured: Vec<String> = configured.iter().map(|id| (*id).to_owned()).collect();

        ordered(Menu::Files, Some(&configured))
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn a_menu_the_configuration_says_nothing_about_holds_every_item() {
        assert_eq!(ordered(Menu::Files, None), ["open", "restore", "untrack"]);
    }

    #[test]
    fn a_configured_menu_holds_what_it_lists_in_the_order_it_lists_it() {
        assert_eq!(ids(&["untrack", "restore"]), ["untrack", "restore"]);
        assert_eq!(ids(&["restore"]), ["restore"]);
        assert!(ids(&[]).is_empty());
    }

    /// A menu with items of its own does not have to spell out the ones
    /// the app comes with to keep them.
    #[test]
    fn the_defaults_stand_for_every_item_the_app_comes_with() {
        assert_eq!(
            ids(&["mine", "defaults"]),
            ["mine", "open", "restore", "untrack"],
            "the items the app comes with go where the defaults are asked for"
        );
        assert_eq!(
            ids(&["defaults", "mine"]),
            ["open", "restore", "untrack", "mine"]
        );
    }

    /// A menu holds an item once, wherever it is asked for again, so
    /// that the defaults can be asked for alongside one of the items
    /// they hold.
    #[test]
    fn an_id_asked_for_more_than_once_is_held_where_it_comes_first() {
        assert_eq!(
            ids(&["untrack", "defaults"]),
            ["untrack", "open", "restore"]
        );
        assert_eq!(ids(&["restore", "restore"]), ["restore"]);
    }

    /// Every id the app comes with names an item, or the menu it is
    /// listed in would report the app's own configuration as unreadable.
    #[test]
    fn every_item_a_menu_comes_holding_is_one_it_can_build() {
        set_test_env();

        for menu in ALL {
            let items = menu.items().iter().copied().map(item).collect();
            let popup = context_menu(
                &JjConfig::default(),
                None,
                menu,
                &Selection::default(),
                items,
            );

            assert_eq!(
                popup.labels(),
                menu.items(),
                "{} does not hold what it comes holding",
                menu.key()
            );
            assert!(
                popup.footnote_text().is_empty(),
                "{} reports an item it comes holding as unknown",
                menu.key()
            );
        }
    }

    /// An id that names no item at all is the configuration saying
    /// something the app cannot read, which is worth saying where it was
    /// asked for.
    #[test]
    fn an_id_naming_no_item_is_listed_under_the_menu() {
        set_test_env();
        let config: JjConfig =
            toml::from_str("blazingjj.context-menu.files = [\"restore\", \"restroe\"]\n")
                .expect("the configuration parses");

        let popup = context_menu(
            &config,
            None,
            Menu::Files,
            &Selection::default(),
            vec![item("restore")],
        );

        assert_eq!(popup.labels(), ["restore"]);
        assert_eq!(
            popup.footnote_text(),
            ["blazingjj.context-menu.files: no action named restroe"]
        );
    }

    /// An item the selection has nothing to offer is left out the way
    /// the menu leaves out what cannot be done to it anyway, rather than
    /// being reported as a name that does not read.
    #[test]
    fn an_item_the_selection_does_not_offer_is_only_left_out() {
        set_test_env();

        let popup = context_menu(
            &JjConfig::default(),
            None,
            Menu::Files,
            &Selection::default(),
            vec![item("untrack")],
        );

        assert_eq!(popup.labels(), ["untrack"]);
        assert!(popup.footnote_text().is_empty());
    }

    /// A command of your own goes in a menu by the name it is
    /// configured under, and says what it is called rather than what it
    /// is named by wherever it says anything.
    #[test]
    fn an_id_naming_a_command_of_your_own_holds_it() {
        set_test_env();
        let config: JjConfig = toml::from_str(
            "blazingjj.context-menu.files = [\"defaults\", \"reveal\", \"blame\"]\n\
             blazingjj.commands.reveal = \"xdg-open\"\n\
             [blazingjj.commands.blame]\n\
             command = [\"jj\", \"file\", \"annotate\", \"$file\"]\n\
             label = \"Blame\"\n",
        )
        .expect("the configuration parses");

        let popup = context_menu(
            &config,
            None,
            Menu::Files,
            &Selection::default(),
            vec![item("restore"), item("untrack")],
        );

        assert_eq!(popup.labels(), ["restore", "untrack", "reveal", "Blame"]);
        assert!(popup.footnote_text().is_empty());
    }

    /// The app's own items come first, so that a command of your own
    /// taking the name of one is not picked in its place.
    #[test]
    fn a_command_of_your_own_cannot_take_the_name_of_an_item() {
        set_test_env();
        let config: JjConfig = toml::from_str(
            "[blazingjj.commands.restore]\n\
             command = \"restore\"\n\
             label = \"Mine\"\n",
        )
        .expect("the configuration parses");

        let popup = context_menu(
            &config,
            None,
            Menu::Files,
            &Selection::default(),
            vec![item("restore")],
        );

        assert_eq!(
            popup.labels(),
            ["restore"],
            "the item is what `restore` holds"
        );
    }
}
