/*! The styles tab lists every element the app draws, what it is drawn
in and how, and shows what the selected one is for in the details panel.

It is the settings tab's, opened from the row for `blazingjj.styles` and
left again for it, so it has no place of its own in the tab bar. What it
writes are the keys under that table, in the user's own config file, just
as the settings tab writes the options beside it.
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
use crate::env::check_config_value;
use crate::event::Mouse;
use crate::keybinds::Binding;
use crate::keybinds::StylesTabEvent;
use crate::keybinds::StylesTabKeybinds;
use crate::theme::Attribute;
use crate::theme::Channel;
use crate::theme::Role;
use crate::theme::ThemeColor;
use crate::theme::theme;
use crate::ui::AppAction;
use crate::ui::Component;
use crate::ui::ComponentInputResult;
use crate::ui::Scroll;
use crate::ui::Tab;
use crate::ui::dialog::ChoicePopup;
use crate::ui::dialog::SettingValuePopup;
use crate::ui::panel::ListPane;
use crate::ui::panel::MouseInput;
use crate::ui::panel::PanelMouseInput;
use crate::ui::panel::Row as SectionRow;
use crate::ui::panel::Sections;
use crate::ui::panel::copy_marked;
use crate::ui::styles::SWATCH_WIDTH;
use crate::ui::styles::panel_block;
use crate::ui::styles::panel_title;
use crate::ui::styles::patched;
use crate::ui::styles::section_heading;
use crate::ui::styles::swatch;
use crate::ui::utils::PaneDivider;
use crate::ui::utils::error_text;

/// What every row of the list is indented by, headings apart.
const INDENT: &str = "   ";

/// What the list says about an attribute in the one cell it gives it:
/// its initial in capitals where it is asked for, in small letters
/// where it is turned down, and a dot where nothing says either way.
fn flag_of(attribute: Attribute, asked: Option<bool>) -> String {
    let initial = attribute.key()[..1].to_owned();

    match asked {
        Some(true) => initial.to_uppercase(),
        Some(false) => initial,
        None => "·".to_owned(),
    }
}

/// What the list says its flags mean, under it. The keys that turn them
/// round are the details panel's to name, beside what they change.
const LEGEND: [&str; 2] = [
    "B D I U  bold dim italic underline",
    "B yes, b no, · inherited",
];

/// How wide the column of names is: as wide as the widest of them.
fn name_width() -> usize {
    Role::ALL
        .iter()
        .map(|role| role.key().len())
        .max()
        .unwrap_or(0)
}

/// Where a row's flags start: past the indent, the swatch and the name,
/// with a cell of blank on either side of the name.
fn flags_at(name_width: usize) -> usize {
    INDENT.len() + SWATCH_WIDTH + 2 + name_width + 2
}

/// How wide a row's flags are: one cell each, a blank apart.
const FLAGS_WIDTH: usize = Attribute::ALL.len() * 2 - 1;

/// What the list is read under, above it: what a row names on the left,
/// and what the cells on the right are about, ending where they do.
fn list_heading() -> Line<'static> {
    const ATTRIBUTES: &str = "Attributes";

    Line::raw(format!(
        "{:width$}{ATTRIBUTES}",
        format!("{INDENT}element"),
        width = (flags_at(name_width()) + FLAGS_WIDTH).saturating_sub(ATTRIBUTES.len())
    ))
}

/// The config key `part` of `role`'s style is written under, `part`
/// being what a channel or an attribute is called there.
fn key_of(role: Role, part: &str) -> String {
    format!("blazingjj.styles.{}.{part}", role.key())
}

/// The table `role`'s style is written under, which is a key of its own
/// for a role written as the single colour to draw it in.
fn table_of(role: Role) -> String {
    format!("blazingjj.styles.{}", role.key())
}

/// What the user's own config file says about the styles, which is the
/// layer the tab writes and the only one it can take a colour out of.
#[derive(Default)]
struct UserStyles {
    /// Its `blazingjj.styles` table, empty where it has none.
    styles: toml::Table,
}

impl UserStyles {
    fn read() -> Result<Self> {
        Ok(Self::of(&new_commander().get_user_config()?))
    }

    /// What `config`, the user's own config file, says about the styles.
    fn of(config: &toml::Table) -> Self {
        Self {
            styles: config_value(config, "blazingjj.styles")
                .and_then(toml::Value::as_table)
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// What the user's own config file says about `role`: a table of its
    /// colours and attributes, or the one colour to draw it in, which is
    /// its foreground.
    fn said_about(&self, role: Role) -> Option<&toml::Value> {
        self.styles.get(role.key())
    }

    /// The key what is asked for `part` of `role`'s style is written
    /// under. jj refuses to set any key under a value that is not a
    /// table, so a role written as the one colour to draw it in is
    /// written afresh as a table rather than given a key under it.
    fn asked_key(&self, role: Role, part: &str) -> String {
        match self.said_alone(role) {
            Some(_) => table_of(role),
            None => key_of(role, part),
        }
    }

    /// The one colour the user's own config file draws `role` in, for a
    /// role written as that rather than as a table of its two.
    fn said_alone(&self, role: Role) -> Option<&str> {
        match self.said_about(role)? {
            toml::Value::Table(_) => None,
            said => said.as_str(),
        }
    }

    /// Whether `said`, what the user's own config file says about a
    /// role, is what says `part` of its style.
    fn gives(said: Option<&toml::Value>, part: &str) -> bool {
        match said {
            Some(toml::Value::Table(style)) => style.contains_key(part),
            Some(_) => part == Channel::Fg.key(),
            None => false,
        }
    }

    /// Whether the user's own config file is what says `part` of
    /// `role`'s style, which is what makes it the tab's to take back
    /// out.
    fn is_users(&self, role: Role, part: &str) -> bool {
        Self::gives(self.said_about(role), part)
    }

    /// The key that takes `part` of `role`'s style back out of the
    /// user's own config file, where that is what says it. A role
    /// written as the single colour to draw it in is a key of its own,
    /// so its foreground goes by taking the role out and there is
    /// nothing under it to take anything else out of.
    fn taken_out_by(&self, role: Role, part: &str) -> Option<String> {
        self.is_users(role, part)
            .then(|| self.asked_key(role, part))
    }

    /// What of `role`'s style the user's own config file says, by the
    /// names it says them under.
    fn parts_of(&self, role: Role) -> Vec<&'static str> {
        Channel::ALL
            .into_iter()
            .map(Channel::key)
            .chain(Attribute::ALL.into_iter().map(Attribute::key))
            .filter(|part| self.is_users(role, part))
            .collect()
    }

    /// The keys the user's own config file styles `role` by, which are
    /// what there is to take back out of it.
    fn keys_of(&self, role: Role) -> Vec<String> {
        Channel::ALL
            .into_iter()
            .map(Channel::key)
            .chain(Attribute::ALL.into_iter().map(Attribute::key))
            .filter_map(|part| self.taken_out_by(role, part))
            .collect()
    }
}

pub struct StylesTab {
    /// What the user's own config file says, or why it could not be read.
    styles: Result<UserStyles>,

    /// The elements under the headings they are listed by.
    roles: Sections<Role>,
    roles_pane: ListPane,
    roles_list_state: ListState,

    keybinds: StylesTabKeybinds,
    pane_divider: PaneDivider,

    stale: bool,
}

impl StylesTab {
    /// A stale tab, holding nothing of what the configuration says yet.
    #[instrument(level = "info", name = "Initializing styles tab", parent = None)]
    pub fn new() -> Self {
        Self {
            styles: Ok(UserStyles::default()),

            roles: Sections::new(Role::ALL, |role: &Role| role.section()),
            roles_pane: ListPane::default(),
            roles_list_state: ListState::default(),

            keybinds: StylesTabKeybinds::new(),
            pane_divider: PaneDivider::default(),

            stale: true,
        }
    }

    fn selected(&self) -> Option<Role> {
        self.roles.selected().copied()
    }

    /// Ask for a colour to draw the selected element in.
    fn change_selected(&self, channel: Channel) -> Option<AppAction> {
        let role = self.selected()?;
        let styles = self.styles.as_ref().ok()?;
        // The colour an element written as one was written as is its
        // foreground, so that is what the other one is written beside,
        // in the one spelling it has. One that reads as no colour is
        // written back as it stands rather than dropped.
        let beside = styles.said_alone(role).map(|said| {
            said.parse::<ThemeColor>()
                .map_or_else(|_| said.to_owned(), |color| color.to_string())
        });
        let key = styles.asked_key(role, channel.key());
        // What it is drawn in now is what to start from, so that a
        // colour is adjusted rather than typed out again.
        let current = theme()
            .color_of(role, channel)
            .map(|color| color.to_string())
            .unwrap_or_default();

        let asked = key.clone();
        Some(AppAction::SetPopup(Box::new(SettingValuePopup::for_key(
            key,
            styles.taken_out_by(role, channel.key()),
            current,
            move |input| color_value(&asked, channel, beside.as_deref(), input),
        ))))
    }

    /// Turn `attribute` round on the selected element: whichever way it
    /// is drawn now, it is asked for the other way, and asking again
    /// takes that back out and leaves the element what it inherits.
    fn toggle_selected(&self, attribute: Attribute) -> Option<AppAction> {
        let role = self.selected()?;
        let styles = self.styles.as_ref().ok()?;

        let key = styles.asked_key(role, attribute.key());
        // Turning it round is what the key is for, so it goes by what
        // the element is drawn with rather than by what the config says:
        // asking for an attribute an element comes with already would
        // change nothing on screen.
        if styles.is_users(role, attribute.key()) {
            return Some(AppAction::Run(Command::UnsetSetting { key }));
        }
        let asked = !theme().attribute_of(role, attribute).unwrap_or(false);

        // An element written as the one colour to draw it in is written
        // afresh as a table, that colour beside the attribute, jj having
        // no way to set a key under a value that is not a table.
        let value = match styles.said_alone(role) {
            None => toml::Value::Boolean(asked),
            Some(beside) => toml::Value::Table(toml::Table::from_iter([
                (
                    Channel::Fg.key().to_owned(),
                    toml::Value::String(beside.to_owned()),
                ),
                (attribute.key().to_owned(), toml::Value::Boolean(asked)),
            ])),
        };

        Some(AppAction::Run(Command::SetSetting {
            key,
            value: value.to_string(),
        }))
    }

    /// Take the selected element's colours out of the user's config
    /// file, leaving whatever the rest of the configuration says.
    fn unset_selected(&self) -> Option<AppAction> {
        let role = self.selected()?;
        let styles = self.styles.as_ref().ok()?;

        // Only the colours the user's own config gives, a colour at a
        // time: what the rest of the configuration says about the role
        // is not the tab's to take away.
        let taken_out: Vec<AppAction> = styles
            .keys_of(role)
            .into_iter()
            .map(|key| AppAction::Run(Command::UnsetSetting { key }))
            .collect();

        (!taken_out.is_empty()).then(|| AppAction::Multiple(taken_out))
    }

    /// The menu of what can be done to the selected element, put at
    /// `anchor` or centered when there is nowhere to point at.
    fn context_menu(&self, anchor: Option<Position>) -> Option<AppAction> {
        let mut items = vec![
            (
                Line::raw("Change what it is drawn in"),
                self.change_selected(Channel::Fg)?,
            ),
            (
                Line::raw("Change what it is drawn on"),
                self.change_selected(Channel::Bg)?,
            ),
        ];
        for attribute in Attribute::ALL {
            items.push((
                Line::raw(format!(
                    "Draw it {} or not, against what it inherits",
                    attribute.key()
                )),
                self.toggle_selected(attribute)?,
            ));
        }
        if let Some(unset) = self.unset_selected() {
            items.push((Line::raw("Take out of your config"), unset));
        }

        Some(AppAction::SetPopup(Box::new(ChoicePopup::new(
            anchor,
            "Style actions",
            items,
        ))))
    }

    fn handle_event(&mut self, event: StylesTabEvent) -> Option<AppAction> {
        match event {
            StylesTabEvent::ChangeForeground => self.change_selected(Channel::Fg),
            StylesTabEvent::ChangeBackground => self.change_selected(Channel::Bg),
            StylesTabEvent::ToggleBold => self.toggle_selected(Attribute::Bold),
            StylesTabEvent::ToggleDim => self.toggle_selected(Attribute::Dim),
            StylesTabEvent::ToggleItalic => self.toggle_selected(Attribute::Italic),
            StylesTabEvent::ToggleUnderline => self.toggle_selected(Attribute::Underline),
            StylesTabEvent::Unset => self.unset_selected(),
            StylesTabEvent::Back => Some(AppAction::ViewTab(TabId::Settings)),
            // Not an operation of its own; the key handler deals with it.
            StylesTabEvent::Unbound => None,
        }
    }

    /// One row per element: a patch of what it is drawn in, its name,
    /// and the two colours as they are written, under the heading of the
    /// part of the app it belongs to.
    fn roles_lines(&self, styles: &UserStyles) -> Vec<Line<'static>> {
        let theme = theme();
        let width = name_width();

        self.roles
            .rows()
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let selected = index == self.roles.selected_row();
                let highlighted = |line| {
                    if selected {
                        patched(line, theme.style(Role::Highlight))
                    } else {
                        line
                    }
                };

                match row {
                    // The indent is no part of the heading, so it is no
                    // part of what is underlined either.
                    SectionRow::Heading(heading) => {
                        highlighted(Line::from(vec![Span::raw(" "), section_heading(*heading)]))
                    }
                    SectionRow::Item(role) => {
                        let by_user = styles.said_about(*role);
                        // What the user's own config does not give is
                        // dimmed as the settings tab dims an option it
                        // falls back on, that being the same thing said
                        // of a style.
                        let as_said = |span: Span<'static>, part: &str| {
                            if UserStyles::gives(by_user, part) {
                                span
                            } else {
                                span.patch_style(theme.style(Role::Hint)).italic()
                            }
                        };
                        // The colours are the swatch's to show and the
                        // details panel's to name, so the row spends
                        // what it has on the attributes, which nothing
                        // else shows.
                        let attributes = Attribute::ALL.into_iter().map(|attribute| {
                            let flag = flag_of(attribute, theme.attribute_of(*role, attribute));

                            as_said(Span::raw(format!("{flag} ")), attribute.key())
                        });

                        let mut line = highlighted(Line::from_iter(
                            [
                                Span::raw(INDENT),
                                Span::raw(format!("  {:width$}  ", role.key())),
                            ]
                            .into_iter()
                            .chain(attributes),
                        ));
                        // The swatch is what the row is about, so it
                        // keeps the element's own colours even where the
                        // highlight has taken the rest of the row.
                        line.spans.insert(1, swatch(theme.style(*role)));

                        line
                    }
                }
            })
            .collect()
    }

    /// What the selected element is drawn for, what it is drawn in, and
    /// where that comes from.
    fn details_text(&self, styles: &UserStyles) -> Text<'static> {
        let Some(role) = self.selected() else {
            return Text::default();
        };
        let theme = theme();
        let mut lines = vec![
            Line::raw(format!("blazingjj.styles.{}", role.key())).bold(),
            Line::raw(""),
            Line::raw(role.doc()),
            Line::raw(""),
        ];

        // Each line is labelled with the key that changes it: the hint
        // under the list has room for two of them, and this is where
        // you are looking while changing one element anyway.
        let label = |what: &str, event: StylesTabEvent| {
            let key = self
                .keybinds
                .shortcut(event)
                .map_or_else(|| "unbound".to_owned(), |shortcut| shortcut.to_string());

            Span::raw(format!("{:22}", format!("{what} ({key}):")))
        };

        for (what, channel, event) in [
            ("Drawn in", Channel::Fg, StylesTabEvent::ChangeForeground),
            ("Drawn on", Channel::Bg, StylesTabEvent::ChangeBackground),
        ] {
            let color = theme.color_of(role, channel);

            lines.push(Line::from(vec![
                label(what, event),
                Span::raw(color.map_or_else(
                    || "whatever is underneath".to_owned(),
                    |color| color.to_string(),
                ))
                .bold(),
            ]));
        }

        for (attribute, event) in [
            (Attribute::Bold, StylesTabEvent::ToggleBold),
            (Attribute::Dim, StylesTabEvent::ToggleDim),
            (Attribute::Italic, StylesTabEvent::ToggleItalic),
            (Attribute::Underline, StylesTabEvent::ToggleUnderline),
        ] {
            lines.push(Line::from(vec![
                label(attribute.key(), event),
                Span::raw(match theme.attribute_of(role, attribute) {
                    Some(true) => "yes",
                    Some(false) => "no",
                    None => "inherited",
                })
                .bold(),
            ]));
        }

        // What of that is yours is worth saying once rather than beside
        // every line: an element is drawn as the app, a scheme and your
        // own config together have it, and only the last is the tab's
        // to take away.
        let unset = self.keybinds.shortcut(StylesTabEvent::Unset).map_or_else(
            || "nothing here".to_owned(),
            |shortcut| shortcut.to_string(),
        );

        lines.push(Line::raw(""));
        lines.push(Line::raw(match styles.parts_of(role).as_slice() {
            [] => "Your config says nothing about this element.".to_owned(),
            parts => format!(
                "Your config says its {}, which {unset} takes back out.",
                parts.join(", ")
            ),
        }));

        lines.push(Line::raw(""));
        lines.push(
            Line::raw(
                "A color is one of the sixteen names, a #rrggbb code, ansi-color-0 to \
                 ansi-color-255, or \"default\" for the terminal's own.",
            )
            .patch_style(theme.style(Role::Hint)),
        );

        Text::from(lines)
    }
}

/// The TOML expression `input` stands for as `channel`'s colour, refused
/// when it names none. What is typed is written back in the one spelling
/// the colour has, so that the config file reads the same however it was
/// asked for.
///
/// `beside` is the colour the element was written as on its own, which
/// is written back as its foreground beside the one asked for. Asking
/// for the foreground is what replaces it, leaving the element the one
/// colour it was.
fn color_value(key: &str, channel: Channel, beside: Option<&str>, input: &str) -> Result<String> {
    let color: ThemeColor = input.trim().parse()?;
    let value = match beside {
        None => toml::Value::String(color.to_string()),
        Some(beside) => toml::Value::Table(toml::Table::from_iter([
            (
                Channel::Fg.key().to_owned(),
                toml::Value::String(beside.to_owned()),
            ),
            (
                channel.key().to_owned(),
                toml::Value::String(color.to_string()),
            ),
        ])),
    }
    .to_string();
    check_config_value(key, &value)?;

    Ok(value)
}

impl Tab for StylesTab {
    fn refresh(&mut self) -> Result<()> {
        self.styles = UserStyles::read();
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
        self.keybinds = StylesTabKeybinds::new();
    }

    fn toggle_layout(&mut self) {
        self.pane_divider.toggle_layout();
    }

    fn scroll_main_panel(&mut self, scroll: Scroll) -> Result<()> {
        self.roles
            .scroll(scroll.distance(self.roles_pane.visible_items()));

        Ok(())
    }

    fn open_context_menu(&self) -> Result<Option<AppAction>> {
        Ok(self.context_menu(self.roles_pane.item_anchor(self.roles.selected_row(), 1)))
    }

    fn main_panel_bindings(&self) -> Vec<Binding> {
        self.keybinds.bindings()
    }

    /// The details panel only says what the selected element is for, so
    /// there is nothing to do to it.
    fn details_panel_bindings(&self) -> Vec<Binding> {
        Vec::new()
    }
}

impl Component for StylesTab {
    fn draw(&mut self, f: &mut Frame<'_>, area: Rect) -> Result<()> {
        let chunks = self.pane_divider.split(area);

        let (rows, details) = match self.styles.as_ref() {
            Ok(styles) => (self.roles_lines(styles), self.details_text(styles)),
            Err(err) => (
                error_text("Error getting the configuration", err)?.lines,
                Text::default(),
            ),
        };

        // The hint goes between the corners, with a space to either side.
        let hint_width = chunks[0].width.saturating_sub(4) as usize;
        // The heading and the legend stand in rows the list gives up, so
        // that they sit inside the panel rather than under it. Neither
        // scrolls with the list, being what it is read by.
        let block = panel_block()
            .title(panel_title(" Settings / Styles "))
            .title_bottom(
                Line::raw(format!(" {} ", self.keybinds.hint(hint_width)))
                    .centered()
                    .patch_style(Role::Hint.style()),
            )
            .padding(Padding::new(0, 0, 1, LEGEND.len() as u16));
        let listed = block.inner(chunks[0]);
        *self.roles_list_state.selected_mut() = Some(self.roles.selected_row());
        self.roles_pane.render(
            f,
            chunks[0],
            block,
            List::new(rows).scroll_padding(3),
            &mut self.roles_list_state,
        );

        f.render_widget(
            Paragraph::new(list_heading()).style(Role::Hint.style()),
            Rect {
                y: listed.y.saturating_sub(1),
                height: 1,
                ..listed
            },
        );
        f.render_widget(
            Paragraph::new(Vec::from_iter(LEGEND.map(Line::raw))).style(Role::Hint.style()),
            Rect {
                y: listed.y + listed.height,
                height: LEGEND.len() as u16,
                ..listed
            },
        );

        f.render_widget(
            Paragraph::new(details).wrap(Wrap { trim: false }).block(
                panel_block()
                    .title(panel_title(" About "))
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
                StylesTabEvent::Unbound => Ok(ComponentInputResult::NotHandled),
                event => Ok(self.handle_event(event).into()),
            };
        }

        Ok(ComponentInputResult::Handled)
    }

    fn input_mouse(&mut self, mouse: Mouse) -> Result<ComponentInputResult> {
        if self.pane_divider.handle_mouse(mouse) {
            return Ok(ComponentInputResult::Handled);
        }
        match self.roles_pane.input_mouse(mouse) {
            MouseInput::Scroll(delta) => self.roles.scroll(delta),
            MouseInput::Select(index) => self.roles.select_row(index),
            MouseInput::Context(index) => {
                self.roles.select_row(index);
                return Ok(self.context_menu(Some(mouse.position())).into());
            }
            MouseInput::Copy(text) => return Ok(copy_marked(text)),
            // Nothing here has a second thing a double click could do.
            MouseInput::Activate | MouseInput::Handled => {}
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

    /// A tab holding the styles as they are, with `config` for the
    /// user's own config file, which the tests have in place of a repo
    /// to read one from.
    fn tab(config: &str) -> StylesTab {
        set_test_env();
        let mut tab = StylesTab::new();
        tab.styles = Ok(UserStyles::of(
            &config.parse().expect("the configuration parses"),
        ));
        tab
    }

    /// What the main panel says, as one string per row.
    fn rows(tab: &StylesTab) -> Vec<String> {
        let styles = tab.styles.as_ref().expect("the configuration was read");

        tab.roles_lines(styles)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// Move the selection to `role`, which is what the keys act on.
    fn select(tab: &mut StylesTab, role: Role) {
        for _ in 0..Role::ALL.len() {
            if tab.selected() == Some(role) {
                return;
            }

            tab.roles.scroll(1);
        }

        panic!("the tab lists {}", role.key());
    }

    /// Every element the app draws is listed, under the heading of the
    /// part of the app it belongs to.
    #[test]
    fn every_element_is_listed_under_a_heading() {
        let rows = rows(&tab(""));

        for role in Role::ALL {
            assert!(
                rows.iter().any(|row| row.contains(role.key())),
                "{} is not listed: {rows:?}",
                role.key()
            );
        }
        assert!(rows.iter().any(|row| row.trim() == "The frame"), "{rows:?}");
        assert!(rows.iter().any(|row| row.trim() == "Files"), "{rows:?}");
    }

    /// The elements of a heading come one after another, that being how
    /// the list gathers them under it: one out of place would put its
    /// heading up a second time.
    #[test]
    fn a_heading_is_put_up_once() {
        let headings: Vec<&str> = Role::ALL.map(Role::section).to_vec();

        for (at, heading) in headings.iter().enumerate() {
            assert!(
                !headings[at + 1..].contains(heading) || headings[at + 1] == *heading,
                "{heading:?} is put up twice: {headings:?}"
            );
        }
    }

    /// A row says how the element is drawn, which nothing else shows:
    /// the colours are the swatch's to show and the details panel's to
    /// name, so the row spends the width it has on the attributes.
    #[test]
    fn a_row_says_which_attributes_the_element_is_drawn_with() {
        let rows = rows(&tab(""));

        assert!(
            rows.iter()
                .any(|row| row.contains("hint") && row.contains("· · · ·")),
            "{rows:?}"
        );

        // The initial says which attribute the cell is about, and
        // whether it is asked for or turned down.
        assert_eq!(flag_of(Attribute::Bold, Some(true)), "B");
        assert_eq!(flag_of(Attribute::Dim, Some(false)), "d");
        assert_eq!(flag_of(Attribute::Underline, None), "·");
    }

    /// The keys are named beside what they change, the hint under the
    /// list having room for two of the eight, and the list says what its
    /// flags mean under it.
    #[test]
    fn the_panel_names_the_key_that_changes_each_part_of_a_style() {
        let screen = drawn(&mut tab(""), 100, 30);
        let said = |what: &str| {
            assert!(screen.iter().any(|row| row.contains(what)), "{screen:?}");
        };

        said("Drawn in (Enter):");
        said("bold (Shift+b):");
        said("B yes, b no, · inherited");
        // What of a style is the user's own is said once, rather than
        // beside every line it could be said of.
        said("Your config says nothing");
        // And the flags are read under their own initials, which stand
        // over the cells rather than scrolling away with the list.
        assert!(
            screen
                .iter()
                .any(|row| row.contains("element") && row.contains("Attributes")),
            "{screen:?}"
        );
    }

    /// The keys the tab answers to are said at the foot of it, which is
    /// where the way out of it is to be found.
    #[test]
    fn the_tab_says_what_it_answers_to() {
        let screen = drawn(&mut tab(""), 100, 30);

        assert!(
            screen
                .iter()
                .any(|row| row.contains("Enter: foreground") && row.contains("Esc: back")),
            "{screen:?}"
        );
    }

    /// Only a colour the user's own config file gives can be taken back
    /// out; one that comes from the scheme or from nowhere is not the
    /// tab's to take out of a layer it does not write.
    #[test]
    fn only_what_the_users_own_config_gives_can_be_taken_back_out() {
        let mut tab = tab("blazingjj.styles.hint = { fg = \"#010203\" }\n");
        select(&mut tab, Role::Hint);
        assert!(tab.unset_selected().is_some());

        select(&mut tab, Role::Error);
        assert!(tab.unset_selected().is_none());
    }

    /// What the keys are is what `jj config unset` is asked to take out,
    /// and it refuses a key whose value is a table. A role written as
    /// one is taken out a colour at a time, and only the colours it
    /// actually gives.
    #[test]
    fn a_role_written_as_a_table_is_taken_out_a_colour_at_a_time() {
        let both = tab("blazingjj.styles.highlight = { fg = \"red\", bg = \"blue\" }\n");
        assert_eq!(
            both.styles.as_ref().unwrap().keys_of(Role::Highlight),
            [
                "blazingjj.styles.highlight.fg",
                "blazingjj.styles.highlight.bg"
            ]
        );

        let one = tab("blazingjj.styles.highlight = { bg = \"blue\" }\n");
        assert_eq!(
            one.styles.as_ref().unwrap().keys_of(Role::Highlight),
            ["blazingjj.styles.highlight.bg"]
        );
    }

    /// A role written as the one colour to draw it in is a key of its
    /// own, which is taken out as it stands.
    #[test]
    fn a_role_written_as_one_colour_is_taken_out_in_one() {
        let tab = tab("blazingjj.styles.hint = \"red\"\n");

        assert_eq!(
            tab.styles.as_ref().unwrap().keys_of(Role::Hint),
            ["blazingjj.styles.hint"]
        );
    }

    /// An attribute goes round being asked for, being turned down and
    /// being left unsaid, so that the one key both sets it and takes it
    /// back out.
    #[test]
    fn an_attribute_is_turned_round_and_taken_back_out() {
        let asked = |config| {
            let mut tab = tab(config);
            select(&mut tab, Role::Hint);

            tab.handle_event(StylesTabEvent::ToggleBold)
        };

        // The hint is drawn in no attribute of its own, so the other way
        // round is bold.
        let Some(AppAction::Run(Command::SetSetting { key, value })) = asked("") else {
            panic!("an attribute is turned round");
        };
        assert_eq!(key, "blazingjj.styles.hint.bold");
        assert_eq!(value, "true");

        // Whichever way round the config has it, saying it again is
        // saying nothing about it.
        for config in [
            "blazingjj.styles.hint.bold = true\n",
            "blazingjj.styles.hint.bold = false\n",
        ] {
            let Some(AppAction::Run(Command::UnsetSetting { key })) = asked(config) else {
                panic!("what the config says about an attribute is taken back out");
            };
            assert_eq!(key, "blazingjj.styles.hint.bold");
        }
    }

    /// jj refuses to set a key under a value that is not a table, so an
    /// element written as the one colour to draw it in is written afresh
    /// as a table of that colour and the attribute asked for.
    #[test]
    fn an_element_written_as_one_colour_is_written_afresh_for_an_attribute() {
        let mut tab = tab("blazingjj.styles.hint = \"red\"\n");
        select(&mut tab, Role::Hint);

        let Some(AppAction::Run(Command::SetSetting { key, value })) =
            tab.handle_event(StylesTabEvent::ToggleItalic)
        else {
            panic!("the attribute is asked for");
        };
        assert_eq!(key, "blazingjj.styles.hint");
        assert_eq!(value, "{ fg = \"red\", italic = true }");
    }

    /// The attributes are the user's to take back out along with the
    /// colours, they being written under the same table.
    #[test]
    fn an_attribute_is_taken_out_with_the_colours() {
        let tab = tab("blazingjj.styles.hint = { fg = \"red\", bold = true }\n");

        assert_eq!(
            tab.styles.as_ref().unwrap().keys_of(Role::Hint),
            ["blazingjj.styles.hint.fg", "blazingjj.styles.hint.bold"]
        );
    }

    /// A role the user's own config says nothing about has nothing to
    /// take out of it.
    #[test]
    fn a_role_the_config_says_nothing_about_has_nothing_to_take_out() {
        let tab = tab("");

        assert!(tab.styles.as_ref().unwrap().keys_of(Role::Hint).is_empty());
    }

    /// A role written as the one colour to draw it in is a foreground,
    /// so that is what the tab reads it as having been given.
    #[test]
    fn a_role_written_as_one_colour_has_been_given_a_foreground() {
        let tab = tab("blazingjj.styles.hint = \"#010203\"\n");
        let styles = tab.styles.as_ref().expect("the configuration was read");

        assert!(styles.is_users(Role::Hint, Channel::Fg.key()));
        assert!(!styles.is_users(Role::Hint, Channel::Bg.key()));
    }

    /// Clearing a colour takes just that one out. A role written as the
    /// one colour to draw it in gives only a foreground, so clearing its
    /// background has nothing to take out rather than the role itself.
    #[test]
    fn clearing_a_colour_takes_out_that_colour_alone() {
        let alone = tab("blazingjj.styles.hint = \"red\"\n");
        let styles = alone.styles.as_ref().expect("the configuration was read");

        assert_eq!(
            styles
                .taken_out_by(Role::Hint, Channel::Fg.key())
                .as_deref(),
            Some("blazingjj.styles.hint")
        );
        assert_eq!(styles.taken_out_by(Role::Hint, Channel::Bg.key()), None);

        let both = tab("blazingjj.styles.hint = { fg = \"red\", bg = \"blue\" }\n");
        let styles = both.styles.as_ref().expect("the configuration was read");

        assert_eq!(
            styles
                .taken_out_by(Role::Hint, Channel::Bg.key())
                .as_deref(),
            Some("blazingjj.styles.hint.bg")
        );
    }

    /// What is typed is written back in the one spelling a colour has,
    /// so that the config file reads the same however it was asked for.
    #[test]
    fn a_colour_is_written_in_the_one_spelling_it_has() {
        let written = |input| {
            color_value("blazingjj.styles.hint.fg", Channel::Fg, None, input)
                .expect("the colour is one that can be written")
        };

        assert_eq!(written(" BrightBlack "), "\"bright black\"");
        assert_eq!(written("208"), "\"ansi-color-208\"");
        assert_eq!(written("#0A141E"), "\"#0a141e\"");
    }

    /// jj refuses to set any key under a value that is not a table, so
    /// an element written as the one colour to draw it in is written
    /// afresh as a table: of both, keeping the one it had, or of the one
    /// asked for where that is the one it had. The one it had is written
    /// in the one spelling it has, as the one asked for is.
    #[test]
    fn an_element_written_as_one_colour_is_written_afresh_as_a_table() {
        let written = |channel, input| {
            color_value(
                "blazingjj.styles.hint",
                channel,
                Some("bright black"),
                input,
            )
            .expect("the colour is one that can be written")
        };

        assert_eq!(
            written(Channel::Bg, "bright blue"),
            "{ bg = \"bright blue\", fg = \"bright black\" }"
        );
        assert_eq!(
            written(Channel::Fg, "bright blue"),
            "{ fg = \"bright blue\" }"
        );
    }

    /// A colour that names nothing is refused where it is typed, with
    /// what one looks like.
    #[test]
    fn a_colour_that_names_nothing_is_refused() {
        let refusal = color_value("blazingjj.styles.hint.fg", Channel::Fg, None, "chartreuse")
            .expect_err("the colour is not one that can be written")
            .to_string();

        assert!(refusal.contains("#rrggbb"), "{refusal}");
    }

    /// A row is one element rather than one colour of one, so each of
    /// its two colours is written under a key of its own.
    #[test]
    fn each_of_the_two_colours_is_written_under_a_key_of_its_own() {
        assert_eq!(
            key_of(Role::Hint, Channel::Fg.key()),
            "blazingjj.styles.hint.fg"
        );
        assert_eq!(
            key_of(Role::Hint, Channel::Bg.key()),
            "blazingjj.styles.hint.bg"
        );
    }

    /// jj refuses to set a key under a value that is not a table, so a
    /// role written as the one colour to draw it in is written afresh
    /// as a table rather than given a key under that colour.
    #[test]
    fn a_role_written_as_one_colour_is_written_afresh_rather_than_under() {
        let alone = tab("blazingjj.styles.hint = \"red\"\n");
        let styles = alone.styles.as_ref().expect("the configuration was read");

        for channel in Channel::ALL {
            assert_eq!(
                styles.asked_key(Role::Hint, channel.key()),
                "blazingjj.styles.hint"
            );
        }

        // A role written as a table, or written nothing about, takes the
        // colour under a key of its own.
        for config in ["blazingjj.styles.hint = { fg = \"red\" }\n", ""] {
            let tab = tab(config);
            let styles = tab.styles.as_ref().expect("the configuration was read");

            assert_eq!(
                styles.asked_key(Role::Hint, Channel::Bg.key()),
                "blazingjj.styles.hint.bg"
            );
        }
    }

    /// Asking for a colour asks for one, whichever of the two was asked
    /// for and whether or not the configuration already gives it.
    #[test]
    fn either_colour_of_the_selected_element_can_be_asked_for() {
        let mut tab = tab("blazingjj.styles.hint = \"red\"\n");
        select(&mut tab, Role::Hint);

        for event in [
            StylesTabEvent::ChangeForeground,
            StylesTabEvent::ChangeBackground,
        ] {
            assert!(matches!(
                tab.handle_event(event),
                Some(AppAction::SetPopup(_))
            ));
        }
    }

    /// The tab is the settings tab's, so leaving it goes back to the row
    /// it was opened from rather than to wherever the tab bar leads.
    #[test]
    fn leaving_the_tab_goes_back_to_the_settings() {
        let mut tab = tab("");

        assert!(matches!(
            tab.handle_event(StylesTabEvent::Back),
            Some(AppAction::ViewTab(TabId::Settings))
        ));
    }
}
