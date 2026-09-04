extern crate thiserror;

use std::env::current_dir;
use std::env::current_exe;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::fs::canonicalize;
use std::io::ErrorKind;
use std::io::Write;
use std::io::{self};
use std::process::Command;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::DisableFocusChange;
use ratatui::crossterm::event::DisableMouseCapture;
use ratatui::crossterm::event::EnableFocusChange;
use ratatui::crossterm::event::EnableMouseCapture;
use ratatui::crossterm::event::KeyboardEnhancementFlags;
use ratatui::crossterm::event::PopKeyboardEnhancementFlags;
use ratatui::crossterm::event::PushKeyboardEnhancementFlags;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::EnterAlternateScreen;
use ratatui::crossterm::terminal::LeaveAlternateScreen;
use ratatui::crossterm::terminal::disable_raw_mode;
use ratatui::crossterm::terminal::enable_raw_mode;
use ratatui::crossterm::terminal::supports_keyboard_enhancement;
use tracing::info;
use tracing_chrome::ChromeLayerBuilder;
use tracing_subscriber::layer::SubscriberExt;

mod app;
mod background_tasks;
mod commander;
mod env;
mod event;
mod interrupt;
mod keybinds;
mod settings;
mod ui;
use crate::app::App;
use crate::app::Handled;
use crate::commander::Commander;
use crate::commander::new_commander;
use crate::env::Env;
use crate::env::set_env;
use crate::interrupt::catch_interrupts;
use crate::interrupt::watch_for_interrupts;
use crate::ui::Interactive;

/// Command line arguments
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to jj repo. Defaults to current directory
    #[arg(short, long)]
    path: Option<String>,

    /// Default revset
    #[arg(short, long)]
    revisions: Option<String>,

    /// Path to jj binary
    #[arg(long, env = "JJ_BIN")]
    jj_bin: Option<String>,

    /// Do not exit if jj version check fails
    #[arg(long)]
    ignore_jj_version: bool,
}

fn main() -> Result<()> {
    // Setup environment
    set_env(init_env()?);

    // A stale working copy has to be dealt with before anything can
    // read the repo.
    if !update_stale_workspace()? {
        return Ok(());
    }

    // Setup app
    let mut app = App::new()?;

    install_panic_hook();
    watch_for_interrupts()?;
    let mut terminal = create_terminal()?;
    setup_terminal()?;

    // Run app
    let res = run_app(&mut terminal, &mut app);
    restore_terminal()?;

    if let Some(root) = res? {
        restart_in(&root)?;
    }

    Ok(())
}

/// Run in the workspace at `root` in place of this run, which is what
/// working in another workspace comes to: every command of ours goes to
/// the workspace we were started in, as does the working copy the tabs
/// show, so we start again rather than take another one over.
fn restart_in(root: &str) -> Result<()> {
    let program = current_exe().context("Could not find the program to start again")?;
    let mut command = Command::new(program);
    command
        .arg("--path")
        .arg(root)
        .args(args_beside_path(std::env::args_os().skip(1)));

    #[cfg(unix)]
    {
        // Nothing of ours is worth keeping around for the workspace we
        // are leaving, so the new run takes this process over.
        use std::os::unix::process::CommandExt;

        Err(command.exec().into())
    }
    #[cfg(not(unix))]
    {
        // Without exec we stay around until the new run is done, having
        // handed it the terminal.
        let status = command.status().context("Could not start again")?;

        std::process::exit(status.code().unwrap_or(0));
    }
}

/// The arguments we were given, without the one naming the workspace:
/// the workspace to start in takes its place. Both spellings of the
/// option go, as does the value of one given as an argument of its own.
fn args_beside_path(args: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let mut kept = Vec::new();
    let mut drop_value = false;

    for arg in args {
        if drop_value {
            drop_value = false;
            continue;
        }
        if arg == "--path" || arg == "-p" {
            drop_value = true;
            continue;
        }

        // A value given with the option is part of the same argument,
        // which for the short option is whatever follows it.
        let text = arg.to_string_lossy();
        if text.starts_with("--path=") || text.starts_with("-p") {
            continue;
        }

        kept.push(arg);
    }

    kept
}

/// Examine environment variables and command line arguments
/// and perform basic initialisation
fn init_env() -> Result<Env> {
    // Configure tracing to log file
    let should_log = std::env::var("BLAZINGJJ_LOG")
        .map(|log| log == "1" || log.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let log_layer = if should_log {
        let log_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open("blazingjj.log")
            .unwrap();

        Some(
            tracing_subscriber::fmt::layer()
                .compact()
                .with_writer(log_file)
                // Add log when span ends with their duration
                .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE),
        )
    } else {
        None
    };

    // Configure tracing to Chrome
    let should_trace = std::env::var("BLAZINGJJ_TRACE")
        .map(|log| log == "1" || log.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let (trace_layer, _guard) = if should_trace {
        let (chrome_layer, _guard) = ChromeLayerBuilder::new().build();
        (Some(chrome_layer), Some(_guard))
    } else {
        (None, None)
    };

    // Set up tracing
    let subscriber = tracing_subscriber::Registry::default()
        .with(log_layer)
        .with(trace_layer);
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting blazingjj");

    // Parse arguments and determine path
    let args = Args::parse();
    let path = match args.path {
        Some(path) => {
            canonicalize(&path).with_context(|| format!("Could not find path {}", path))?
        }
        None => current_dir()?,
    };

    let jj_bin = args.jj_bin.unwrap_or("jj".to_string());

    // Check that jj exists
    if let Err(err) = Command::new(&jj_bin).arg("help").output()
        && err.kind() == ErrorKind::NotFound
    {
        bail!(
            "jj command not found. Please make sure it is installed: https://martinvonz.github.io/jj/latest/install-and-setup"
        );
    }

    // Check that jj is recent enough
    let env = Env::new(path, args.revisions, jj_bin)?;

    if !args.ignore_jj_version {
        let commander = Commander::new(&env);
        commander.check_jj_version()?;
    }

    // Return initialized environment
    Ok(env)
}

/// Offer to update the working copy if it is stale, which jj refuses to
/// read the repo until, and report whether the app can go on.
fn update_stale_workspace() -> Result<bool> {
    let commander = new_commander();
    if !commander.is_workspace_stale() {
        return Ok(true);
    }

    println!("The working copy is stale: the repo has moved on since it was last updated.");
    print!("Update it now? [y/N] ");
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes") {
        return Ok(false);
    }

    print!("{}", commander.update_stale_workspace()?);

    Ok(true)
}

/// Run the app until it is done, and report the workspace it is to be
/// started again in, if it was asked to move to one.
fn run_app(terminal: &mut DefaultTerminal, app: &mut App) -> Result<Option<String>> {
    app.launch_input_channel();
    let mut quiet = false;
    loop {
        let mut changed = app.update()?;

        changed |= app.refresh_view()?;

        // Working in another workspace is the app coming down, so it
        // leaves the loop rather than drawing anything else.
        if let Some(root) = app.take_pending_restart() {
            return Ok(Some(root));
        }

        // Before the wait below, which anything but user input may be a
        // long time coming to.
        if let Some(interactive) = app.take_pending_interactive() {
            run_interactive(terminal, app, interactive)?;
            quiet = false;
            continue;
        }

        // Waking up on a timer to find nothing has happened must not
        // cost a frame, or an app nobody is touching would rebuild the
        // whole display every poll interval.
        if changed || !quiet {
            terminal.draw(|f| {
                let _ = app.draw(f, f.area());
            })?;
        }

        match input_to_app(app)? {
            Handled::Stop => return Ok(None),
            Handled::Redraw => quiet = false,
            Handled::Nothing => quiet = true,
        }
    }
}

/// Hand the terminal over to a command and take it back once it is done.
fn run_interactive(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    interactive: Interactive,
) -> Result<()> {
    app.pause_input();
    restore_terminal()?;

    let ran = {
        let _interrupts = catch_interrupts();
        match interactive.program.run_foreground() {
            Ok(status) => status.success(),
            Err(err) => {
                println!("Could not run the command: {err}");
                false
            }
        }
    };
    // What the command left is on the screen we are about to take back,
    // so it has to be read before we do.
    if !ran || interactive.hold_screen {
        wait_for_user()?;
    }

    setup_terminal()?;
    terminal.clear()?;
    app.resume_input();
    app.catch_up_with_repo()
}

/// Let app process all input events in queue before returning.
fn input_to_app(app: &mut App) -> Result<Handled> {
    // Duration::MAX overflows the timespec struct used by kevent/kqueue on macOS,
    // causing EINVAL (os error 22). Use a safe large value instead.
    const FOREVER: Duration = Duration::from_secs(24 * 3600);

    // Something that counts up on its own needs a frame every 100ms.
    // Otherwise the app may be due to check for work done outside it.
    // With neither, everything is delivered on the event channel, so
    // there is nothing to wake up for.
    let wait_duration = if app.needs_periodic_redraw() {
        Duration::from_millis(100)
    } else {
        app.time_until_poll()
            .map_or(FOREVER, |until| until.min(FOREVER))
    };

    // Handle all pending events in the queue.
    // Stop if an event requested the app to stop.
    let mut event = app.try_recv_app_event(wait_duration);
    app.stats.start_time = Instant::now();
    let mut changed = false;
    while let Some(next) = event.take() {
        match app.input(next)? {
            Handled::Stop => return Ok(Handled::Stop),
            Handled::Redraw => changed = true,
            Handled::Nothing => {}
        }
        event = app.try_recv_app_event(Duration::ZERO);
    }

    Ok(if changed {
        Handled::Redraw
    } else {
        Handled::Nothing
    })
}

fn create_terminal() -> Result<DefaultTerminal> {
    let backend = CrosstermBackend::new(io::stdout());
    Ok(DefaultTerminal::new(backend)?)
}

/// Leave the terminal as it is until the user says they are done with it.
fn wait_for_user() -> Result<()> {
    println!("\nPress enter to return to blazingjj");
    io::stdin().read_line(&mut String::new())?;
    Ok(())
}

fn setup_terminal() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableFocusChange
    )?;

    if supports_keyboard_enhancement()? {
        execute!(
            stdout,
            // required to properly detect ctrl+shift
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }

    Ok(())
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableFocusChange
    )?;

    if supports_keyboard_enhancement()? {
        execute!(stdout, PopKeyboardEnhancementFlags)?;
    }

    Ok(())
}

fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Err(err) = restore_terminal() {
            eprintln!("Failed to restore terminal: {err}");
        }
        original_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beside_path(args: &[&str]) -> Vec<String> {
        args_beside_path(args.iter().map(OsString::from))
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    /// The workspace we start again in is the one we were asked for, so
    /// the path we were started with goes, however it was given.
    #[test]
    fn starting_again_keeps_every_argument_but_the_path() {
        assert_eq!(
            beside_path(&["--path", "/repo", "-r", "@", "--ignore-jj-version"]),
            ["-r", "@", "--ignore-jj-version"]
        );
        assert_eq!(beside_path(&["-p", "/repo", "-r", "@"]), ["-r", "@"]);
        assert_eq!(beside_path(&["--path=/repo", "-r", "@"]), ["-r", "@"]);
        assert_eq!(beside_path(&["-p/repo", "-r", "@"]), ["-r", "@"]);
    }
}
