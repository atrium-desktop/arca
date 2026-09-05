# 0002. XDG Desktop Portal FileChooser Prompter Protocol

- Status: Accepted
- Date: 2026-09-02
- Deciders: Ming Li
- Consulted: Tessera desktop engineering team
- Informed: Arca contributors

## Context and Problem Statement

Wayland environments require sandboxed Flatpak and native applications to access the host filesystem through the `org.freedesktop.portal.FileChooser` portal interface. Desktop environments such as Tessera delegate user selection to a standalone prompter process rather than embedding complex file dialog UI inside the portal daemon. Arca needed to function as both a day-to-day interactive file manager and an asynchronous, secure portal file chooser prompter.

## Decision Drivers

- Driver 1: Conformance to the freedesktop `org.freedesktop.impl.portal.FileChooser` specification (version 3).
- Driver 2: Compatibility with the Tessera desktop portal prompter protocol contract (contract v6 over standard I/O).
- Driver 3: Support for standalone CLI file selection (`--choose-file`, `--choose-files`, `--choose-dir`, `--save-file`).
- Driver 4: Crash resilience and process boundary isolation between the portal daemon and the UI prompter.

## Considered Options

- Option 1: Implement an in-process D-Bus daemon inside Arca listening on `org.freedesktop.impl.portal.desktop.arca`.
- Option 2: Run a separate helper binary dedicated exclusively to portal prompting.
- Option 3: Integrate prompter execution directly into the primary `arca` binary via `--chooser-prompt` exchanging JSON over anonymous stdin/stdout pipes, alongside standalone CLI picking flags.

## Decision Outcome

Chosen option: "Option 3", because using the unified `arca` binary with `--chooser-prompt` avoids code duplication, reuses `arca-ui` components and `arca-core` file abstractions, and matches the Tessera prompter isolation model.

The prompter protocol parses a JSON request on standard input containing:
- Dialog mode: `open_file`, `open_directory`, or `save_file`
- Configuration: multiple selection flag, modal parent handle, title, default directory, current file name
- Filter sets: typed glob and MIME-type filters
- Custom portal choices: combo boxes and checkboxes requested by caller applications

Upon user confirmation or cancellation, the process writes a structured JSON response to standard output containing the exit outcome and the selected Unix byte paths.

### Positive Consequences

- Seamless integration with the Tessera desktop portal daemon without lingering background daemons.
- Single binary footprint simplifies packaging and deployment across Linux distributions.
- Enables command-line utilities and shell scripts to invoke native Wayland file dialogs via CLI flags (`--choose-file`, `--save-file`).

### Negative Consequences

- The application must support short-lived, transient window lifecycles and immediate exit upon response serialization.
- Mitigation: Dedicated exit paths in `run_prompter_mode()` ensuring stdout is flushed and buffers closed cleanly before process termination.

## Links

- [Portal Protocol Reference](../reference/portal-protocol.md)
- [Portal Chooser How-To Guide](../how-to/use-portal-file-picker.md)
