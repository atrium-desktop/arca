# Automated Verification & Testing Guide

This document describes Arca's layered test architecture, the commands for each
lane, and the regression invariants the suites guard. No automated test requires
a live Wayland session or a physical display GPU: presentation coverage runs
through `optics_ui::Ui::headless()`.

---

## 1. The Test Pyramid

```text
                  ▲
                 / \
                /   \      Tier 4: End-to-end (spawned binary)
               / E2E \     CLI surface, portal JSON stdio, single-instance IPC
              /───────\
             /  Integ  \   Tier 3: Workspace integration (`arca-tests`)
            /  -ration  \  Architecture firewalls, engine+XDG flows, headless
           /─────────────\ user journeys, damage/virtualization, click stress
          /   Headless    \ Tier 2: Headless UI unit frames (in `arca-ui`)
         /   Frame Tests   \ Widgets, layouts, input routing, arena budget
        /───────────────────\
       /        Unit         \ Tier 1: Fast unit tests (beside each module)
      /      Assertions       \ XDG specs, sorting, trash, config, protocols
     /─────────────────────────\
```

- **Tier 1 — crate-local unit tests.** Live in `#[cfg(test)] mod tests` next to
  the module they constrain in `arca-xdg`, `arca-engine`, `arca-ui`, and
  `arca/src/cli.rs`. They must stay fast and deterministic.
- **Tier 2 — headless UI frames.** Also crate-local (`arca-ui`), driven through
  `optics_ui::Ui::headless()`. They assert widget responses, focus, layout, and
  the 1 MiB frame-arena invariant.
- **Tier 3 — workspace integration (`tests/integration/`).** Crosses crate and
  protocol boundaries: the architecture firewalls, real scratch-directory
  navigation and file operations, portal contract encode/decode, the
  single-instance socket, complete headless user journeys, regional-damage
  quiescence, virtualization, and repetitive-interaction stress.
- **Tier 4 — end-to-end (`tests/e2e/`).** Spawns the production `arca` binary
  and speaks its real entry surfaces: argv/stdout/stderr, prompter JSON over
  stdio, and forwarding through the Unix socket.

The external suites are one package, `arca-tests`, with two test binaries
(`integration` and `e2e`) so Cargo links each suite once. Shared fixtures live
in `crates/test-support` (std-only) and `tests/common/`. See
[`tests/README.md`](../../tests/README.md).

---

## 2. Testing Principles

1. **Deterministic execution.** No compositor, X11 display, or physical GPU.
   Presentation tests build real frames headlessly.
2. **Mechanically guarded invariants.** A rule that cannot be enforced by the
   type system gets a named test (see §6). Architecture firewalls are Rust tests
   in `tests/integration/architecture.rs`, so `cargo test` and CI fail together.
3. **Leak-free fixtures.** `test_support::fs::TestDir` creates a unique
   scratch tree under the system temp directory and removes it on drop,
   including on panic. `AppState::with_config_and_trash_root` keeps destructive
   tests inside a scratch Trash, and `UiApp::with_thumb_cache` keeps thumbnail
   pruning out of the user's real cache. `test_support::env::XdgIsolation`
   remains available for tests that must exercise environment-derived paths.
4. **No fixed sleeps for asynchronous effects.** Use
   `test_support::poll::wait_until`/`wait_for`; every wait is bounded.
5. **Never build from a test.** A nested `cargo build` blocks on the runner's
   target-directory lock and looks like a product hang. The end-to-end suite
   resolves an already-built binary (`test_support::workspace::binary`);
   `nextest` builds workspace binaries once through its setup script.
6. **Layer-appropriate placement.** A test belongs beside the module if it only
   needs that module; at the root if it proves a boundary, a whole user
   journey, or a process contract.
7. **Prefer nextest when touching process-global state.** `XdgIsolation` still
   mutates `XDG_*`/`HOME`, which is process-global. Nextest's process-per-test
   model isolates that cleanly; plain `cargo test` runs sibling tests as
   threads. Keep env-derived assertions inside the isolated test, and run
   `cargo nextest run` when adding another test that depends on XDG/HOME.

---

## 3. Run Commands

```bash
# Everything, canonical runner (process-per-test, timeouts, binary setup)
cargo nextest run --locked --workspace

# Same graph through Cargo (fast local loop; threads share the process)
cargo test --locked --workspace

# Fast, native-free engine/XDG/protocol/architecture lane
cargo nextest run --locked -p arca-tests --no-default-features

# One external suite, with per-test processes and the binary setup script
cargo nextest run --locked -p arca-tests

# One crate's unit tests
cargo test -p arca-xdg
cargo test -p arca-engine
cargo test -p arca-ui

# Focused selections
cargo test -p arca-tests architecture::
cargo test -p arca-tests --test e2e
cargo test -p arca-ui --lib
cargo test large_directory_does_not_overflow_frame_arena_in_any_view
```

Nextest uses [`.config/nextest.toml`](../../.config/nextest.toml): a 60 s
slow-test timeout (so a deadlock fails instead of hanging), a setup script that
builds `--workspace --bins` once before the end-to-end tests, and single-thread
groups for the IPC endpoint and trash roundtrip. CI uses the nextest profile,
so tests that touch process-global state are isolated by construction.

### Static analysis and lint gates

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
python3 scripts/check-docs.py tests   # test locality
```

### Link-mode verification

Arca's link mode is a build input (ADR-0014, ADR-0016): the workspace manifest
enables Optics's `static` feature by default so canonical and release builds
embed `liboptics_*.a`, while `OPTICS_DYNAMIC=1` (configured in
`arca-dev/.cargo/config.toml`) overrides to `liboptics_*.so` for fast local
iteration. The script reports and asserts the outcome:

```bash
cargo build --release -p arca
scripts/check-optics-link.sh target/release/arca                  # report
scripts/check-optics-link.sh --expect static target/release/arca  # release gate
scripts/check-optics-link.sh --expect dynamic target/release/arca # shared gate
scripts/test-check-optics-link.sh                                  # gate's own tests
```

Run the gate before `strip`. `strip --strip-debug` keeps the symbol table it
reads; a fully stripped artifact fails `--expect static` unless
`--allow-unverified-stripped` is passed deliberately.

---

## 4. Suite Map

| Path | Layer | What it proves |
| :--- | :--- | :--- |
| `crates/arca-xdg/src/**` | unit | Base/User Dirs, MIME + glob, file URIs, trash v1.0, `.desktop`, MD5 |
| `crates/arca-engine/src/**` | unit | Sorting, history, state machine, ops, config, preview, thumbnails, worker, watcher |
| `crates/arca-ui/src/**` | unit + headless | Widgets, input routing, every view, inline rename, menus, preview, tabs |
| `crates/arca/src/cli.rs` | unit | Every argv combination, including forwarding decisions |
| `tests/integration/architecture.rs` | integration | Zero-dependency XDG and test support, headless engine, concentric dependencies, one binary, test-only crates |
| `tests/integration/navigation.rs` | integration | Tabs, per-tab history/filter, hidden files, sorting, completion, bookmarks |
| `crates/arca-engine/src/state.rs` | unit | Synchronous and asynchronous navigation, last-writer-wins, deferred history cursor, cancel-on-sync, no UI-thread reads |
| `crates/arca-engine/src/telemetry/**` | unit | Log filters, line format, rotation, metrics/quantiles, spans, alert dedupe, watchdog rules (stall, storm, error burst), diagnostics snapshots |
| `tests/integration/file_operations.rs` | integration | Anti-clobber rename, unique folders, copy/paste, drop payloads, trash + undo/purge |
| `tests/integration/chooser_contract.rs` | integration | Prompter contract v6 request/response encoding, validation, filters |
| `tests/integration/ipc.rs` | integration | Single-instance handshake, queueing, stale endpoints, concurrent clients |
| `tests/integration/ui_flows.rs` | integration | End-to-end headless journeys: views, keys, double-click, context menu, settings |
| `tests/integration/damage_and_quiescence.rs` | integration | Regional damage, departure damage, caret phase edges, animation termination |
| `tests/integration/virtualization.rs` | integration | 2 500-entry directories and scrolling stay inside the frame arena |
| `tests/integration/interaction_stress.rs` | integration | Hundreds of clicks / view changes stay bounded and quiescent |
| `tests/integration/observability.rs` | integration | Engine operations feed the diagnostics report; transfers are measured; a stalled UI thread is alerted with its span; `telemetry::init` routes logs into the file |
| `tests/integration/observability_ui.rs` | integration | Headless UI journeys keep directory reads off the UI thread; frames beat the heartbeat |
| `tests/e2e/cli.rs` | e2e | `--help`/`--version`, documented flags, unknown-option reporting |
| `tests/e2e/portal_prompter.rs` | e2e | One JSON response line, malformed/foreign requests fail closed |
| `tests/e2e/ipc_forward.rs` | e2e | Real binary forwards a relative path to a running instance and exits |
| `tests/e2e/diagnostics.rs` | e2e | `--diagnostics` rendering and exit codes, log file creation, stdout stays protocol-clean, `ARCA_LOG_STDERR` |

---

## 5. Failure Triage Matrix

| Symptom | Likely cause | First inspection |
| :--- | :--- | :--- |
| `missing optics-* pkg-config` | Optics C libraries not found | `pkg-config --modversion optics-ui`; see `docs/dev/setup.md` |
| `Optics was asked to link ... statically, but no liboptics_*.a was found` | Static build on a shared-only tree | Build Optics with `-Ddefault_library=both`, or pass `OPTICS_DYNAMIC=1` |
| `Optics was asked to link ... dynamically, but no liboptics_*.so was found` | Shared request on an archive-only tree | Build both, or drop `OPTICS_DYNAMIC` |
| `... request opposite Optics link modes` | A static and a dynamic switch are both set | Unset one |
| `FAIL: ... is stripped` | Link gate ran after a full `strip` | Run before stripping or use `--strip-debug` |
| `workspace binary 'arca' is missing` | E2E suite ran without building the binary | `cargo build --workspace --bins`, or use `cargo nextest run` |
| `large_directory_does_not_overflow_frame_arena_in_any_view` fails | Unbounded loop in a view builder | `crates/arca-ui/src/content.rs` viewport slicing |
| `hundreds_of_row_clicks_are_bounded...` fails | Per-click stall or unbounded allocation | `docs/dev/interaction-latency.md` |
| `trash_roundtrip_through_engine_and_undo` fails | Scratch Trash permissions or a record mismatch | Inspect the injected `trash-root` directory and its `.trashinfo` record |
| `builds_frames_headless` fails | Optics shared libraries not loadable | Run via the workspace harness (rpath relay) or `OPTICS_DYNAMIC=0` |
| `..._keeps_blocking_reads_off_the_ui_thread` fails | A UI-path change calls `entry::read_dir` synchronously | The span named in the failing frame; `docs/dev/interaction-latency.md` |
| `a_stalled_ui_thread_is_alerted...` fails | Watchdog not sampling, or span not published on the heartbeat | `crates/arca-engine/src/telemetry/watchdog.rs` |

---

## 6. Invariants and Their Enforcing Tests

1. **`[INV-ARCH-01]` / `[INV-ARCH-02]` / `[INV-ARCH-03]` — architecture
   firewalls.** `tests/integration/architecture.rs` asserts `arca-xdg` has zero
   dependencies, `arca-engine` never links the graphics stack, and dependencies
   flow inward only. The same rules remain duplicated in the CI `architecture`
   job for defence in depth.
2. **`[INV-UI-01]` — 1 MiB frame arena.**
   `virtualization::large_directory_does_not_overflow_frame_arena_in_any_view`
   (2 500 entries × Grid/List/Miller) and
   `virtualization::scrolled_large_listing_builds_only_visible_rows`.
3. **`[INV-OPS-01]` — FreeDesktop trash.**
   `arca-xdg` unit tests plus
   `file_operations::trash_roundtrip_through_engine_and_undo` and
   `file_operations::purge_permanently_removes_a_trashed_record`.
4. **`[INV-OPS-02]` — anti-clobber.** `arca-engine` unit tests plus
   `file_operations::rename_refuses_to_clobber_then_succeeds_to_a_free_name`.
5. **`[INV-UI-04]` — interaction latency.** A click must not request a
   synchronous XDG activation token unless it launches an application, and
   interactive navigation must not read the directory on the frame thread.
   `app::tests::double_click_into_directory_does_not_panic_on_stale_rows`
   asserts the token policy,
   `app::tests::double_click_does_not_cross_a_navigation_to_a_different_entry`
   pins path-based double-click identity, `state::tests::async_navigation*`
   covers last-writer-wins and history, `ui_flows::asynchronous_navigation_*`
   exercises the production policy, and `interaction_stress::*` bounds
   repetitive input. See [Interaction Latency](interaction-latency.md).
6. **Portal wire contract.** `chooser_contract::*` plus
   `e2e::portal_prompter::*` cover version refusal, field validation, and the
   one-line response framing.
7. **`[INV-OBS-01]` — protocol-clean logging.**
   `e2e::diagnostics::prompter_logs_to_the_state_dir_and_keeps_stdout_protocol_clean`
   runs the prompter at `ARCA_LOG=trace` and requires exactly one JSON line on
   stdout while the log file receives the records.
8. **`[INV-OBS-02]` — freeze evidence.** `watchdog::tests::*` drive the pure
   `Monitor` with synthetic samples (stall, recovery, back-to-back stalls,
   storm re-arming, vsync rates, idle windows), and
   `observability::a_stalled_ui_thread_is_alerted_with_the_blocking_operation`
   proves the live thread names the blocking span.
9. **`[INV-OPS-05]` / `[INV-NAV-01]` — reaped launches, deferred history.**
   `desktop::tests::detached_children_are_reaped`,
   `state::tests::a_failed_async_back_leaves_the_history_cursor_in_place`,
   `state::tests::rapid_async_back_clicks_walk_history_and_publish_once`.

Telemetry tests share process-wide registries under plain `cargo test`.
Assert on deltas or unique metric names, keep heartbeat assertions on the
test's own thread (heartbeats are per thread), and never assert on the exact
text of the newest global alert.

---

## 7. Adding Tests

1. **Pure specification (`arca-xdg`):** add a `#[cfg(test)] mod tests` block in
   the module. Stay std-only.
2. **Domain logic (`arca-engine`):** add tests beside the module. Use
   `test-support` only from external suites; crate-local tests may keep a
   small private helper.
3. **UI interaction (`arca-ui`):** add headless tests beside the module when
   they need private state; add `tests/integration/ui_flows.rs` when the journey
   goes through the public `UiApp` surface.
4. **Cross-crate or process boundary:** add a module under
   `tests/integration/` (crate-level composition) or `tests/e2e/` (spawns the
   binary). Register it in the corresponding `main.rs`.
5. **New architectural rule with high blast radius:** record it in an ADR,
   add a named assertion to `tests/integration/architecture.rs` or the relevant
   suite, and update §6 here.
6. Assert both nominal and boundary outcomes (empty paths, duplicate names,
   missing metadata, unreadable files, stale sockets).
