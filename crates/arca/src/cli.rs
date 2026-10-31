//! Command-line parsing for the `arca` binary.
//!
//! Kept pure and separate from `main` so every flag combination is testable
//! without spawning a process or opening a window. `main` performs the side
//! effects the parsed [`Command`] describes.

use arca_engine::ViewMode;

/// Where the initial directory comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartDir {
    /// No path given: the process working directory.
    WorkingDirectory,
    /// An explicit path (argument order preserved for last-wins).
    Path(String),
}

/// What the process should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Print help and exit 0.
    Help,
    /// Print the version and exit 0.
    Version,
    /// Read a portal request from stdin, prompt, write the response.
    Prompter,
    /// Run a standalone picker.
    Chooser(ChooserInvocation),
    /// Forward `path` to a running instance instead of opening a window.
    Forward { path: String },
    /// Open the interactive file manager window.
    Window {
        view_mode: Option<ViewMode>,
        start: StartDir,
    },
}

/// A standalone file-picker invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChooserInvocation {
    pub mode: arca_engine::chooser::FileChooserMode,
    pub multiple: bool,
    pub title: Option<String>,
    /// Suggested filename (`--save-file --name`).
    pub name: Option<String>,
    pub start_dir: Option<String>,
}

/// The `--help` text.
pub const USAGE: &str = "Arca file manager

Usage:
  arca [--new-window] [--grid|--list|--miller] [DIRECTORY]
  arca --chooser-prompt | arca --prompter
  arca --choose-file [--title TITLE] [DIRECTORY]
  arca --choose-files [--title TITLE] [DIRECTORY]
  arca --choose-dir [--title TITLE] [DIRECTORY]
  arca --save-file [--name NAME] [--title TITLE] [DIRECTORY]

Options:
  -h, --help       Print this help and exit
  -V, --version    Print the version and exit
  --new-window     Open a new window instead of forwarding to a running instance
";

/// Flags that take a value and would otherwise be mistaken for a directory.
const VALUE_FLAGS: [&str; 2] = ["--title", "--name"];

impl Command {
    /// Parse `args` (without `argv[0]`).
    ///
    /// `forward_probe` is asked whether an already-running instance accepts
    /// `path`; injecting it keeps this function pure.
    pub fn parse(args: &[String], forward_probe: impl Fn(&str) -> bool) -> Command {
        // Mode selection is resolved before anything else: a prompter or
        // picker invocation must never fall through to window handling.
        if args
            .iter()
            .any(|a| a == "--chooser-prompt" || a == "--prompter")
        {
            return Command::Prompter;
        }
        if let Some(first) = args.first() {
            if let Some(invocation) = parse_chooser(first, &args[1..]) {
                return Command::Chooser(invocation);
            }
        }

        if args.iter().any(|a| a == "-h" || a == "--help") {
            return Command::Help;
        }
        if args.iter().any(|a| a == "-V" || a == "--version") {
            return Command::Version;
        }

        let mut view_mode = None;
        let mut start = StartDir::WorkingDirectory;
        for argument in args {
            match argument.as_str() {
                // Suppresses the single-instance hand-off.
                "--new-window" => {}
                "--grid" => view_mode = Some(ViewMode::Grid),
                "--list" => view_mode = Some(ViewMode::List),
                "--miller" => view_mode = Some(ViewMode::Miller),
                // A value belonging to a flag is not a directory.
                value if VALUE_FLAGS.contains(&value) => {}
                path if !path.starts_with('-') => start = StartDir::Path(path.to_string()),
                // Unknown options are reported by the caller, not fatal here.
                _ => {}
            }
        }

        let forwards = !args.iter().any(|a| a == "--new-window");
        if forwards {
            if let StartDir::Path(path) = &start {
                if forward_probe(path) {
                    return Command::Forward { path: path.clone() };
                }
            } else if forward_probe(".") {
                return Command::Forward {
                    path: ".".to_string(),
                };
            }
        }

        Command::Window { view_mode, start }
    }

    /// Unknown options, for an `eprintln!` diagnostic. Purely informational:
    /// parsing already ignored them.
    pub fn unknown_options(args: &[String]) -> Vec<&str> {
        args.iter()
            .map(String::as_str)
            .filter(|a| a.starts_with('-'))
            .filter(|a| {
                !matches!(
                    *a,
                    "-h" | "--help"
                        | "-V"
                        | "--version"
                        | "--new-window"
                        | "--grid"
                        | "--list"
                        | "--miller"
                        | "--chooser-prompt"
                        | "--prompter"
                        | "--choose-file"
                        | "--choose-files"
                        | "--choose-dir"
                        | "--save-file"
                        | "--title"
                        | "--name"
                )
            })
            .collect()
    }
}

fn parse_chooser(flag: &str, rest: &[String]) -> Option<ChooserInvocation> {
    use arca_engine::chooser::FileChooserMode;

    let (mode, multiple) = match flag {
        "--choose-file" => (FileChooserMode::OpenFile, false),
        "--choose-files" => (FileChooserMode::OpenFile, true),
        "--choose-dir" => (FileChooserMode::OpenDirectory, false),
        "--save-file" => (FileChooserMode::SaveFile, false),
        _ => return None,
    };

    let mut title = None;
    let mut name = None;
    let mut start_dir = None;
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--title" => title = iter.next().cloned(),
            "--name" => name = iter.next().cloned(),
            dir if !dir.starts_with('-') => start_dir = Some(dir.to_string()),
            // Unknown option for this mode: ignored, like the window path.
            _ => {}
        }
    }

    Some(ChooserInvocation {
        mode,
        multiple,
        title,
        name,
        start_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn no_forward(_: &str) -> bool {
        false
    }

    #[test]
    fn help_and_version_win_over_everything_else() {
        assert_eq!(Command::parse(&args(&["-h"]), no_forward), Command::Help);
        assert_eq!(
            Command::parse(&args(&["--help", "/tmp"]), no_forward),
            Command::Help
        );
        assert_eq!(Command::parse(&args(&["-V"]), no_forward), Command::Version);
        assert_eq!(
            Command::parse(&args(&["--miller", "--version"]), no_forward),
            Command::Version
        );
    }

    #[test]
    fn prompter_mode_is_detected_under_both_spellings() {
        for flag in ["--chooser-prompt", "--prompter"] {
            assert_eq!(
                Command::parse(&args(&[flag]), no_forward),
                Command::Prompter
            );
            // Even alongside other arguments.
            assert_eq!(
                Command::parse(&args(&["/tmp", flag]), no_forward),
                Command::Prompter
            );
        }
    }

    #[test]
    fn every_picker_mode_maps_to_its_chooser_mode() {
        use arca_engine::chooser::FileChooserMode;

        let cases = [
            ("--choose-file", FileChooserMode::OpenFile, false),
            ("--choose-files", FileChooserMode::OpenFile, true),
            ("--choose-dir", FileChooserMode::OpenDirectory, false),
            ("--save-file", FileChooserMode::SaveFile, false),
        ];
        for (flag, mode, multiple) in cases {
            match Command::parse(&args(&[flag]), no_forward) {
                Command::Chooser(invocation) => {
                    assert_eq!(invocation.mode, mode, "{flag}");
                    assert_eq!(invocation.multiple, multiple, "{flag}");
                }
                other => panic!("{flag} produced {other:?}"),
            }
        }
    }

    #[test]
    fn picker_options_are_parsed_and_not_mistaken_for_directories() {
        let parsed = Command::parse(
            &args(&["--choose-file", "--title", "Pick me", "/tmp/docs"]),
            no_forward,
        );
        match parsed {
            Command::Chooser(invocation) => {
                assert_eq!(invocation.title.as_deref(), Some("Pick me"));
                assert_eq!(invocation.start_dir.as_deref(), Some("/tmp/docs"));
                assert_eq!(invocation.name, None);
            }
            other => panic!("expected a chooser, got {other:?}"),
        }

        let save = Command::parse(
            &args(&["--save-file", "--name", "report.pdf", "--title", "Save"]),
            no_forward,
        );
        match save {
            Command::Chooser(invocation) => {
                assert_eq!(invocation.name.as_deref(), Some("report.pdf"));
                assert_eq!(invocation.title.as_deref(), Some("Save"));
                assert_eq!(invocation.start_dir, None, "--name is a value, not a dir");
            }
            other => panic!("expected a chooser, got {other:?}"),
        }
    }

    #[test]
    fn view_mode_flags_select_the_mode() {
        for (flag, mode) in [
            ("--grid", ViewMode::Grid),
            ("--list", ViewMode::List),
            ("--miller", ViewMode::Miller),
        ] {
            match Command::parse(&args(&[flag]), no_forward) {
                Command::Window { view_mode, .. } => assert_eq!(view_mode, Some(mode), "{flag}"),
                other => panic!("{flag} produced {other:?}"),
            }
        }
        // The last mode flag wins.
        match Command::parse(&args(&["--grid", "--miller"]), no_forward) {
            Command::Window { view_mode, .. } => assert_eq!(view_mode, Some(ViewMode::Miller)),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_positional_argument_is_the_start_directory() {
        match Command::parse(&args(&["--miller", "/tmp/docs"]), no_forward) {
            Command::Window { view_mode, start } => {
                assert_eq!(view_mode, Some(ViewMode::Miller));
                assert_eq!(start, StartDir::Path("/tmp/docs".into()));
            }
            other => panic!("got {other:?}"),
        }
        match Command::parse(&args(&[]), no_forward) {
            Command::Window { start, .. } => assert_eq!(start, StartDir::WorkingDirectory),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_running_instance_takes_the_path_when_forwarding_is_allowed() {
        let accepted = |path: &str| path == "/tmp/docs";
        assert_eq!(
            Command::parse(&args(&["/tmp/docs"]), accepted),
            Command::Forward {
                path: "/tmp/docs".into()
            }
        );
        // With no path, the working directory is forwarded.
        assert_eq!(
            Command::parse(&args(&["--grid"]), |p| p == "."),
            Command::Forward { path: ".".into() }
        );
    }

    #[test]
    fn new_window_suppresses_forwarding() {
        let always = |_: &str| true;
        match Command::parse(&args(&["--new-window", "/tmp/docs"]), always) {
            Command::Window { start, .. } => {
                assert_eq!(start, StartDir::Path("/tmp/docs".into()))
            }
            other => panic!("--new-window must open a window, got {other:?}"),
        }
    }

    #[test]
    fn help_and_version_never_forward() {
        // A probe that accepts everything must not swallow --help.
        let always = |_: &str| true;
        assert_eq!(Command::parse(&args(&["--help"]), always), Command::Help);
        assert_eq!(Command::parse(&args(&["-V"]), always), Command::Version);
    }

    /// `--help` is the only place most users meet the CLI, and it drifted
    /// from `docs/reference/cli.md` before. Every flag the parser accepts
    /// must be named in the usage text.
    #[test]
    fn usage_text_names_every_accepted_flag() {
        for flag in [
            "--new-window",
            "--grid",
            "--list",
            "--miller",
            "--chooser-prompt",
            "--choose-file",
            "--choose-files",
            "--choose-dir",
            "--save-file",
            "--title",
            "--name",
            "-h",
            "-V",
        ] {
            assert!(USAGE.contains(flag), "usage text omits {flag}");
        }
    }

    /// Both spellings of a "stop and print" flag must win over the directory
    /// argument, and neither may be treated as a path even when it appears
    /// after one.
    #[test]
    fn help_and_version_win_over_a_directory_argument() {
        assert_eq!(
            Command::parse(&args(&["/tmp", "--help"]), no_forward),
            Command::Help
        );
        assert_eq!(
            Command::parse(&args(&["/tmp", "-V"]), no_forward),
            Command::Version
        );
        // A picker flag in a later position is not a mode selector: only the
        // first argument may start a chooser invocation.
        match Command::parse(&args(&["--grid", "--choose-file"]), no_forward) {
            Command::Window { .. } => {}
            other => panic!("a trailing picker flag must not switch mode: {other:?}"),
        }
    }

    /// A flag's value must never be captured as the start directory, in any
    /// order, and the last real positional argument wins.
    #[test]
    fn flag_values_are_never_mistaken_for_directories() {
        let parsed = Command::parse(
            &args(&["--save-file", "--title", "/not/a/dir", "/real"]),
            no_forward,
        );
        match parsed {
            Command::Chooser(invocation) => {
                assert_eq!(invocation.title.as_deref(), Some("/not/a/dir"));
                assert_eq!(invocation.start_dir.as_deref(), Some("/real"));
            }
            other => panic!("got {other:?}"),
        }

        match Command::parse(&args(&["/first", "/second"]), no_forward) {
            Command::Window { start, .. } => {
                assert_eq!(start, StartDir::Path("/second".into()), "last wins")
            }
            other => panic!("got {other:?}"),
        }
    }

    /// The forwarded path must be the one the window would have opened.
    #[test]
    fn the_forwarded_path_matches_the_window_path() {
        let seen = std::cell::RefCell::new(Vec::new());
        let probe = |path: &str| {
            seen.borrow_mut().push(path.to_string());
            false
        };
        Command::parse(&args(&["--miller", "/tmp/docs"]), probe);
        assert_eq!(*seen.borrow(), vec!["/tmp/docs".to_string()]);
    }

    #[test]
    fn unknown_options_are_collected_for_diagnostics() {
        let list = args(&["--grid", "--bogus", "/tmp", "-x"]);
        assert_eq!(Command::unknown_options(&list), vec!["--bogus", "-x"]);
        assert!(Command::unknown_options(&args(&["--grid", "/tmp"])).is_empty());
    }
}
