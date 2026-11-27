//! Arca — a fast file manager for Wayland,
//! built on the optics stack (optics-app/optics-gfx/optics-ui/optics-window).
//!
//! `main` only performs the side effects that [`cli::Command`] describes;
//! every argument combination is decided in `cli` and unit-tested there.

mod cli;

use arca_engine::chooser::{
    BytePath, FileChooserRequest, FileChooserResponse, read_chooser_request, write_chooser_response,
};
use arca_engine::telemetry::{self, Role, diagnostics};
use arca_engine::{AppState, ViewMode};

use cli::{Command, StartDir};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = Command::parse(&args, arca_ui::ipc::try_forward_to_existing);

    // Long-running surfaces get the logger, watchdog, and (window only) the
    // diagnostics writer. One-shot commands stay side-effect free. The guard
    // lives until `main` returns so the final snapshot is written on exit.
    let role = match &command {
        Command::Window { .. } => Some(Role::Window),
        Command::Prompter => Some(Role::Prompter),
        Command::Chooser(_) => Some(Role::Chooser),
        _ => None,
    };
    let _telemetry = role.map(|role| telemetry::init(telemetry::Options::from_env(role)));

    let unknown = Command::unknown_options(&args);
    for option in &unknown {
        eprintln!("arca: unknown option: {option}");
        log::warn!(target: "arca", "unknown option: {option}");
    }
    if !unknown.is_empty() && !matches!(command, Command::Help | Command::Version) {
        log::logger().flush();
        std::process::exit(2);
    }

    match command {
        Command::Help => print!("{}", cli::USAGE),
        Command::Version => println!("arca {}", env!("CARGO_PKG_VERSION")),
        Command::Diagnostics => print_diagnostics(),
        Command::Prompter => run_prompter_mode(),
        Command::Chooser(invocation) => run_chooser_mode(invocation),
        // `parse` only returns this when a live instance accepted the path.
        Command::Forward { .. } => {}
        Command::Window { view_mode, start } => run_window(view_mode, start),
    }
}

/// `--diagnostics`: render the window's latest snapshot. Exit 1 when none
/// has been written yet (no window has run under this `$XDG_STATE_HOME`).
fn print_diagnostics() {
    let path = diagnostics::path_for(&telemetry::state_dir(), Role::Window);
    match diagnostics::read(&path) {
        Ok(report) => {
            let alive = diagnostics::writer_alive(&report);
            print!("{}", diagnostics::render_text(&report, alive));
            print_stall_captures();
            println!("\nSnapshot: {}", path.display());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "arca: no diagnostics recorded yet ({} does not exist); open a window first",
                path.display()
            );
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("arca: cannot read {}: {error}", path.display());
            std::process::exit(1);
        }
    }
}

/// The most recent stall captures (newest last), if any were written.
fn print_stall_captures() {
    const SHOWN: usize = 5;
    #[cfg(unix)]
    {
        let dir = telemetry::capture::dir_for(&telemetry::state_dir());
        let captures = telemetry::capture::list(&dir).unwrap_or_default();
        if captures.is_empty() {
            return;
        }
        println!("\nStall captures ({} in {})", captures.len(), dir.display());
        for capture in captures.iter().rev().take(SHOWN).rev() {
            println!("  {}", capture.display());
        }
    }
}

/// Open the interactive file manager.
fn run_window(view_mode: Option<ViewMode>, start: StartDir) {
    let mut state = AppState::new();
    // A CLI flag is a launch-time override, not a preference: persisting it
    // would rewrite the user's config on every flag-bearing invocation.
    if let Some(mode) = view_mode {
        state.override_view_mode(mode);
    }
    if let StartDir::Path(path) = start {
        state.navigate(&path);
    }
    if let Err(error) = arca_ui::run(state) {
        // `arca_ui::run` already logged it; stderr is for the terminal user.
        eprintln!("arca: {error}");
        log::logger().flush();
        std::process::exit(1);
    }
}

/// Run as a one-shot portal chooser process speaking JSON on stdin/stdout.
fn run_prompter_mode() {
    let request = match read_chooser_request(std::io::stdin()) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("arca: invalid chooser request: {error}");
            let _ = write_chooser_response(
                std::io::stdout(),
                FileChooserResponse::Failed { message: error },
            );
            std::process::exit(1);
        }
    };

    let response = match arca_ui::run_chooser(request.clone()) {
        // Validate our own answer against the contract before emitting it: a
        // chooser that returns the wrong number of paths, a filter it was
        // not offered, or a choice id the caller never defined is worse than
        // an explicit failure, which lets the daemon fall back.
        Ok(response) => match response.validate_for(&request) {
            Ok(()) => response,
            Err(message) => FileChooserResponse::Failed {
                message: format!("chooser produced an invalid response: {message}"),
            },
        },
        Err(error) => FileChooserResponse::Failed {
            message: format!("GUI run failed: {error}"),
        },
    };

    if let Err(error) = write_chooser_response(std::io::stdout(), response) {
        eprintln!("arca: failed to write response: {error}");
        std::process::exit(1);
    }
}

/// Run as a CLI standalone picker and output chosen paths to stdout.
fn run_chooser_mode(invocation: cli::ChooserInvocation) {
    let request = FileChooserRequest {
        mode: invocation.mode,
        app_id: "io.arca.Chooser".into(),
        title: invocation.title.unwrap_or_default(),
        accept_label: None,
        modal: false,
        parent_window: None,
        multiple: invocation.multiple,
        current_folder: invocation.start_dir.as_deref().map(BytePath::from),
        current_name: invocation.name,
        current_file: None,
        filters: Vec::new(),
        current_filter: None,
        choices: Vec::new(),
        files: Vec::new(),
    };

    match arca_ui::run_chooser(request) {
        Ok(FileChooserResponse::Selected { paths, .. }) => {
            use std::io::Write as _;
            let mut stdout = std::io::stdout().lock();
            for path in paths {
                if invocation.null_delimited {
                    let _ = stdout.write_all(&path.0);
                    let _ = stdout.write_all(&[0]);
                } else {
                    let _ = stdout.write_all(&path.0);
                    let _ = stdout.write_all(b"\n");
                }
            }
            let _ = stdout.flush();
        }
        Ok(FileChooserResponse::Cancelled) => std::process::exit(1),
        Ok(FileChooserResponse::Failed { message }) => {
            eprintln!("arca: {message}");
            std::process::exit(2);
        }
        Err(error) => {
            eprintln!("arca: {error}");
            std::process::exit(2);
        }
    }
}
