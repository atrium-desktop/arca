# Automated Verification & Testing Guide

This document details the automated testing strategy, execution commands, and regression prevention practices for developers working on Arca.

---

## 1. The Test Pyramid

Arca organizes automated verification across four structured tiers to guarantee fast regression feedback without requiring a live Wayland session or physical display GPU:

```text
                  ▲
                 / \
                /   \     Tier 4: End-to-End System Tests
               / E2E \    (CLI execution, Portal stdio JSON roundtrips)
              /───────\
             /  Integ  \  Tier 3: Subsystem Integration Tests
            /  -ration  \ (Multi-tab state machines, file operations undo)
           /─────────────\
          /   Headless    \ Tier 2: Headless Lens Frame Tests
         /   Frame Tests   \ (Immediate-mode rendering, 1 MiB arena budget)
        /───────────────────\
       /        Unit         \ Tier 1: Fast Unit Tests
      /      Assertions       \ (XDG protocols, sorting, trash, config)
     /─────────────────────────\
```

- **Tier 1: Fast Unit Tests (`arca-xdg`, `arca-engine`)**: In-memory and temp-directory tests covering XDG base/user directories, MIME sniffing and glob matching, RFC 8089 file URIs, FreeDesktop trash v1.0 specifications, sorting/filtering, and config serialization. Runs in milliseconds.
- **Tier 2: Headless Frame Tests (`arca-ui`)**: Driven via `lens::Ui::headless()`, verifying frame construction, layout generation, input event handling, widget focus, popup interactions, and virtualized viewport arena memory safety.
- **Tier 3: Subsystem Integration Tests**: Validates interactions between `arca-engine` state machines and `arca-ui` view components (e.g., location bar tab completion, batch trash undo, dynamic tab reordering).
- **Tier 4: End-to-End System Tests (`arca`)**: CLI flag validation, standalone file picking, and XDG desktop portal prompter JSON protocol execution over stdio.

---

## 2. Testing Principles

1. **Deterministic Execution**:
   - Tests must NOT depend on a running Wayland compositor, X11 display, or physical Vulkan GPU.
   - All presentation tests are driven headlessly via `lens::Ui::headless()`.
2. **Invariant Verification via Tests**:
   - Whenever an ADR defines a true system invariant that cannot be enforced purely by the type system at compile time, a dedicated automated test asserting the invariant must exist in the appropriate test suite.
   - Example: `tests::large_directory_does_not_overflow_frame_arena` mechanically guards the 1 MiB Lens arena budget (`[INV-UI-01]`).
3. **Purity and Leak-Free Fixtures**:
   - Tests creating temporary directories or files must use RAII fixtures (e.g., `std::env::temp_dir()` with unique prefixes or cleanup guards) that clean up on drop, preventing disk pollution.
4. **Fast Feedback**:
   - Unit tests in `arca-xdg` and `arca-engine` execute in sub-second time to support rapid developer inner loops.

---

## 3. Run Commands

### Executing Tests

```bash
# Run all workspace tests (XDG + engine + UI headless + binary)
cargo test --workspace

# Run all tests in arca-xdg only (pure std specs)
cargo test -p arca-xdg

# Run all tests in arca-engine only (headless domain engine)
cargo test -p arca-engine

# Run all tests in arca-ui only (headless lens frames)
cargo test -p arca-ui

# Run a specific test by name filter
cargo test large_directory_does_not_overflow_frame_arena

# Run tests with uncaptured stdout output
cargo test -- --nocapture
```

### Static Analysis and Linters

```bash
# Check formatting across all crates
cargo fmt --check

# Run clippy with workspace rules
cargo clippy --workspace --all-targets
```

---

## 4. Key Invariants Guarded by Tests

1. **Lens Frame Arena Budget (`[INV-UI-01]`)**:
   - `tests::large_directory_does_not_overflow_frame_arena` verifies that directories with thousands of items do not exceed the 1 MiB immediate-mode arena limit.
   - Virtualized row calculation ensures only visible rows allocate draw calls.

2. **Anti-Clobber in File Operations (`[INV-OPS-02]`)**:
   - `ops::tests::rename_refuses_to_clobber` ensures renaming never silently overwrites an existing destination.
   - `ops::tests::create_folder_picks_unique_names` checks automatic deduplication ("New Folder", "New Folder (1)").

3. **FreeDesktop Trash Specification (`[INV-OPS-01]`)**:
   - `trash::tests::test_trash_roundtrip` and `trash::tests::test_trashinfo_roundtrip` verify that `.trashinfo` metadata stores valid RFC 3339 timestamps and percent-encoded original paths.

4. **Portal Prompter Wire Contract**:
   - `chooser::tests::wire_roundtrip` and `chooser::tests::glob_matching` verify compatibility with the Tessera prompter contract v6 and FileChooser v3 filters.

5. **XDG Specification Purity (`[INV-ARCH-01]`)**:
   - Tests in `arca-xdg` verify MIME database matching, desktop file exec code expansions (`%f`, `%u`), and RFC 1321 MD5 hashes without pulling any third-party dependencies.

---

## 5. Failure Triage Matrix

| Symptom | Likely Cause | First Inspection |
| :--- | :--- | :--- |
| `missing flux/lens/iris pkg-config` | Optics C libraries not found | Ensure `pkg-config --modversion iris lens flux` succeeds or check meson build tree |
| `large_directory_does_not_overflow_frame_arena` fails | Unbounded loop added to row builder | Verify viewport height and row range slicing in `crates/arca-ui/src/views/` |
| `flac_with_picture_decodes` fails | Corrupted test fixture or image decoder feature disabled | Check `image` crate features in `crates/arca-engine/Cargo.toml` |
| `chooser_save_overwrite_modal_triggers` fails | Modal state flag not cleared or rendered | Check `chooser.rs` prompt state handling in `arca-engine` |
| `test_trash_roundtrip` fails | Permissions issue or invalid timestamp format | Inspect temporary path permissions and RFC 3339 formatting |

---

## 6. Adding New Tests

1. **Pure Specifications (`arca-xdg`)**: Add unit tests in `crates/arca-xdg/src/` inside `#[cfg(test)] mod tests`. Must remain std-only.
2. **Domain Business Logic (`arca-engine`)**: Add unit tests in the corresponding module under `crates/arca-engine/src/`. Use scratch directories via `std::env::temp_dir()` and ensure tests clean up after themselves.
3. **UI Interaction & Layout (`arca-ui`)**: Add headless tests in `crates/arca-ui/src/app.rs` or submodules. Construct state via `AppState::new()`, invoke frames via `lens::Ui::headless()`, and assert state mutations or widget response IDs.
4. Assert both nominal outcomes and boundary edge cases (empty paths, duplicate names, missing metadata, unreadable files).
