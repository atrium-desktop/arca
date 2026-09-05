//! Arca — a fast file manager for Wayland,
//! built on the optics stack (flux/lens/iris).

use arca_engine::chooser::{
    read_prompter_request, write_prompter_response, BytePath, FileChooserMode, FileChooserRequest,
    FileChooserResponse,
};
use arca_engine::{AppState, ViewMode};

fn main() {
    let mut args = std::env::args().skip(1).peekable();

    // Check if prompter mode was requested
    if std::env::args().any(|arg| arg == "--chooser-prompt" || arg == "--prompter") {
        run_prompter_mode();
        return;
    }

    // Check if CLI chooser mode was requested
    if let Some(first) = args.peek() {
        if first == "--choose-file"
            || first == "--choose-files"
            || first == "--choose-dir"
            || first == "--save-file"
        {
            let mode_flag = args.next().unwrap();
            run_cli_chooser_mode(mode_flag, args);
            return;
        }
    }

    let mut state = AppState::new();
    for argument in args {
        match argument.as_str() {
            "-h" | "--help" => {
                println!(
                    "Arca file manager\n\n\
                     Usage:\n  \
                       arca [--grid|--list|--miller] [DIRECTORY]\n  \
                       arca --chooser-prompt\n  \
                       arca --choose-file [--title TITLE] [DIRECTORY]\n  \
                       arca --choose-files [--title TITLE] [DIRECTORY]\n  \
                       arca --choose-dir [--title TITLE] [DIRECTORY]\n  \
                       arca --save-file [--name NAME] [--title TITLE] [DIRECTORY]"
                );
                return;
            }
            "-V" | "--version" => {
                println!("arca {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--grid" => state.view_mode = ViewMode::Grid,
            "--list" => state.view_mode = ViewMode::List,
            "--miller" => state.view_mode = ViewMode::Miller,
            path if !path.starts_with('-') => state.navigate(path),
            _ => eprintln!("arca: unknown option: {argument}"),
        }
    }
    if let Err(e) = arca_ui::run(state) {
        eprintln!("arca: {e}");
        std::process::exit(1);
    }
}

/// Run as a one-shot portal prompter process speaking JSON on stdin/stdout.
fn run_prompter_mode() {
    let (request, appearance) = match read_prompter_request(std::io::stdin()) {
        Ok(req) => req,
        Err(error) => {
            eprintln!("arca: invalid prompter request: {error}");
            let _ = write_prompter_response(
                std::io::stdout(),
                FileChooserResponse::Failed {
                    message: error,
                },
            );
            std::process::exit(1);
        }
    };

    let response = match arca_ui::run_chooser(request, appearance) {
        Ok(res) => res,
        Err(e) => FileChooserResponse::Failed {
            message: format!("GUI run failed: {e}"),
        },
    };

    if let Err(e) = write_prompter_response(std::io::stdout(), response) {
        eprintln!("arca: failed to write response: {e}");
        std::process::exit(1);
    }
}

/// Run as a CLI standalone picker and output chosen paths to stdout.
fn run_cli_chooser_mode(mode_flag: String, mut args: impl Iterator<Item = String>) {
    let mode = match mode_flag.as_str() {
        "--choose-file" => FileChooserMode::OpenFile,
        "--choose-files" => FileChooserMode::OpenFile,
        "--choose-dir" => FileChooserMode::OpenDirectory,
        "--save-file" => FileChooserMode::SaveFile,
        _ => unreachable!(),
    };
    let multiple = mode_flag == "--choose-files";

    let mut title = String::new();
    let mut current_name = None;
    let mut start_dir = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--title" => {
                if let Some(t) = args.next() {
                    title = t;
                }
            }
            "--name" => {
                if let Some(n) = args.next() {
                    current_name = Some(n);
                }
            }
            dir if !dir.starts_with('-') => {
                start_dir = Some(BytePath::from_path(dir));
            }
            _ => {}
        }
    }

    let request = FileChooserRequest {
        mode,
        app_id: "io.arca.Chooser".into(),
        title,
        accept_label: None,
        modal: false,
        parent_window: None,
        multiple,
        current_folder: start_dir,
        current_name,
        current_file: None,
        filters: Vec::new(),
        current_filter: None,
        choices: Vec::new(),
        files: Vec::new(),
    };

    match arca_ui::run_chooser(request, None) {
        Ok(FileChooserResponse::Selected { paths, .. }) => {
            for p in paths {
                println!("{}", p.to_path_buf().display());
            }
        }
        Ok(FileChooserResponse::Cancelled) => {
            std::process::exit(1);
        }
        Ok(FileChooserResponse::Failed { message }) => {
            eprintln!("arca: {message}");
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("arca: {e}");
            std::process::exit(2);
        }
    }
}
