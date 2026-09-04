use std::str::FromStr;

use ratatui::crossterm::event::KeyEvent;

use super::Binding;
use super::Context;
use super::Section;
use super::Shortcut;
use super::config::CommandsTabKeybindsConfig;
use super::config::KeybindsConfig;
use super::keybinds_store::KeybindsStore;
use crate::env::keybinds_config;
use crate::make_bindings;
use crate::set_keybinds;
use crate::update_keybinds;

#[derive(Debug)]
pub struct CommandsTabKeybinds {
    keys: KeybindsStore<CommandsTabEvent>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CommandsTabEvent {
    ChangeCommandLine,
    ChangeLabel,
    ToggleInteractive,
    Add,
    Unset,
    Back,

    Unbound,
}

impl Default for CommandsTabKeybinds {
    fn default() -> Self {
        let mut keys = KeybindsStore::<CommandsTabEvent>::default();
        set_keybinds!(
            keys,
            CommandsTabEvent::ChangeCommandLine => "enter",
            CommandsTabEvent::ChangeLabel => "l",
            CommandsTabEvent::ToggleInteractive => "i",
            CommandsTabEvent::Add => "n",
            CommandsTabEvent::Unset => "x",
            CommandsTabEvent::Back => "esc",
        );
        Self { keys }
    }
}

impl CommandsTabKeybinds {
    /// The bindings as the configuration has them.
    pub fn new() -> Self {
        Self::from_config(keybinds_config())
    }

    /// The bindings as `config` has them.
    pub(super) fn from_config(config: Option<&KeybindsConfig>) -> Self {
        let mut keybinds = Self::default();
        if let Some(config) = config.and_then(|config| config.commands_tab.as_ref()) {
            keybinds.extend_from_config(config);
        }
        keybinds
    }

    fn extend_from_config(&mut self, config: &CommandsTabKeybindsConfig) {
        update_keybinds!(
            self.keys,
            CommandsTabEvent::ChangeCommandLine => config.change_command_line,
            CommandsTabEvent::ChangeLabel => config.change_label,
            CommandsTabEvent::ToggleInteractive => config.toggle_interactive,
            CommandsTabEvent::Add => config.add,
            CommandsTabEvent::Unset => config.unset,
            CommandsTabEvent::Back => config.back,
        );
    }

    pub fn match_event(&self, event: KeyEvent) -> CommandsTabEvent {
        self.keys
            .match_event(event)
            .unwrap_or(CommandsTabEvent::Unbound)
    }

    /// The line under the list saying what it answers to, in as much of
    /// `width` as it takes. Where there is no room for all of it, what
    /// is least worth saying goes first, down to changing a command and
    /// leaving the list again.
    pub fn hint(&self, width: usize) -> String {
        let mut parts: Vec<String> = [
            (CommandsTabEvent::ChangeCommandLine, "change"),
            (CommandsTabEvent::Add, "add"),
            (CommandsTabEvent::ChangeLabel, "label"),
            (CommandsTabEvent::ToggleInteractive, "interactive"),
            (CommandsTabEvent::Unset, "take out"),
            (CommandsTabEvent::Back, "back"),
        ]
        .into_iter()
        .filter_map(|(event, what)| Some(format!("{}: {what}", self.shortcut(event)?)))
        .collect();

        while parts.len() > 2 && Self::hint_width(&parts) > width {
            parts.remove(parts.len() - 2);
        }

        parts.join(" | ")
    }

    /// How wide the hint `parts` are with what joins them.
    fn hint_width(parts: &[String]) -> usize {
        let joins = 3 * parts.len().saturating_sub(1);

        joins + parts.iter().map(|part| part.chars().count()).sum::<usize>()
    }

    /// The shortcut to name `event` by, of those bound to it.
    fn shortcut(&self, event: CommandsTabEvent) -> Option<Shortcut> {
        self.keys.get_shortcuts(event).into_iter().next()
    }

    pub fn bindings(&self) -> Vec<Binding> {
        make_bindings!(
            self.keys, Self::default().keys, Context::CommandsTab,
            CommandsTabEvent::ChangeCommandLine => "change-command-line", Some(Section::Settings), "change what the command runs",
            CommandsTabEvent::ChangeLabel => "change-label", Some(Section::Settings), "change what a menu calls the command",
            CommandsTabEvent::ToggleInteractive => "toggle-interactive", Some(Section::Settings), "hand the terminal to the command, or capture its output",
            CommandsTabEvent::Add => "add", Some(Section::Settings), "add a command of your own",
            CommandsTabEvent::Unset => "unset", Some(Section::Settings), "take the command out of your config",
            CommandsTabEvent::Back => "back", Some(Section::Settings), "go back to the settings",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commands_tab_keybinds_default() {
        let _ = CommandsTabKeybinds::default();
    }

    /// The hint says as much as there is room for, and what it drops is
    /// what is least worth saying rather than what comes last.
    #[test]
    fn test_the_hint_drops_what_there_is_no_room_for() {
        let keybinds = CommandsTabKeybinds::default();

        assert_eq!(
            keybinds.hint(80),
            "Enter: change | n: add | l: label | i: interactive | x: take out | Esc: back"
        );
        assert_eq!(keybinds.hint(40), "Enter: change | n: add | Esc: back");
        assert_eq!(keybinds.hint(1), "Enter: change | Esc: back");
    }
}
