# Setup

How to build, test, and run Arca for development.

## Prerequisites

| Requirement | Notes |
|-------------|-------|
| Rust toolchain | `rustc` and `cargo`, edition 2024 workspace (1.88+) |
| Optics stack | Sibling `../optics` checkout for cross-repository development or installed native libraries for the canonical build. `Cargo.toml` enables Optics's `static` feature by default so canonical and release builds embed `liboptics_*.a`; `arca-dev` copies `.cargo/optics-local.toml` to `.cargo/config.toml` to patch Optics crates to `../optics/crates/*` and set `[env] OPTICS_SOURCE_DIR`, `OPTICS_BUILD_DIR`, and `OPTICS_DYNAMIC = "1"` for fast shared-library iteration |
| Meson and a C23 compiler | Required to build the native Optics libraries (`optics-app`, `optics-gfx`, `optics-ui`, `optics-window`); Optics needs `#embed` support (GCC >= 15, Clang >= 19) |
| Vulkan 1.3 runtime and loader | `optics-gfx` is Vulkan-first; requires a Vulkan-capable GPU driver |
| Wayland compositor | Runtime environment (`optics-window` creates Wayland xdg-shell surfaces) |
| pkg-config | Required by build scripts to locate native dependencies |

## Choose Your Workflow

| Role / Task | Workflow | Why |
|-------------|----------|-----|
| **Contributing to Arca only** | [Canonical workflow](#canonical-workflow-contributor) | You do not modify Optics. The tagged Optics release bindings and system-installed native libraries are all you need. No sibling checkout or Cargo patch. |
| **Developing Arca and Optics together** | [Local workflow](#local-workflow-dual-maintainer) | You patch Optics crates and native discovery to a live sibling `../optics` checkout inside a linked `arca-dev` worktree so Rust and C stay co-versioned; the primary `main` worktree owns the canonical `Cargo.lock`. |

### How the Two Roles Differ

Arca isolates local Optics overrides in a linked `dev` worktree so canonical
builds and live cross-repository iteration coexist cleanly:

| Concern | Canonical workflow | Local workflow |
|---------|--------------------|----------------|
| Rust bindings | Pinned Optics Git source (`features = ["static"]`) | Patched to `../optics/crates/*` via `.cargo/config.toml` |
| Native linkage | Static `liboptics_*.a` archives by default (override with `OPTICS_DYNAMIC=1`) | Shared `../optics/build` via `[env] OPTICS_DYNAMIC = "1"` + `-rpath` |
| `.cargo/config.toml` | Absent | `[patch]` + `OPTICS_SOURCE_DIR` / `OPTICS_BUILD_DIR` + `OPTICS_DYNAMIC=1` (`.cargo/optics-local.toml`) |
| `Cargo.lock` | Canonical and committed | Locally rewritten by `[patch]`; refused by pre-commit hook |
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

### 2. Enable Local Optics Discovery

Enter the development worktree, copy the local environment configuration, and
configure repository hooks:

```bash
cd ../arca-dev
cp .cargo/optics-local.toml .cargo/config.toml
git config core.hooksPath .githooks
```

The read-only `.githooks/pre-commit` hook refuses a staged `.cargo/config.toml`
and, while local `[patch]` mode is active, refuses staged `Cargo.lock` or
release-shaped commits so the primary `main` worktree owns the canonical
lockfile.

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

The root `Cargo.toml` enables Optics's `static` feature by default, while
`arca-dev`'s `.cargo/config.toml` sets `[env] OPTICS_DYNAMIC = "1"` so daily
builds load `liboptics_*.so` from `../optics/build` through the relayed
`-Wl,-rpath` — never set `LD_LIBRARY_PATH` manually. Pass `OPTICS_DYNAMIC=0` in
the shell to exercise the production static link on demand;
`scripts/check-optics-link.sh <binary>` reports which mode was used.

For full instructions on committing, synchronizing, and promoting Optics
releases, see [Local Optics Development](optics-local-development.md).

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
`arca-ui` tests headless frame construction using the safe `optics-ui` API.

---

## Troubleshooting

| Symptom | Cause | Solution |
|---------|-------|----------|
| `pkg-config` cannot find `optics-app`, `optics-window`, `optics-ui`, or `optics-gfx`, or reports a version mismatch | Optics C libraries not installed, or local `Cargo.lock` still points at an older Optics version than `../optics` | In local mode, build `../optics/build` with Meson, keep `.cargo/optics-local.toml` copied to `.cargo/config.toml`, and run `cargo update -p optics-app -p optics-window -p optics-ui -p optics-ui-sys -p optics-gfx-sys -p optics-motion` after `../optics` bumps its version. In canonical mode, install matching Optics packages. |
| `pre-commit` refuses `Cargo.lock` | `Cargo.lock` was staged in patched `arca-dev` mode, or canonical `Cargo.lock` is out of date in `main` | In `arca-dev`, run `git restore --staged -- Cargo.lock`. In `main`, run `cargo check --workspace` and stage the regenerated canonical `Cargo.lock`. |
| Wayland surface creation fails | No active Wayland compositor | Run within a Wayland session (e.g. Aegis, Sway, Hyprland, or GNOME Wayland). |
| Vulkan device initialization fails | Missing GPU drivers or loader | Ensure Vulkan ICD loader and drivers (Mesa / proprietary) are installed and `vulkaninfo` succeeds. |

## See Also

- [Local Optics Development](optics-local-development.md) — Detailed
  worktree lifecycle, commit protection, and release promotion.
- [Optics Migration](optics-migration.md) — Architecture history and workspace
  structure.
- [README](../../README.md) — User overview, features, and configuration.
