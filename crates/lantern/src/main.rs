//! Lantern — a fast file manager for Wayland,
//! built on the optics stack (flux/lens/iris).

use lantern_core::{AppState, ViewMode};

fn main() {
    let mut state = AppState::new();
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "-h" | "--help" => {
                println!(
                    "Lantern file manager\n\nUsage: lantern [--grid|--list|--miller] [DIRECTORY]"
                );
                return;
            }
            "-V" | "--version" => {
                println!("lantern {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--grid" => state.view_mode = ViewMode::Grid,
            "--list" => state.view_mode = ViewMode::List,
            "--miller" => state.view_mode = ViewMode::Miller,
            path if !path.starts_with('-') => state.navigate(path),
            _ => eprintln!("lantern: unknown option: {argument}"),
        }
    }
    if let Err(e) = lantern_ui::run(state) {
        eprintln!("lantern: {e}");
        std::process::exit(1);
    }
}
