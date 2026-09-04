/*! Running a program, which is what every command of the app comes down
to.

A [Program] describes what to run, with what and where. It is run with
the terminal handed over to it ([Program::run_foreground]), run with what
it writes captured ([Program::run_captured]), left running on its own
([Program::run_detached]), or turned into a [Command]
([Program::command]) for a caller that drives the child process itself.
*/

use std::ffi::OsStr;
use std::ffi::OsString;
use std::io;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::thread;

use crate::commander::CommandError;
use crate::commander::cancel::CancelToken;
use crate::commander::run_child;

/// A program to run, with the arguments and the environment it is to run
/// with.
#[derive(Clone, Debug)]
pub struct Program {
    program: OsString,
    args: Vec<OsString>,
    /// The working directory the program is run in.
    dir: PathBuf,
    env: Vec<(String, String)>,
}

impl Program {
    /// The program `program`, run in `dir` with no arguments.
    pub fn new(program: impl AsRef<OsStr>, dir: impl Into<PathBuf>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            dir: dir.into(),
            env: Vec::new(),
        }
    }

    /// The program with `args` appended to the arguments it has so far.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self
    }

    /// The program with the given variables set in its environment, on
    /// top of the one the app was started with.
    pub fn envs(mut self, env: impl IntoIterator<Item = (String, String)>) -> Self {
        self.env.extend(env);
        self
    }

    /// The arguments it is to run with, for a test checking what a
    /// program was built to run.
    #[cfg(test)]
    pub(crate) fn args_text(&self) -> Vec<String> {
        self.args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    /// The program as a command to configure and run, for a caller that
    /// handles the child process itself.
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command.current_dir(&self.dir);
        command.envs(self.env.iter().cloned());
        command
    }

    /// Run the program with the terminal handed over to it, so it can
    /// page, colorize and prompt as it would outside the app, and return
    /// how it exited.
    pub fn run_foreground(&self) -> io::Result<ExitStatus> {
        self.command().status()
    }

    /// Run the program and return what it wrote, on standard error after
    /// standard output, for a caller that puts up whatever it has to say
    /// rather than parsing it.
    ///
    /// Bytes that are not UTF-8 become replacement characters, so the
    /// output is fit to put on screen but not to parse.
    pub fn run_captured(&self) -> Result<String, CommandError> {
        let written = run_child(self, None, Stdio::piped(), &CancelToken::new(), true)?;

        Ok(String::from_utf8_lossy(&written).into_owned())
    }

    /// Start the program and leave it running on its own, with neither
    /// the terminal nor anything to say to the app.
    ///
    /// It is put in a process group of its own, so that the keys the app
    /// answers to are not read as signals by it, and waited on in a
    /// thread of its own, so that it is reaped whenever it exits.
    pub fn run_detached(&self) -> io::Result<()> {
        let mut command = self.command();
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        command.process_group(0);

        let mut child = command.spawn()?;
        thread::spawn(move || child.wait());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use std::time::Instant;

    use anyhow::Result;

    use super::*;

    #[test]
    fn run_foreground_reports_how_the_program_exited() -> Result<()> {
        let program = Program::new("false", ".");

        assert!(!program.run_foreground()?.success());

        Ok(())
    }

    #[test]
    fn a_program_that_is_not_there_fails_to_start() {
        assert!(
            Program::new("blazingjj-no-such-program", ".")
                .run_foreground()
                .is_err()
        );
        assert!(
            Program::new("blazingjj-no-such-program", ".")
                .run_detached()
                .is_err()
        );
    }

    /// A detached program is the app's only in that it started it, so
    /// the app carries on while it runs.
    #[test]
    fn a_detached_program_is_not_waited_for() -> Result<()> {
        let started = Instant::now();

        Program::new("sleep", ".").args(["3"]).run_detached()?;

        assert!(started.elapsed() < Duration::from_secs(1));

        Ok(())
    }
}
