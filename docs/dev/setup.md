# Setup

How to build, test, and run Arca for development.

## Prerequisites

| Requirement | Notes |
|-------------|-------|
| Rust toolchain | `rustc` and `cargo`, edition 2021 workspace (1.85+) |
| Optics stack | Sibling `../optics` checkout for cross-repository development or installed native libraries for the canonical build |
| Meson and a C23 compiler | Required to build the native Optics libraries (`flux`, `lens`, `iris`) |
| Vulkan 1.3 runtime and loader | `flux` is Vulkan-first; requires a Vulkan-capable GPU driver |
| Wayland compositor | Runtime environment (`iris` creates Wayland xdg-shell surfaces) |
| pkg-config | Required by build scripts to locate native dependencies |

## Choose Your Workflow

| Role / Task | Workflow | Why |
|-------------|----------|-----|
| **Contributing to Arca only** | [Canonical workflow](#canonical-workflow-contributor) | You do not modify Optics. The tagged Optics release bindings and system-installed native libraries are all you need. No sibling checkout or Cargo patch. |
| **Developing Arca and Optics together** | [Local workflow](#local-workflow-dual-maintainer) | You isolate the live sibling Optics patch, Cargo lockfile, and target directory in a linked `arca-dev` worktree. |

### How the Two Roles Differ

Arca separates Rust source selection from native library discovery.
This separation allows both modes to coexist cleanly:

| Concern | Canonical workflow | Local workflow |
|---------|--------------------|----------------|
| Rust bindings | Tagged Optics Git sources (`Cargo.toml`) | `[patch]` entries to `../optics/bindings` |
| Native libraries | System-installed via `pkg-config` | Sibling `../optics/build` via rpath-relay |
| `.cargo/config.toml` | Absent | Local patch enabled (`.cargo/optics-local.toml`) |
| `Cargo.lock` | Canonical and committed | Worktree-local resolution, unstaged by git hook |
| Target directory | Primary `target/` | Worktree-local `target/` |
| Sibling `../optics` required | No | Yes |

---

## Canonical Workflow (Contributor)

Use this workflow when working solely on Arca against an installed Optics
release.

### 1. Build and Test

```bash
cargo check
cargo test
```

### 2. Run Arca

```bash
cargo run
# Or specify a view and starting directory:
cargo run -- --miller ~/projects
```

---

## Local Workflow (Dual Maintainer)

Use this workflow when modifying Optics and Arca concurrently.

### 1. Create the Linked Worktree

Run this once from the primary `arca` repository:

```bash
git worktree add -b dev ../arca-dev main
```

The expected sibling layout is:

```text
projects/
├── arca/
├── arca-dev/
└── optics/
```

### 2. Enable Local Patch Mode

Enter the development worktree, copy the local patch configuration, and
configure repository hooks:

```bash
cd ../arca-dev
cp .cargo/optics-local.toml .cargo/config.toml
git config core.hooksPath .githooks
```

The `.githooks/pre-commit` hook prevents worktree-local changes to
`Cargo.lock` and `.cargo/config.toml` from being committed accidentally.

### 3. Build Sibling Optics Libraries

Configure and build the native Optics libraries in `../optics`:

```bash
# Configure once:
meson setup ../optics/build ../optics -Dtests=false -Dbuildtype=debugoptimized

# Compile native libraries:
meson compile -C ../optics/build
```

`debugoptimized` keeps assertions active while enabling `-O2` optimizations.

### 4. Daily Development Loop

```bash
cd ../arca-dev

# Rebuild native libraries after changing Optics C code:
meson compile -C ../optics/build

# Check or build Arca:
cargo check -p arca
cargo test --workspace

# Run Arca with live sibling optics:
cargo run --bin arca
```

Runtime dynamic library lookup is handled automatically by the rpath-relay
build scripts reading `DEP_IRIS_RS_RPATHS`. Never set `LD_LIBRARY_PATH`
manually.

For full instructions on committing, synchronizing, and promoting Optics
releases, see [Cross-Repository Development](cross-repository-development.md).

---

## Tests

Arca uses Cargo test suites across its workspace crates:

```bash
# Run all tests in the workspace:
cargo test

# Run pure unit tests (xdg specs and engine domain logic, fast, no graphics needed):
cargo test -p arca-xdg
cargo test -p arca-engine

# Run UI rendering and headless frame tests:
cargo test -p arca-ui
```

`arca-xdg` and `arca-engine` have zero GUI dependencies and can be tested on any system.
`arca-ui` tests headless frame construction using the safe `lens` API.

---

## Troubleshooting

| Symptom | Cause | Solution |
|---------|-------|----------|
| `pkg-config` cannot find `iris`, `lens`, or `flux` | Optics C libraries not installed and not built in sibling tree | In local mode, build `../optics/build` with Meson. In canonical mode, install matching Optics packages. |
| `Cargo.lock` unstaged during `git commit` | Pre-commit hook active in local patch mode | Normal behavior. The hook keeps worktree-local lockfile diffs out of commits. |
| Wayland surface creation fails | No active Wayland compositor | Run within a Wayland session (e.g. Aegis, Sway, Hyprland, or GNOME Wayland). |
| Vulkan device initialization fails | Missing GPU drivers or loader | Ensure Vulkan ICD loader and drivers (Mesa / proprietary) are installed and `vulkaninfo` succeeds. |

## See Also

- [Cross-Repository Development](cross-repository-development.md) — Detailed
  worktree lifecycle, commit protection, and release promotion.
- [Optics Migration](optics-migration.md) — Architecture history and workspace
  structure.
- [AGENTS.md](../../AGENTS.md) — Contributor rules, conventions, and crate
  responsibilities.
- [README](../../README.md) — User overview, features, and configuration.
