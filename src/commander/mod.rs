/*!
This module contains all functions used to interact with jj via command
line execution.


The module has one primary struct: [`Commander`] which implements
several member functions that each call a jj command and handles the output.
Since the number of jj commands are quite high and some are quite complex,
the implementation is found in multiple source files. This is why you
will find multiple "impl Commander" sections in Commander, one for each source file.

A [Commander] is a reusable handle to a repository: it carries the
ambient context (the repo root, the jj binary, test overrides) and
exposes one method per jj operation. Each operation builds a single
invocation with [Commander::jj], which returns a [JjCommand] builder:

* [Commander::new] - Create a new instance
* [Commander::check_jj_version] - Check jj works with blazingjj
* [Commander::jj] - Start building a single jj invocation
* [JjCommand::run] - Execute the command and return its output
* [JjCommand::run_void] - Execute the command and discard the output

*/

pub mod bookmarks;
pub mod files;
pub mod ids;
pub mod jj;
pub mod log;

use std::ffi::OsStr;
use std::ffi::OsString;
use std::io;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;
use std::string::FromUtf8Error;

use ansi_to_tui::IntoText;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use ratatui::style::Color;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Text;
use thiserror::Error;
use tracing::instrument;
use tracing::trace;
use version_compare::Cmp;
use version_compare::compare;

use crate::env::DiffFormat;
use crate::env::Env;
use crate::env::get_env;

/// The oldest version of jj that is known to work with blazingjj.
/// 0.33.0 changed the template language for evolog/obslog
const JJ_MIN_VERSION: &str = "0.33.0";
const JJ_VERSION_IGNORE_HELP: &str = "If you want to continue anyway, use --ignore-jj-version";

impl DiffFormat {
    pub fn get_args(&self) -> Vec<&str> {
        match self {
            DiffFormat::ColorWords => vec!["--color-words"],
            DiffFormat::Git => vec!["--git"],
            DiffFormat::Summary => vec!["--summary"],
            DiffFormat::Stat => vec!["--stat"],
            DiffFormat::DiffTool(Some(tool)) => vec!["--tool", tool],
            DiffFormat::DiffTool(None) => vec![],
        }
    }
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("Error getting output: {0}")]
    Output(#[from] io::Error),
    #[error("{0}")]
    Status(String, Option<i32>),
    #[error("Error parsing UTF-8 output: {0}")]
    FromUtf8(#[from] FromUtf8Error),
}

impl CommandError {
    #[expect(clippy::wrong_self_convention)]
    pub fn into_text<'a>(&self, title: &'a str) -> Result<Text<'a>, ansi_to_tui::Error> {
        let mut lines = vec![];
        if !title.is_empty() {
            lines.push(Line::raw(title).bold().fg(Color::Red));
            lines.append(&mut vec![Line::raw(""), Line::raw("")]);
        }
        lines.append(&mut self.to_string().into_text()?.lines);

        Ok(Text::from(lines))
    }
}

/// Reusable handle to a repository.
///
/// Holds the ambient context shared by every jj invocation (the repo
/// root, the jj binary, test overrides) and exposes one method per
/// operation. A commander can be reused for any number of commands;
/// per-command options (color, quiet, stdin, ...) live on the
/// [JjCommand] returned by [Commander::jj], not here.
#[derive(Clone, Debug)]
pub struct Commander {
    pub env: Env,
    /// Terminal width passed to jj as `COLUMNS`, if set. Applies to every
    /// command this commander runs, since it describes the output device
    /// rather than any single command.
    columns: Option<usize>,

    // Used for testing
    pub jj_config_toml: Option<Vec<String>>,
    pub force_no_color: bool,
}

/// Initialize a new [Commander] using [ENV]
/// Panics if ENV is not yet initialized
pub fn new_commander() -> Commander {
    Commander::new(get_env())
}

impl Commander {
    pub fn new(env: &Env) -> Self {
        Self {
            env: env.clone(),
            columns: None,
            jj_config_toml: None,
            force_no_color: false,
        }
    }

    /// Tell jj to limit the width of output of secondary programs, like diff
    /// tools, by setting `COLUMNS` on every command this commander runs.
    /// Too narrow width requests are ignored, as they produce garbage output.
    pub fn limit_width(&mut self, columns: usize) {
        const MIN_SETTABLE_WIDTH: usize = 20;
        if columns >= MIN_SETTABLE_WIDTH {
            self.columns = Some(columns);
        }
    }

    /// Start building a single jj invocation with the given arguments.
    ///
    /// The returned [JjCommand] carries the per-command options (color,
    /// quiet, ...) and is executed with [JjCommand::run] or
    /// [JjCommand::run_void].
    pub fn jj<I, S>(&self, args: I) -> JjCommand<'_>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut env_var = Vec::new();
        if let Some(columns) = self.columns {
            env_var.push(("COLUMNS".to_owned(), columns.to_string()));
        }

        JjCommand {
            commander: self,
            args: args.into_iter().map(|s| s.as_ref().to_owned()).collect(),
            color: false,
            quiet: true,
            stdin: None,
            env_var,
        }
    }

    /// Check that the version of jj is recent enough to work with blazingjj
    ///
    /// See also [JJ_MIN_VERSION]
    #[instrument(level = "trace", skip(self))]
    pub fn check_jj_version(&self) -> Result<()> {
        // Ask jj about its version
        let found_version = self
            .jj(["version"])
            .verbose()
            .run()
            .context("Run jj version")?;

        // Extract version number
        if found_version[0..3] != *"jj " {
            trace!("jj version output \"{}\"", found_version);
            bail!("jj version string was not recognized");
        }
        let found_version = &found_version[3..].trim();

        trace!(
            found_version = found_version,
            min_version = JJ_MIN_VERSION,
            "Checking jj version",
        );

        // Verify that jj is not too old
        match compare(found_version, JJ_MIN_VERSION) {
            Err(_) => bail!(
                "Unable to compare version '{found_version}' to '{JJ_MIN_VERSION}'\n{JJ_VERSION_IGNORE_HELP}"
            ),
            Ok(Cmp::Lt) => bail!(
                "jj version is too old ({found_version}). Must be at least {JJ_MIN_VERSION}\n{JJ_VERSION_IGNORE_HELP}"
            ),
            Ok(_) => Ok(()), // found >= min, so jj is recent enough
        }
    }
}

/// A single jj invocation, built from a [Commander] via [Commander::jj].
///
/// Carries the arguments and the per-command options. Configuration
/// methods consume and return the builder so they can be chained; the
/// command is run exactly once with [Self::run] or [Self::run_void].
pub struct JjCommand<'a> {
    commander: &'a Commander,
    args: Vec<OsString>,
    /// Whether the command should emit ANSI color. Off by default so output
    /// is safe to parse; enable with [Self::color] for output shown to the
    /// user.
    color: bool,
    /// Whether to pass `--quiet`. On by default.
    quiet: bool,
    /// Data to feed the command on standard input, if any.
    stdin: Option<String>,
    /// Environment variables for this command.
    env_var: Vec<(String, String)>,
}

impl JjCommand<'_> {
    /// Enable ANSI color in the command's output.
    ///
    /// Off by default, so parsed output stays free of escape codes; enable it
    /// for output shown directly to the user (diffs, logs, the command log).
    pub fn color(mut self) -> Self {
        self.color = true;
        self
    }

    /// Don't pass `--quiet`, so jj's informational output (snapshot and hint
    /// messages) is included. Quiet is on by default.
    pub fn verbose(mut self) -> Self {
        self.quiet = false;
        self
    }

    /// Feed `stdin` to the command on standard input.
    ///
    /// Useful for commands like `jj describe --stdin`, where passing the value
    /// as an argument would be misinterpreted (e.g. a message starting with a
    /// dash being parsed as a flag).
    pub fn stdin(mut self, stdin: &str) -> Self {
        self.stdin = Some(stdin.to_owned());
        self
    }

    /// Execute the command and return its standard output.
    pub fn run(self) -> Result<String, CommandError> {
        let stdout = self.execute(Stdio::piped())?;
        Ok(String::from_utf8(stdout)?)
    }

    /// Execute the command, discarding its output.
    pub fn run_void(self) -> Result<(), CommandError> {
        // The output isn't used, so don't bother capturing or decoding it.
        // Color stays enabled so a failure's stderr reaches the user with its
        // formatting intact.
        self.color().execute(Stdio::null())?;
        Ok(())
    }

    /// Configure and run the command, returning the captured standard output.
    ///
    /// `stdout` selects how the child's standard output is handled: piped to
    /// be captured and returned, or null to be discarded. Standard error is
    /// always captured so it can be surfaced on failure.
    fn execute(self, stdout: Stdio) -> Result<Vec<u8>, CommandError> {
        let mut command = Command::new(&self.commander.env.jj_bin);
        command.args(&self.args);
        command.args(get_output_args(
            !self.commander.force_no_color && self.color,
            self.quiet,
        ));

        if let Some(jj_config_toml) = &self.commander.jj_config_toml {
            for cfg in jj_config_toml {
                command.args(["--config", cfg]);
            }
        }

        command.current_dir(&self.commander.env.root);
        command.envs(self.env_var.iter().cloned());
        command.stdout(stdout);
        command.stderr(Stdio::piped());

        let output = match self.stdin {
            Some(input) => {
                command.stdin(Stdio::piped());
                let mut child = command.spawn()?;
                let mut stdin = child.stdin.take().expect("stdin was piped");
                // Write on a separate thread while we drain the child's output,
                // so neither side deadlocks on a full pipe buffer. Dropping the
                // handle closes the pipe, signalling EOF to the child.
                let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
                let output = child.wait_with_output()?;
                // Ignore a broken pipe (child exited early); the status check
                // below surfaces the real error.
                writer
                    .join()
                    .expect("stdin writer thread panicked")
                    .or_else(|err| match err.kind() {
                        io::ErrorKind::BrokenPipe => Ok(()),
                        _ => Err(err),
                    })?;
                output
            }
            None => command.spawn()?.wait_with_output()?,
        };

        if !output.status.success() {
            // Return JjError if non-zero status code
            return Err(CommandError::Status(
                String::from_utf8_lossy(&output.stderr).to_string(),
                output.status.code(),
            ));
        }

        Ok(output.stdout)
    }
}

pub trait RemoveEndLine {
    fn remove_end_line(self) -> Self;
}

impl RemoveEndLine for String {
    fn remove_end_line(mut self) -> Self {
        if self.ends_with('\n') {
            self.pop();
            if self.ends_with('\r') {
                self.pop();
            }
        }
        self
    }
}

pub fn get_output_args(color: bool, quiet: bool) -> Vec<String> {
    vec![
        "--no-pager",
        "--color",
        if color { "always" } else { "never" },
        if quiet { "--quiet" } else { "" },
    ]
    .into_iter()
    .map(String::from)
    .filter(|arg| !arg.is_empty())
    .collect()
}

#[cfg(test)]
pub mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::env::Env;
    use crate::env::JjConfig;

    macro_rules! apply_common_filters {
        {} => {
            let mut settings = insta::Settings::clone_current();
            // Change + commit IDs
            settings.add_filter(r"[k-z]{8} [0-9a-fA-F]{8}", "[CHANGE_ID + COMMIT_ID]");
            let _bound = settings.bind_to_scope();
        }
    }

    pub struct TestRepo {
        pub commander: Commander,
        pub directory: TempDir,
    }

    impl TestRepo {
        pub fn new() -> Result<Self> {
            let directory = TempDir::with_prefix("blazingjj")?;

            let jj_config_toml = vec![
                r#"user.email="blazingjj@example.com""#.to_owned(),
                r#"user.name="blazingjj""#.to_owned(),
                r#"ui.color="never""#.to_owned(),
            ];

            let jj_bin = "jj".to_string();

            let env = Env {
                root: directory.path().to_string_lossy().to_string(),
                jj_config: JjConfig::default(),
                default_revset: None,
                jj_bin,
            };

            let mut commander = Commander::new(&env);
            commander.jj_config_toml = Some(jj_config_toml);
            commander.force_no_color = true;

            commander.jj(["git", "init", "--colocate"]).run_void()?;

            Ok(Self {
                directory,
                commander,
            })
        }
    }

    #[test]
    fn test_repo() -> Result<()> {
        apply_common_filters!();

        let test_repo = TestRepo::new()?;

        test_repo.commander.jj(["status"]).color().run()?;

        Ok(())
    }
}
