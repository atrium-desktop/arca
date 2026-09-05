# Testing

This document details how to execute, interpret, and extend the automated test
suites for Arca.

---

## Scope & Test Model

The Arca test suite is organized into three tiers to guarantee fast regression feedback
without requiring a live Wayland session or physical display GPU:

- **Core Unit Tests (`arca-core`)**: In-memory and temp-directory tests covering
  filesystem operations, path resolution, trash specification compliance, thumbnail
  LRU caching, config serialization, and portal request/response contracts.
- **Headless Frame Tests (`arca-ui`)**: Driven via `lens::Ui::headless()`, verifying
  frame construction, layout generation, input event handling, widget focus, popup
  interactions, and virtualized viewport arena memory safety.
- **Static Analysis & Formatting**: Clippy and rustfmt checks enforcing memory safety,
  idiomatic Rust, and workspace conventions.

---

## Run Commands

### Executing Tests

```bash
# Run all workspace tests (core + UI headless)
cargo test

# Run all tests in arca-core only
cargo test -p arca-core

# Run all tests in arca-ui only (headless lens frames)
cargo test -p arca-ui

# Run a specific test by name filter
cargo test large_directory_does_not_overflow_frame_arena

# Run tests with uncaptured stdout output
cargo test -- --nocapture
```

### Static Analysis and Linters

```bash
# Check formatting
cargo fmt --check

# Run clippy with workspace rules
cargo clippy --workspace --all-targets
```

---

## Key Invariants Guarded by Tests

1. **Lens Frame Arena Limit**:
   - `tests::large_directory_does_not_overflow_frame_arena` verifies that directories
     with thousands of items do not exceed the 1 MiB immediate-mode arena limit.
   - Virtualized row calculation ensures only visible rows allocate draw calls.

2. **Zero Clashing in File Operations**:
   - `ops::tests::rename_refuses_to_clobber` ensures renaming never silently
     overwrites an existing destination.
   - `ops::tests::create_folder_picks_unique_names` checks automatic deduplication
     ("New Folder", "New Folder (1)").

3. **FreeDesktop Trash Specification**:
   - `trash::tests::test_trashinfo_roundtrip` verifies that `.trashinfo` metadata
     stores valid RFC 3339 timestamps and percent-encoded original paths.

4. **Portal Prompter Serialization**:
   - `chooser::tests::wire_roundtrip` and `chooser::tests::glob_matching` verify
     compatibility with the Tessera prompter contract v6 and FileChooser v3 filters.

---

## Failure Triage Matrix

| Symptom | Likely Cause | First Inspection |
|---------|--------------|------------------|
| `missing flux/lens/iris pkg-config` | Optics C libraries not found | Ensure `pkg-config --modversion iris lens flux` succeeds or check meson build tree |
| `large_directory_does_not_overflow_frame_arena` fails | Unbounded loop added to row builder | Verify viewport height and row range slicing in `crates/arca-ui/src/views/` |
| `flac_with_picture_decodes` fails | Corrupted test fixture or image decoder feature disabled | Check `image` crate features in `crates/arca-core/Cargo.toml` |
| `chooser_save_overwrite_modal_triggers` fails | Modal state flag not cleared or rendered | Check `chooser.rs` prompt state handling |

---

## Adding New Tests

1. **Business Logic**: Add unit tests in the corresponding module under
   `crates/arca-core/src/` inside `#[cfg(test)] mod tests`. Use scratch directories via
   `std::env::temp_dir()` and ensure tests clean up after themselves.
2. **UI Interaction**: Add headless tests in `crates/arca-ui/src/app.rs` or submodules.
   Construct state via `AppState::new()`, invoke frames via `lens::Ui::headless()`,
   and assert state mutations or response IDs.
3. Assert both nominal outcomes and edge cases (empty paths, duplicate names, missing
   metadata).
