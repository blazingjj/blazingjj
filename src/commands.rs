/*! The commands of your own that the configuration adds, as opposed to
the operations the app has of its own ([Command](crate::app::command)).

A command is configured under the name a context menu holds it by, as
the command line to run, which the placeholders naming what a tab has
selected can be written into:

```toml
[blazingjj.commands.create-pr]
command = ["jj", "util", "exec", "--", "gh", "pr", "create"]
label = "Create PR"
interactive = true
```

The command line on its own is the same thing without a label and with
its output captured, which is what most of them come to:

```toml
[blazingjj.commands]
show-marked = ["jj", "show", "$marked"]
```
*/

use std::collections::BTreeMap;

use anyhow::Result;
use anyhow::anyhow;
use serde::Deserialize;

use crate::commander::program::Program;
use crate::env::ConfiguredCommandLine;
use crate::env::get_env;
use crate::selection::Missing;
use crate::selection::Selection;

/// A command of your own, run against what a tab has selected.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(try_from = "ConfiguredCommand")]
pub struct CustomCommand {
    program: String,
    /// The arguments to run it with, which hold the placeholders naming
    /// the selection.
    args: Vec<String>,
    /// What a menu holding it says, or None to go by its name.
    label: Option<String>,
    /// Whether the terminal is handed over to it, rather than its output
    /// being captured and put up.
    interactive: bool,
}

/// A command as it is configured: the command line on its own, or the
/// command line along with what else there is to say about it.
#[derive(Deserialize)]
#[serde(untagged)]
enum ConfiguredCommand {
    CommandLine(ConfiguredCommandLine),
    Described {
        command: ConfiguredCommandLine,
        label: Option<String>,
        #[serde(default)]
        interactive: bool,
    },
}

impl TryFrom<ConfiguredCommand> for CustomCommand {
    type Error = &'static str;

    fn try_from(configured: ConfiguredCommand) -> Result<Self, Self::Error> {
        let (command, label, interactive) = match configured {
            ConfiguredCommand::CommandLine(command) => (command, None, false),
            ConfiguredCommand::Described {
                command,
                label,
                interactive,
            } => (command, label, interactive),
        };
        let (program, args) = command.split()?;

        Ok(Self {
            program,
            args,
            label,
            interactive,
        })
    }
}

impl CustomCommand {
    /// What a menu holding it says, which is `name`, the name it is
    /// configured under, while it says nothing itself.
    pub fn label(&self, name: &str) -> String {
        self.label.clone().unwrap_or_else(|| name.to_owned())
    }

    pub fn is_interactive(&self) -> bool {
        self.interactive
    }

    /// The program to run against `selection`, with the placeholders of
    /// the arguments filled in from it.
    pub fn program(&self, selection: &Selection) -> Result<Program, Missing> {
        let env = get_env();

        Ok(Program::new(&self.program, env.root.clone()).args(selection.substitute(&self.args)?))
    }

    /// The config key it is configured under, for the name it goes by.
    pub fn key(name: &str) -> String {
        format!("blazingjj.commands.{name}")
    }

    /// The command line as it reads: the program and its arguments, a
    /// space between each, the way a configured pager reads.
    pub fn command_line(&self) -> String {
        let mut words = vec![self.program.as_str()];
        words.extend(self.args.iter().map(String::as_str));

        words.join(" ")
    }

    /// What a menu holding it says, or None to go by its name.
    pub fn configured_label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// The command running `command_line`, with nothing else said about
    /// it: a command captures its output and goes by its name until it
    /// says otherwise.
    pub fn to_run(command_line: &str) -> Result<Self> {
        Self {
            program: String::new(),
            args: Vec::new(),
            label: None,
            interactive: false,
        }
        .with_command_line(command_line)
    }

    /// The command with `command_line` to run, which is taken apart at
    /// whitespace, so an argument that holds any has to be written in
    /// the config file itself. A command line with no program to run is
    /// refused.
    pub fn with_command_line(&self, command_line: &str) -> Result<Self> {
        let words: Vec<String> = command_line.split_whitespace().map(str::to_owned).collect();
        let (program, args) = ConfiguredCommandLine::CommandLine(words)
            .split()
            .map_err(|err| anyhow!("{err}"))?;

        Ok(Self {
            program,
            args,
            ..self.clone()
        })
    }

    /// The command a menu says `label` for, going by its name again
    /// where `label` says nothing.
    pub fn with_label(&self, label: &str) -> Self {
        Self {
            label: Some(label.trim().to_owned()).filter(|label| !label.is_empty()),
            ..self.clone()
        }
    }

    /// The command run the other way round: with the terminal handed
    /// over to it if its output was captured, and the other way about.
    pub fn toggled_interactive(&self) -> Self {
        Self {
            interactive: !self.interactive,
            ..self.clone()
        }
    }

    /// The TOML expression it is written as, which is the command line
    /// on its own where there is nothing else to say about it.
    pub fn value(&self) -> String {
        let command_line = toml::Value::Array(
            std::iter::once(&self.program)
                .chain(self.args.iter())
                .map(|word| toml::Value::String(word.clone()))
                .collect(),
        );

        let mut table = toml::Table::new();
        table.insert("command".to_owned(), command_line.clone());
        if let Some(label) = self.label.as_ref() {
            table.insert("label".to_owned(), toml::Value::String(label.clone()));
        }
        if self.interactive {
            table.insert("interactive".to_owned(), toml::Value::Boolean(true));
        }

        if table.len() == 1 {
            return command_line.to_string();
        }

        toml::Value::Table(table).to_string()
    }
}

/// A command of your own to run, and what a tab had selected when it
/// was asked for.
#[derive(Clone, Debug)]
pub struct CustomRun {
    /// The name it is configured under, which is what there is to call
    /// it in a report about it.
    pub name: String,
    pub command: CustomCommand,
    pub selection: Selection,
}

/// The commands of your own the configuration adds, under the names
/// they are configured by.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct CustomCommands(BTreeMap<String, CustomCommand>);

impl CustomCommands {
    /// The command `name` names, if the configuration adds one by that
    /// name.
    pub fn get(&self, name: &str) -> Option<&CustomCommand> {
        self.0.get(name)
    }

    /// Every command there is, by name, in the order the names sort in.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &CustomCommand)> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commander::ids::ChangeId;
    use crate::commander::ids::CommitId;
    use crate::commander::log::Head;
    use crate::env::JjConfig;
    use crate::env::set_test_env;
    use crate::selection::Placeholder;

    /// The commands the given configuration adds
    fn commands(config: &str) -> CustomCommands {
        toml::from_str::<JjConfig>(config)
            .expect("the configuration parses")
            .commands()
            .clone()
    }

    /// The one command `config` adds, under `name`
    fn command(config: &str, name: &str) -> CustomCommand {
        commands(config)
            .get(name)
            .expect("the configuration adds the command")
            .clone()
    }

    /// Most commands have nothing to say beyond what to run, so the
    /// command line on its own configures one.
    #[test]
    fn a_command_line_configures_a_command_that_captures_its_output() {
        let command = command(
            "blazingjj.commands.show-marked = [\"jj\", \"show\", \"$marked\"]\n",
            "show-marked",
        );

        assert_eq!(command.program, "jj");
        assert_eq!(command.args, ["show", "$marked"]);
        assert!(!command.is_interactive());
        assert_eq!(command.label("show-marked"), "show-marked");
    }

    /// A command with more to say about it than what to run says it
    /// alongside the command line.
    #[test]
    fn a_command_can_say_what_it_is_called_and_how_it_is_run() {
        let command = command(
            "[blazingjj.commands.create-pr]\n\
             command = [\"gh\", \"pr\", \"create\"]\n\
             label = \"Create PR\"\n\
             interactive = true\n",
            "create-pr",
        );

        assert_eq!(command.program, "gh");
        assert_eq!(command.args, ["pr", "create"]);
        assert!(command.is_interactive());
        assert_eq!(command.label("create-pr"), "Create PR");
    }

    /// A command that is only a program is a command line of one word,
    /// as a configured pager or editor is.
    #[test]
    fn a_program_on_its_own_is_a_command_line() {
        let command = command("blazingjj.commands.tug = \"tug\"\n", "tug");

        assert_eq!(command.program, "tug");
        assert!(command.args.is_empty());
    }

    /// A command with no program to run is one to say something about
    /// where it is configured rather than one to leave in the menus for
    /// the picking.
    #[test]
    fn a_command_without_a_program_is_refused() {
        let error = toml::from_str::<JjConfig>("blazingjj.commands.broken = []\n")
            .expect_err("a command without a program is an error");

        assert!(
            error.to_string().contains("a command line needs a program"),
            "the error does not say what is wrong: {error}"
        );
    }

    /// The placeholders of a command are filled in when it is run, not
    /// when it is read, so that one command works against whatever is
    /// selected at the time.
    #[test]
    fn a_command_is_run_against_what_is_selected_at_the_time() {
        set_test_env();
        let command = command(
            "blazingjj.commands.diff = [\"difft\", \"--rev=$revision\", \"$file\"]\n",
            "diff",
        );
        let selection = Selection::default().file("src/main.rs");

        assert_eq!(
            command
                .program(&selection)
                .expect_err("the tab has no revision"),
            Missing(Placeholder::Revision)
        );

        let head = Head {
            change_id: ChangeId("change".to_owned()),
            commit_id: CommitId("commit".to_owned()),
            divergent: false,
            immutable: false,
        };
        let program = command
            .program(&selection.revision(&head, false))
            .expect("the selection fills the placeholders in");

        assert_eq!(program.args_text(), ["--rev=change", "src/main.rs"]);
    }
}
