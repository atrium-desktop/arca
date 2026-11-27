---
id: ADR-0023
title: "0023. Chooser Contract v1, Hardened Single-Instance IPC v2, and Strict CLI Semantics"
status: accepted
date: 2026-11-25
---

# 0023. Chooser Contract v1, Hardened Single-Instance IPC v2, and Strict CLI Semantics

- Status: Accepted
- Date: 2026-11-25
- Supersedes in part: [ADR-0002](0002-xdg-desktop-portal-prompter.md)

## Context and Problem Statement

An industrial-grade compatibility and boundary audit across `xdg-desktop-portal-atrium` and `arca` exposed four structural weaknesses in Arca's external interfaces:

1. **Broken Portal Chooser Wire Compatibility**: `xdg-desktop-portal-atrium` retired its multi-prompt v6 envelope in Atrium ADR-0026/0027 and introduced the flat FileChooser-only contract v1 (`{"version":1,"request":...}` / `{"version":1,"response":...}`), while `arca` still spoke the legacy v6 envelope (`{"version":6,"prompt":{"kind":"file_chooser",...},"appearance":...}`). In addition, `arca` rejected empty `Choice.selected == ""` (valid per freedesktop FileChooser spec), validated choice answers without enforcing request order, and read `stdin` without a byte limit.
2. **Prototype-Grade Single-Instance IPC**: `arca-ui::ipc` used an unframed `"OPEN\t<path>\n"` text protocol with `line.trim()`, silently corrupting paths containing trailing spaces, tabs, or newlines; ran a single blocking reader with no timeout or size bound (allowing a single stalled client to deadlock forwarding permanently); omitted `0600` permissions and `SO_PEERCRED` verification; fell back to world-writable `/tmp/arca-ipc-{USER}.sock`; and dropped `XDG_ACTIVATION_TOKEN` so existing Wayland windows could not receive compositor focus.
3. **Broken `OpenURI` Integration**: `arca.desktop` and `org.tessera.Arca.desktop` advertise `Exec=arca %U` and `MimeType=inode/directory;x-scheme-handler/file;`, yet neither the CLI nor `AppState::navigate` decoded `file://` URIs.
4. **Permissive CLI & Lossy Picker Output**: Unknown flags were silently ignored instead of exiting with usage code `2`; `--` end-of-options was unsupported (preventing paths starting with `-`); and standalone pickers had no `-0`/`--null` raw-byte NUL-delimited output mode.

## Decision Outcome

We execute a clean break across Arca's external boundaries with zero legacy shims:

1. **Portal Chooser Contract v1 (`arca-engine::chooser`)**:
   - Adopt `ARCA_CONTRACT_VERSION = 1` with `#[serde(deny_unknown_fields)]` flat envelopes (`ChooserRequestEnvelope` / `ChooserResponseEnvelope`).
   - Retire all v6 types (`PROCESS_CONTRACT_VERSION`, `PrompterRequest`, `PromptRequest`, `PrompterResponse`, `PromptResult`, `PromptAppearance`, `PromptColorScheme`, `PromptAccent`).
   - Check `version` before field decoding so retired v6 or foreign-version peers receive an explicit version-mismatch error.
   - Bound `read_chooser_request` to `MAX_CHOOSER_REQUEST_BYTES = 1 MiB`.
   - Accept `Choice.selected == ""` per freedesktop spec and normalize defaults (`"false"` / first option ID) in `ChooserState::new`; enforce request-ordered choice answers in `validate_choice_answers`.
2. **Hardened Single-Instance IPC v2 (`arca-engine::ipc`)**:
   - Relocate the pure IPC protocol, server, and client into `arca-engine::ipc` (GUI-free, directly unit-testable), leaving `arca-ui::ipc` as a thin wake-callback adapter.
   - Replace the text-line protocol with length-prefixed (`u32_be`, max `64 KiB`) strict JSON frames (`IPC_PROTOCOL_VERSION = 2`) carrying lossless `BytePath` and optional `activation_token` (`XDG_ACTIVATION_TOKEN`).
   - Bind `$XDG_RUNTIME_DIR/arca/ipc.sock` (falling back to `/run/user/<euid>/arca/ipc.sock`, never `/tmp`), enforce `0700` dir / `0600` socket permissions, verify Linux `SO_PEERCRED` same-UID ownership, cap concurrent peer handlers (`16`), and enforce `2s` read/write timeouts per connection.
3. **`file://` URI Decoding & Strict CLI (`arca::cli`, `arca-engine::state`)**:
   - Decode `file://` URIs in CLI target normalization (before single-instance forwarding and window startup) and in `AppState::navigate` (revealing the parent directory when a `file://` URI targets a regular file).
   - Reject unknown CLI options with exit code `2`; support `--` end-of-options; add `-0` / `--null` NUL-delimited native byte output for standalone picker modes (`--choose-file`, `--choose-files`, `--choose-dir`, `--save-file`).

### Deferred Platform-Binding Work (Tracked Explicitly)

Two desktop capabilities depend on upstream bindings beyond this change and are explicitly tracked rather than faked:
- **Wayland `xdg-foreign` parent attachment (`modal` / `parent_window`) & `xdg_activation_v1::activate(token)`**: `arca-ipc-v2` already delivers `XDG_ACTIVATION_TOKEN` to `arca-ui`, and `FileChooserRequest` delivers `modal` / `parent_window`; full compositor wiring requires `optics-window` to expose `xdg_foreign_v2` and token-based activation bindings.
- **`org.freedesktop.FileManager1` D-Bus interface (`ShowFolders` / `ShowItems` / `ShowItemProperties`)**: will bind onto the new `arca-engine::ipc` transport once a D-Bus host crate is introduced in `arca`.

## Consequences

- `arca --chooser-prompt` is 100% wire- and validation-compatible with `xdg-desktop-portal-atrium` ADR-0027.
- Single-instance forwarding preserves arbitrary Unix paths and `XDG_ACTIVATION_TOKEN` without slow-client deadlocks or `/tmp` exposure.
- `OpenURI` launching `arca %U` with `file://` URIs opens directories and reveals files cleanly.
- Standalone picker scripts can safely consume arbitrary Unix filenames via `arca --choose-files -0 | xargs -0 ...`.
