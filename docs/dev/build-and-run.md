# Build & Run Guide

This document is the authoritative developer guide for building, running, and
verifying Arca across Wayland desktop sessions, portal file pickers, and headless tests.

---

## 1. Prerequisites & Toolchain Setup

Arca requires Rust 1.85+ and the native Optics graphics stack (`optics-app`, `optics-gfx`, `optics-ui`, `optics-window`):

```bash
rustc --version                  # rustc >= 1.85
```

When building alongside a local Optics development tree in this `arca-dev` worktree:

```bash
cp .cargo/optics-local.toml .cargo/config.toml
```

---

## 2. Compiling the Unified Binary

The workspace compiles into a single unified binary (`arca`):

```bash
# Debug build
cargo build -p arca

# Release build
cargo build --release -p arca
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
- **Optics Libraries**: If the dynamic linker fails to find `liboptics_app.so`, `liboptics_gfx.so`, or `liboptics_ui.so`, ensure `LD_LIBRARY_PATH` includes the build tree or Optics is installed.
