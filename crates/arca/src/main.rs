//! Arca — a fast file manager for Wayland,
//! built on the optics stack (flux/lens/iris).
//!
//! `main` only performs the side effects that [`cli::Command`] describes;
//! every argument combination is decided in `cli` and unit-tested there.

mod cli;

use arca_engine::chooser::{
    read_prompter_request, write_prompter_response, BytePath, FileChooserRequest,
    FileChooserResponse,
};
use arca_engine::{AppState, ViewMode};

use cli::{Command, StartDir};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = Command::parse(&args, arca_ui::ipc::try_forward_to_existing);

    for option in Command::unknown_options(&args) {
        eprintln!("arca: unknown option: {option}");
    }

    match command {
        Command::Help => print!("{}", cli::USAGE),
        Command::Version => println!("arca {}", env!("CARGO_PKG_VERSION")),
        Command::Prompter => run_prompter_mode(),
        Command::Chooser(invocation) => run_chooser_mode(invocation),
        // `parse` only returns this when a live instance accepted the path.
        Command::Forward { .. } => {}
        Command::Window { view_mode, start } => run_window(view_mode, start),
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
        eprintln!("arca: {error}");
        std::process::exit(1);
    }
}

/// Run as a one-shot portal prompter process speaking JSON on stdin/stdout.
fn run_prompter_mode() {
    let (request, appearance) = match read_prompter_request(std::io::stdin()) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("arca: invalid prompter request: {error}");
            let _ = write_prompter_response(
                std::io::stdout(),
                FileChooserResponse::Failed { message: error },
            );
            std::process::exit(1);
        }
    };

    let response = match arca_ui::run_chooser(request.clone(), appearance) {
        // Validate our own answer against the contract before emitting it: a
        // prompter that returns the wrong number of paths, a filter it was
        // not offered, or a choice id the caller never defined is worse than
        // an explicit failure, which lets the daemon fall back.
        Ok(response) => match response.validate_for(&request) {
            Ok(()) => response,
            Err(message) => FileChooserResponse::Failed {
                message: format!("prompter produced an invalid response: {message}"),
            },
        },
        Err(error) => FileChooserResponse::Failed {
            message: format!("GUI run failed: {error}"),
        },
    };

    if let Err(error) = write_prompter_response(std::io::stdout(), response) {
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

    match arca_ui::run_chooser(request, None) {
        Ok(FileChooserResponse::Selected { paths, .. }) => {
            for path in paths {
                println!("{}", path.to_path_buf().display());
            }
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
