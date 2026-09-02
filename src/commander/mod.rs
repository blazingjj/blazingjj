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
* [JjCommand::run_cancellable] - Execute the command so it can be killed
* [JjCommand::run_foreground] - Execute the command attached to the terminal

*/

pub mod bookmarks;
pub mod cancel;
pub mod config;
pub mod files;
pub mod ids;
pub mod jj;
pub mod log;
pub mod operation;
pub mod revset;

use std::ffi::OsStr;
use std::ffi::OsString;
use std::io;
use std::io::Read;
use std::io::Write;
use std::process::Child;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::string::FromUtf8Error;
use std::thread;
use std::thread::JoinHandle;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use thiserror::Error;
use tracing::error;
use tracing::instrument;
use tracing::trace;
use tracing::warn;
use version_compare::Cmp;
use version_compare::compare;

use crate::commander::cancel::CancelToken;
use crate::env::DiffFormat;
use crate::env::DiffPager;
use crate::env::Env;
use crate::env::get_env;

/// The oldest version of jj that is known to work with blazingjj.
/// 0.42.0 dropped `jj git push --allow-new` and took `--all` to mean what
/// it and `--allow-new` meant together, which is how we push
const JJ_MIN_VERSION: &str = "0.42.0";
const JJ_VERSION_IGNORE_HELP: &str = "If you want to continue anyway, use --ignore-jj-version";

/// The narrowest width jj is told to limit secondary programs to. Anything
/// below this is ignored, as those programs produce garbage output at that
/// size, so it makes no difference to what they print.
pub const MIN_SETTABLE_WIDTH: usize = 20;

/// The editor a command run with [JjCommand::no_editor] is given. There is
/// no such program, so jj names it in the error it fails with, which is how
/// a command that wanted an editor is recognized.
pub const NO_EDITOR: &str = "blazingjj-no-editor";

impl DiffFormat {
    fn get_args(&self) -> Vec<&str> {
        match self {
            DiffFormat::ColorWords => vec!["--color-words"],
            // The pager renders the Git format, so that is what it is fed
            DiffFormat::Git | DiffFormat::Pager(_) => vec!["--git"],
            DiffFormat::Summary => vec!["--summary"],
            DiffFormat::Stat => vec!["--stat"],
            DiffFormat::DiffTool(Some(tool)) => vec!["--tool", tool],
            DiffFormat::DiffTool(None) => vec![],
        }
    }
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("Error spawning child: {0}")]
    Spawn(io::Error),
    #[error("Error getting output: {0}")]
    Output(#[from] io::Error),
    #[error("{0}")]
    Status(String, Option<i32>),
    #[error("Error parsing UTF-8 output: {0}")]
    FromUtf8(#[from] FromUtf8Error),
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
    /// Whether every command this commander runs leaves the working copy
    /// alone, for a caller that must not record an operation whatever it
    /// ends up asking.
    ignore_working_copy: bool,

    // Used for testing
    pub jj_config_toml: Option<Vec<String>>,
    pub force_no_color: bool,
}

/// Initialize a new [Commander] using [get_env]
/// Panics if ENV is not yet initialized
pub fn new_commander() -> Commander {
    Commander::new(get_env())
}

impl Commander {
    pub fn new(env: &Env) -> Self {
        Self {
            env: env.clone(),
            columns: None,
            ignore_working_copy: false,
            jj_config_toml: None,
            force_no_color: false,
        }
    }

    /// Tell jj to limit the width of output of secondary programs, like diff
    /// tools, by setting `COLUMNS` on every command this commander runs.
    /// Too narrow width requests are ignored, as they produce garbage output.
    pub fn limit_width(&mut self, columns: usize) {
        if columns >= MIN_SETTABLE_WIDTH {
            self.columns = Some(columns);
        }
    }

    /// Leave the working copy alone in every command this commander runs,
    /// so nothing it asks can record an operation. For a caller whose
    /// whole read has to leave the repo where it found it, rather than
    /// one command that happens not to need a snapshot.
    pub fn ignore_working_copy(&mut self) {
        self.ignore_working_copy = true;
    }

    /// Start building a single jj invocation with the given arguments.
    ///
    /// The returned [JjCommand] carries the per-command options (color,
    /// quiet, ...) and is executed with [JjCommand::run],
    /// [JjCommand::run_void], [JjCommand::run_cancellable] or
    /// [JjCommand::run_foreground].
    pub fn jj<I, S>(&self, args: I) -> JjCommand
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut env_var = Vec::new();
        if let Some(columns) = self.columns {
            env_var.push(("COLUMNS".to_owned(), columns.to_string()));
        }
        let mut args: Vec<OsString> = args.into_iter().map(|s| s.as_ref().to_owned()).collect();
        if let Some(jj_config_toml) = &self.jj_config_toml {
            for cfg in jj_config_toml {
                args.extend(["--config".into(), cfg.into()]);
            }
        }

        JjCommand {
            jj_bin: self.env.jj_bin.clone(),
            root: self.env.root.clone(),
            args,
            color: false,
            force_no_color: self.force_no_color,
            quiet: true,
            ignore_working_copy: self.ignore_working_copy,
            with_stderr: false,
            stdin: None,
            pager: None,
            columns: self.columns,
            env_var,
        }
    }

    /// Start building a single jj invocation showing a diff in the given
    /// format, which adds the arguments selecting that format and has the
    /// output rendered by its pager, if it has one.
    pub fn jj_diff<'a>(&self, mut args: Vec<&'a str>, diff_format: &'a DiffFormat) -> JjCommand {
        args.extend(diff_format.get_args());

        self.jj(args).pipe_to(diff_format.pager())
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
/// command is run exactly once with [Self::run], [Self::run_void],
/// [Self::run_cancellable] or [Self::run_foreground], the last of which
/// leaves the output options to jj.
pub struct JjCommand {
    jj_bin: String,
    root: String,
    args: Vec<OsString>,
    /// Whether the command should emit ANSI color. Off by default so output
    /// is safe to parse; enable with [Self::color] for output shown to the
    /// user.
    color: bool,
    /// Whether to keep ANSI color off whatever `color` asks for.
    force_no_color: bool,
    /// Whether to pass `--quiet`. On by default.
    quiet: bool,
    /// Whether to pass `--ignore-working-copy`. Off unless the commander
    /// already asks for it, so a command sees the files as they are now.
    ignore_working_copy: bool,
    /// Whether what the command writes on standard error is part of its
    /// output. Off by default, as stderr is not fit to parse.
    with_stderr: bool,
    /// Data to feed the command on standard input, if any.
    stdin: Option<String>,
    /// The pager the output of the command is piped through, if any.
    pager: Option<DiffPager>,
    /// The width the output is rendered at, as far as it can be set.
    columns: Option<usize>,
    /// Environment variables for this command.
    env_var: Vec<(String, String)>,
}

impl JjCommand {
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

    /// Pass `--ignore-working-copy`, so the command reads the repo as it
    /// stands without snapshotting the files first.
    ///
    /// Off unless the commander already asks for it. Use it where a stale
    /// answer beats an operation of our own: reads that are meant to
    /// leave the repo where the caller found it.
    pub fn ignore_working_copy(mut self) -> Self {
        self.ignore_working_copy = true;
        self
    }

    /// Leave the command no editor to open, so that one meant to be used
    /// interactively fails instead of taking the terminal from under the
    /// app. A failure is told apart from any other by [NO_EDITOR].
    pub fn no_editor(mut self) -> Self {
        for setting in ["ui.editor", "ui.diff-editor", "ui.merge-editor"] {
            self.args.push("--config".into());
            self.args.push(format!("{setting}=\"{NO_EDITOR}\"").into());
        }
        self
    }

    /// Take what the command writes on standard error as part of its
    /// output, after what it wrote on standard output.
    ///
    /// Off by default. Use it where what the command has to say about
    /// what it did is the point of running it, as jj reports that on
    /// standard error and keeps standard output for the answer it was
    /// asked for.
    pub fn with_stderr(mut self) -> Self {
        self.with_stderr = true;
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

    /// Pipe the output of the command through `pager`, and take that as the
    /// output.
    pub fn pipe_to(mut self, pager: Option<&DiffPager>) -> Self {
        self.pager = pager.cloned();
        self
    }

    /// Execute the command and return its standard output.
    pub fn run(self) -> Result<String, CommandError> {
        let stdout = self.execute(Stdio::piped(), &CancelToken::new())?;
        Ok(String::from_utf8(stdout)?)
    }

    /// Execute the command, discarding its output.
    pub fn run_void(self) -> Result<(), CommandError> {
        // The output isn't used, so don't bother capturing or decoding it.
        // Color stays enabled so a failure's stderr reaches the user with its
        // formatting intact.
        self.color().execute(Stdio::null(), &CancelToken::new())?;
        Ok(())
    }

    /// Execute the command in a child process `cancel` can kill, and return
    /// its standard output.
    ///
    /// Bytes that are not UTF-8 become replacement characters, so the
    /// output is fit to put on screen but not to parse.
    pub fn run_cancellable(self, cancel: &CancelToken) -> Result<String, CommandError> {
        let stdout = self.execute(Stdio::piped(), cancel)?;
        Ok(String::from_utf8(stdout).unwrap_or_else(|err| {
            warn!("Output of the command is not valid UTF-8");
            String::from_utf8_lossy(err.as_bytes()).into_owned()
        }))
    }

    /// Run the command with the terminal handed over to it, so it can page,
    /// colorize and prompt as it would outside the app, and return how it
    /// exited. The terminal is the command's to read and write as it sees
    /// fit, so [Self::stdin], [Self::color] and [Self::verbose] have no say
    /// here.
    pub fn run_foreground(self) -> io::Result<ExitStatus> {
        self.build_command().status()
    }

    /// Configure and run the command as a child process, as described in
    /// [run_child].
    fn execute(mut self, stdout: Stdio, cancel: &CancelToken) -> Result<Vec<u8>, CommandError> {
        let input = self.stdin.take().map(String::into_bytes);

        let mut command = self.build_command();
        command.args(get_output_args(
            !self.force_no_color && self.color,
            self.quiet,
        ));

        let Some(pager) = self.pager.take() else {
            return run_child(command, input, stdout, cancel, self.with_stderr);
        };

        // The pager is what produces the output, so what jj writes is
        // captured whatever the caller asked for.
        let piped = run_child(command, input, Stdio::piped(), cancel, self.with_stderr)?;

        run_child(
            self.build_pager_command(&pager),
            Some(piped),
            stdout,
            cancel,
            false,
        )
    }

    /// Construct a Command ready for execution. The caller adds the output
    /// args, which only suit a command whose output it captures.
    fn build_command(&self) -> Command {
        let mut command = Command::new(&self.jj_bin);
        command.args(&self.args);

        if self.ignore_working_copy {
            command.arg("--ignore-working-copy");
        }

        command.current_dir(&self.root);
        command.envs(self.env_var.iter().cloned());
        command
    }

    /// Construct a Command running the pager the output goes through
    fn build_pager_command(&self, pager: &DiffPager) -> Command {
        let mut command = Command::new(pager.program());
        command.args(pager.args(self.columns.unwrap_or(0)));
        command.current_dir(&self.root);
        command.envs(self.env_var.iter().cloned());
        command
    }
}

/// Run `command` in a child process `cancel` can kill, blocking until it is
/// done, and return its captured standard output.
///
/// `input` is fed to the child on standard input, if there is any. `stdout`
/// selects how the child's standard output is handled: piped to be captured
/// and returned, or null to be discarded. Standard error is always captured
/// so it can be surfaced on failure, and `with_stderr` appends it to the
/// output of a child that succeeded.
fn run_child(
    mut command: Command,
    input: Option<Vec<u8>>,
    stdout: Stdio,
    cancel: &CancelToken,
    with_stderr: bool,
) -> Result<Vec<u8>, CommandError> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(stdout)
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(CommandError::Spawn)?;

    let stdin_writer = input.map(|input| spawn_stdin_writer(&mut child, input));
    let child_stdout = child.stdout.take();
    let mut child_stderr = child.stderr.take().expect("stderr was piped");
    // From here on the child can be killed, so every pipe has to be
    // drained even when the command is going nowhere.
    cancel.register(child);

    let stderr_reader = thread::spawn(move || {
        let mut stderr = Vec::new();
        if let Err(err) = child_stderr.read_to_end(&mut stderr) {
            error!("Failed to read stderr of child process: {err}");
        }
        stderr
    });

    let mut output = Vec::new();
    // Every thread and the child have to be collected below whatever
    // went wrong, so failures are held back rather than returned here.
    let read_result = child_stdout
        .map(|mut pipe| pipe.read_to_end(&mut output))
        .transpose();
    let stderr = stderr_reader.join().unwrap_or_default();
    // The child is done reading its input too, either because it read
    // everything or because it exited and closed the pipe.
    let write_result = stdin_writer.map(join_stdin_writer).transpose();

    // Every pipe is closed, so the child has finished and there is
    // nothing left to wait for.
    let mut child = cancel
        .take_child()
        .expect("the child registered above is only taken here");
    let status = child.wait()?;

    if !status.success() {
        return Err(CommandError::Status(
            String::from_utf8_lossy(&stderr).into_owned(),
            status.code(),
        ));
    }
    // A pipe that broke along the way only matters for a command that
    // succeeded; for one that failed, its status and stderr say more.
    write_result?;
    read_result?;
    if with_stderr {
        output.extend_from_slice(&stderr);
    } else if !stderr.is_empty() {
        warn!(
            "Ignoring stderr of successful command:\n{}",
            String::from_utf8_lossy(&stderr)
        );
    }

    Ok(output)
}

/// Feed `input` to a child's standard input from a thread of its own, so
/// neither side deadlocks on a full pipe buffer. Dropping the handle closes
/// the pipe, signalling EOF to the child.
fn spawn_stdin_writer(child: &mut Child, input: Vec<u8>) -> JoinHandle<io::Result<()>> {
    let mut stdin = child.stdin.take().expect("stdin was piped");
    thread::spawn(move || stdin.write_all(&input))
}

/// Collect the outcome of a [spawn_stdin_writer] thread. A broken pipe means
/// the child exited before reading its input, which the caller's status check
/// describes better than this would.
fn join_stdin_writer(writer: JoinHandle<io::Result<()>>) -> Result<(), CommandError> {
    match writer.join().expect("stdin writer thread panicked") {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => Err(err.into()),
        _ => Ok(()),
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
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;
    use crate::commander::bookmarks::Bookmark;
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
                config: toml::Table::new(),
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

        /// The bookmarks as the set-bookmark dialog would list them for
        /// the change the working copy is on.
        pub fn bookmarks(&self) -> Result<Vec<Bookmark>> {
            let on = self.commander.get_current_head()?.commit_id;

            Ok(self.commander.get_bookmarks_list(false, &on)?)
        }
    }

    #[test]
    fn test_repo() -> Result<()> {
        apply_common_filters!();

        let test_repo = TestRepo::new()?;

        test_repo.commander.jj(["status"]).color().run()?;

        Ok(())
    }

    #[test]
    fn run_foreground_reports_how_the_command_exited() -> Result<()> {
        let test_repo = TestRepo::new()?;

        // The command writes to the terminal the test runs in, so it has
        // to be one that says nothing in a fresh repo.
        let status = test_repo
            .commander
            .jj(["bookmark", "list"])
            .run_foreground()?;
        assert!(status.success());

        Ok(())
    }

    #[test]
    fn a_command_that_wants_an_editor_fails_instead_of_waiting_for_one() -> Result<()> {
        let test_repo = TestRepo::new()?;

        // Should the command get an editor after all, it waits on it for
        // as long as this test is left to run.
        let cancel = CancelToken::new();
        let watchdog = cancel.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(30));
            watchdog.cancel();
        });

        let err = test_repo
            .commander
            .jj(["describe"])
            .no_editor()
            .run_cancellable(&cancel)
            .expect_err("describe has no editor to open");

        assert!(err.to_string().contains(NO_EDITOR), "{err}");

        Ok(())
    }

    #[test]
    fn run_cancellable_returns_the_output() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let output = test_repo
            .commander
            .jj(["status"])
            .run_cancellable(&CancelToken::new())?;

        assert!(output.contains("The working copy has no changes"));

        Ok(())
    }

    #[test]
    fn run_cancellable_reports_stderr_of_a_failing_command() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let error = test_repo
            .commander
            .jj(["log", "-r", "nope()"])
            .run_cancellable(&CancelToken::new())
            .expect_err("an unknown revset function is an error");

        let CommandError::Status(stderr, code) = error else {
            panic!("expected a non-zero exit status");
        };
        assert!(stderr.contains("nope"), "stderr is not reported: {stderr}");
        assert_eq!(code, Some(1));

        Ok(())
    }

    #[test]
    fn run_cancellable_feeds_stdin_to_the_command() -> Result<()> {
        let test_repo = TestRepo::new()?;

        test_repo
            .commander
            .jj(["describe", "--stdin"])
            .stdin("message from stdin")
            .run_cancellable(&CancelToken::new())?;

        let description = test_repo
            .commander
            .jj(["log", "-r", "@", "--no-graph", "-T", "description"])
            .run()?;
        assert_eq!(description.remove_end_line(), "message from stdin");

        Ok(())
    }

    /// The pager the given `blazingjj.diff-pager` value configures
    pub fn pager(setting: &str) -> DiffPager {
        toml::from_str::<JjConfig>(&format!("blazingjj.diff-pager = {setting}\n"))
            .expect("the setting is a valid configuration")
            .diff_pager()
            .expect("the setting configures a pager")
    }

    #[test]
    fn a_pager_renders_what_the_command_wrote() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let output = test_repo
            .commander
            .jj(["status"])
            .pipe_to(Some(&pager(r#"["sed", "s/^/rendered: /"]"#)))
            .run()?;

        assert!(
            output.lines().all(|line| line.starts_with("rendered: ")),
            "the output did not go through the pager: {output}"
        );

        Ok(())
    }

    #[test]
    fn a_pager_that_fails_fails_the_command() -> Result<()> {
        let test_repo = TestRepo::new()?;

        let error = test_repo
            .commander
            .jj(["status"])
            .pipe_to(Some(&pager(r#"["sed", "-e", "s/"]"#)))
            .run()
            .expect_err("an incomplete script is an error");

        let CommandError::Status(stderr, _) = error else {
            panic!("expected a non-zero exit status");
        };
        assert!(stderr.contains("sed"), "stderr is not reported: {stderr}");

        Ok(())
    }
}
