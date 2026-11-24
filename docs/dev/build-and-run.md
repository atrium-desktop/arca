# Build & Run Guide

This document is the authoritative developer guide for building, running, and
verifying Arca across Wayland desktop sessions, portal file pickers, and headless tests.

---

## 1. Prerequisites & Toolchain Setup

Arca requires Rust 1.85+ and the native Optics graphics stack (`optics-app`, `optics-gfx`, `optics-ui`, `optics-window`):

```bash
rustc --version                  # rustc >= 1.85
```

### Choosing the Optics link mode

Optics supports both modes and the choice belongs to the build, not to Optics
(ADR-0014, ADR-0016). Arca's root `Cargo.toml` enables Optics's `static` Cargo
feature by default so canonical and release builds embed `liboptics_*.a`, while
a linked `arca-dev` worktree sets `[env] OPTICS_DYNAMIC = "1"` in
`.cargo/config.toml` for fast shared-library iteration:

| Invocation | Result |
| :--- | :--- |
| `cargo build -p arca` (canonical `main`) | static via workspace `static` feature (`liboptics_*.a`); fails with remediation when the host has no archives |
| `cargo build -p arca` (linked `arca-dev` worktree) | dynamic via `.cargo/config.toml` `[env] OPTICS_DYNAMIC = "1"` (`liboptics_*.so` + relayed `-Wl,-rpath`) |
| `OPTICS_DYNAMIC=0 cargo build -p arca` (inside `arca-dev`) | disables the worktree dynamic override and verifies the production static closure |
| `OPTICS_STATIC=1 cargo build -p arca` | explicitly require the archives |
| `OPTICS_DYNAMIC=1 cargo build -p arca` | override `static` and force the shared objects |

A default Optics Meson tree (`default_library=both`) provides both `.so` and
`.a`. The local development worktree relays the build-tree rpaths so no
`LD_LIBRARY_PATH` is needed:

```bash
meson setup ../optics/build ../optics -Dtests=false -Dbuildtype=debugoptimized
meson compile -C ../optics/build
```

When building alongside a local Optics development tree in this `arca-dev` worktree:

```bash
cp .cargo/optics-local.toml .cargo/config.toml
```

Report which mode a built binary actually used with
`scripts/check-optics-link.sh [binary]`; pass `--expect static` or
`--expect dynamic` to turn it into an assertion.

---

## 2. Compiling the Unified Binary

The workspace compiles into a single unified binary (`arca`):

```bash
# Debug build
cargo build -p arca

# Release build
cargo build --release -p arca
```

A release artifact must embed the Optics modules (`[INV-BUILD-01]`). Because
the workspace manifest enables `features = ["static"]` by default, a canonical
release build embeds them automatically (pass `OPTICS_DYNAMIC=0` if verifying
from inside `arca-dev`):

```bash
cargo build --release -p arca
scripts/check-optics-link.sh --expect static target/release/arca
```

---

## 3. Running Arca

### Cold-Start Sandbox Isolation

To avoid modifying your host configuration (`~/.config/arca`) or personal trash (`~/.local/share/Trash`), run Arca inside an isolated XDG sandbox directory:

```bash
# 1. Establish isolated sandbox directory
export TEST_ROOT=$(mktemp -d /tmp/arca-sandbox.XXXXXX)
export XDG_CONFIG_HOME="$TEST_ROOT/config"
export XDG_DATA_HOME="$TEST_ROOT/data"
export XDG_CACHE_HOME="$TEST_ROOT/cache"
mkdir -p "$XDG_CONFIG_HOME/arca" "$XDG_DATA_HOME" "$XDG_CACHE_HOME"

# 2. Launch interactive file manager
cargo run -p arca -- "$TEST_ROOT"
```

### Standalone File Picker Mode

```bash
cargo run -p arca -- pick-file --title "Choose an Asset" --mime "image/*"
```

### Desktop Portal Prompter Mode

```bash
cargo run -p arca -- portal-prompt --kind open-file
```

---

## 4. Automated Verification & Testing

Arca's automated test suite runs completely headlessly without requiring a live Wayland session or GPU:

```bash
# Run all unit and headless optics-ui frame tests
cargo test --workspace

# Lint & static analysis
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

---

## 5. Runtime Diagnostics

- **Wayland Session**: Arca is a native Wayland client. Ensure `$WAYLAND_DISPLAY` is exported.
- **Optics Stack**: Canonical `main` and release builds embed the Optics modules via the workspace `static` feature, so `ldd target/release/arca` must not list any `liboptics_*`. A linked `arca-dev` build overrides the mode with `OPTICS_DYNAMIC=1` and links the shared modules through the rpath relay, so `ldd target/debug/arca` lists `liboptics_*.so` and the loader finds them in `../optics/build`. If a shared binary cannot find `liboptics_app.so`, `liboptics_gfx.so`, or `liboptics_ui.so`, check whether the rpath relay is active for an uninstalled tree or whether Optics is installed; canonical static builds avoid runtime lookup entirely.
- **Vulkan**: `libvulkan.so.1` and the ICD drivers stay dynamic even in a static Optics build; a missing driver surfaces as a device-creation failure at startup, not as a loader error.
