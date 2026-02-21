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
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::DisableFocusChange;
use ratatui::crossterm::event::DisableMouseCapture;
use ratatui::crossterm::event::EnableFocusChange;
use ratatui::crossterm::event::EnableMouseCapture;
use ratatui::crossterm::event::Event;
use ratatui::crossterm::event::KeyboardEnhancementFlags;
use ratatui::crossterm::event::MouseEvent;
use ratatui::crossterm::event::MouseEventKind;
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
use crate::ui::ComponentAction;
use crate::ui::ui;

/// Simple program to greet a person
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

    let should_trace = std::env::var("BLAZINGJJ_TRACE")
        .map(|log| log == "1" || log.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let (trace_layer, _guard) = if should_trace {
        let (chrome_layer, _guard) = ChromeLayerBuilder::new().build();
        (Some(chrome_layer), Some(_guard))
    } else {
        (None, None)
    };

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

    // Setup environment
    let env = Env::new(path, args.revisions, jj_bin)?;
    let mut commander = Commander::new(&env);

    if !args.ignore_jj_version {
        commander.check_jj_version()?;
    }

    // Setup app
    let mut app = App::new(env.clone())?;

    let mut terminal = setup_terminal()?;
    install_panic_hook();

    // Run app
    let res = run_app(&mut terminal, &mut app, &mut commander);
    restore_terminal()?;
    res?;

    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    commander: &mut Commander,
) -> Result<()> {
    let mut wait_duration = Duration::from_millis(0);
    loop {
        if event::poll(wait_duration)? {
            match event::read()? {
                event::Event::FocusLost => continue,
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Moved,
                    ..
                }) => continue,
                event => {
                    app.stats.start_time = Instant::now();
                    if app.input(event, commander)? {
                        return Ok(());
                    }
                }
            }
        }

        app.update(commander)?;
        terminal.draw(|f| {
            let _ = ui(f, app);
        })?;

        // Allow popups like the fetch animation to update every 100ms, if there is no popup, just
        // wait for an incoming event
        wait_duration = if app.popup.is_none() {
            Duration::MAX
        } else {
            Duration::from_millis(100)
        };
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
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
    Ok(Terminal::new(backend)?)
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
