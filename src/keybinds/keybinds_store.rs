use ratatui::crossterm::event::KeyEvent;

use super::Keybind;
use super::Shortcut;

/// The shortcuts bound to actions, in the order they were bound, which
/// is the order they are shown in.
#[derive(Debug)]
pub struct KeybindsStore<A> {
    shortcut_actions: Vec<(Shortcut, A)>,
}

impl<A> KeybindsStore<A>
where
    A: Clone + Eq,
{
    pub fn match_event(&self, event: KeyEvent) -> Option<A> {
        let shortcut = Shortcut::from_event(event);
        self.shortcut_actions
            .iter()
            .find(|(s, _)| *s == shortcut)
            .map(|(_, a)| a.to_owned())
    }
    pub fn add_action(&mut self, shortcut: Shortcut, action: A) {
        match self
            .shortcut_actions
            .iter_mut()
            .find(|(s, _)| *s == shortcut)
        {
            Some(bound) => bound.1 = action,
            None => self.shortcut_actions.push((shortcut, action)),
        }
    }
    pub fn get_shortcuts(&self, action: A) -> Vec<Shortcut> {
        self.shortcut_actions
            .iter()
            .filter(|(_, a)| *a == action)
            .map(|(s, _)| *s)
            .collect()
    }
    pub fn replace_action_from_config(&mut self, action: A, key: &Keybind) {
        // just ignore this case
        if matches!(key, Keybind::Enable(true)) {
            return;
        }

        self.remove_action(action.clone());
        match key {
            Keybind::Single(s) => self.add_action(*s, action),
            Keybind::Multiple(list) => {
                for s in list {
                    self.add_action(*s, action.clone());
                }
            }
            // in case Enable(false) action is only removed
            Keybind::Enable(_) => (),
        }
    }
    /// Remove all shortcuts for specified action
    fn remove_action(&mut self, action: A) {
        self.shortcut_actions.retain(|(_, a)| action != *a);
    }
    pub fn len(&self) -> usize {
        self.shortcut_actions.len()
    }
}

impl<A> Default for KeybindsStore<A> {
    fn default() -> Self {
        Self {
            shortcut_actions: Vec::new(),
        }
    }
}
