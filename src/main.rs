extern crate thiserror;

use std::env::current_dir;
use std::fs::OpenOptions;
use std::fs::canonicalize;
use std::io::ErrorKind;
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
use ratatui::crossterm::event::{self};
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
mod commander;
mod env;
mod keybinds;
mod ui;

use crate::app::App;
use crate::commander::Commander;
use crate::env::Env;
use crate::env::set_env;
use crate::ui::ComponentAction;
use crate::ui::ui;

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

    // Setup app
    let mut app = App::new()?;

    install_panic_hook();
    let mut terminal = setup_terminal()?;

    // Run app
    let res = run_app(&mut terminal, &mut app);
    restore_terminal()?;
    res?;

    Ok(())
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
            canonicalize(&path).with_context(|| format!("Could not find path {}", &path))?
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

fn run_app(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        app.update()?;
        terminal.draw(|f| {
            let _ = ui(f, app);
        })?;

        let should_stop = input_to_app(app)?;

        if should_stop {
            return Ok(());
        }
    }
}

/// Let app process all input events in queue before returning
/// to draw the next frame.
/// Return true if application should stop
fn input_to_app(app: &mut App) -> Result<bool> {
    // Duration::MAX overflows the timespec struct used by kevent/kqueue on macOS,
    // causing EINVAL (os error 22). Use a safe large value instead.
    const FOREVER: Duration = Duration::from_secs(24 * 3600);

    // Allow popups like the fetch animation to update every 100ms.
    let wait_duration = if app.popup.is_some() {
        Duration::from_millis(100)
    } else {
        FOREVER
    };
    // If no event arrives, return and draw next frame.
    let event_arrived = event::poll(wait_duration)?;
    app.stats.start_time = Instant::now();
    if !event_arrived {
        return Ok(false);
    }

    // Handle all pending events in the queue.
    // Stop if an event requested the app to stop.
    let mut should_stop: bool = false;
    while event::poll(Duration::ZERO)? && !should_stop {
        let event = event::read()?;
        should_stop = app.input(event)?;
    }
    Ok(should_stop)
}

fn setup_terminal() -> Result<DefaultTerminal> {
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

    let backend = CrosstermBackend::new(stdout);
    Ok(DefaultTerminal::new(backend)?)
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

enum ComponentInputResult {
    Handled,
    HandledAction(ComponentAction),
    NotHandled,
}

impl ComponentInputResult {
    pub fn is_handled(&self) -> bool {
        match self {
            ComponentInputResult::Handled => true,
            ComponentInputResult::HandledAction(_) => true,
            ComponentInputResult::NotHandled => false,
        }
    }
}
