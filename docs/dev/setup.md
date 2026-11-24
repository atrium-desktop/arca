# Setup

How to build, test, and run Arca for development.

## Prerequisites

| Requirement | Notes |
|-------------|-------|
| Rust toolchain | `rustc` and `cargo`, edition 2021 workspace (1.85+) |
| Optics stack | Sibling `../optics` checkout for cross-repository development or installed native libraries for the canonical build. `Cargo.toml` enables Optics's `static` feature by default so canonical and release builds embed `liboptics_*.a`; `arca-dev` sets `[env] OPTICS_SOURCE_DIR`, `OPTICS_BUILD_DIR`, and `OPTICS_DYNAMIC = "1"` in `.cargo/optics-local.toml` for fast shared-library iteration |
| Meson and a C23 compiler | Required to build the native Optics libraries (`optics-app`, `optics-gfx`, `optics-ui`, `optics-window`); Optics v0.0.86 needs `#embed` support (GCC >= 15, Clang >= 19) |
| Vulkan 1.3 runtime and loader | `optics-gfx` is Vulkan-first; requires a Vulkan-capable GPU driver |
| Wayland compositor | Runtime environment (`optics-window` creates Wayland xdg-shell surfaces) |
| pkg-config | Required by build scripts to locate native dependencies |

## Choose Your Workflow

| Role / Task | Workflow | Why |
|-------------|----------|-----|
| **Contributing to Arca only** | [Canonical workflow](#canonical-workflow-contributor) | You do not modify Optics. The tagged Optics release bindings and system-installed native libraries are all you need. No sibling checkout or Cargo patch. |
| **Developing Arca and Optics together** | [Local workflow](#local-workflow-dual-maintainer) | You point the native build scripts at a live sibling Optics checkout and isolate the target directory in a linked `arca-dev` worktree; the Cargo graph and lockfile stay canonical. |

### How the Two Roles Differ

Arca separates Rust source selection from native library discovery.
This separation allows both modes to coexist cleanly:

| Concern | Canonical workflow | Local workflow |
|---------|--------------------|----------------|
| Rust bindings | Pinned Optics Git source (`features = ["static"]`) | Same pinned Git source |
| Native linkage | Static `liboptics_*.a` archives by default (override with `OPTICS_DYNAMIC=1`) | Shared `../optics/build` via `[env] OPTICS_DYNAMIC = "1"` + `-rpath` |
| `.cargo/config.toml` | Absent | `OPTICS_SOURCE_DIR` / `OPTICS_BUILD_DIR` + `OPTICS_DYNAMIC=1` (`.cargo/optics-local.toml`) |
| `Cargo.lock` | Canonical and committed | Same canonical lockfile |
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

The `.githooks/pre-commit` hook validates the canonical `Cargo.lock` and
refuses a staged `.cargo/config.toml`; the lockfile itself is committed from
every worktree.

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
`arca-ui` tests headless frame construction using the safe `optics-ui` API.

---

## Troubleshooting

| Symptom | Cause | Solution |
|---------|-------|----------|
| `pkg-config` cannot find `optics-app`, `optics-window`, `optics-ui`, or `optics-gfx` | Optics C libraries not installed and no sibling build discovered | In local mode, build `../optics/build` with Meson and keep `OPTICS_SOURCE_DIR` / `OPTICS_BUILD_DIR` in `.cargo/config.toml`. In canonical mode, install matching Optics packages. |
| `Cargo.lock` fails `--locked` during `git commit` | Manifest changed without updating the lockfile | Run `cargo update` / `cargo check` and stage the canonical `Cargo.lock`. |
| Wayland surface creation fails | No active Wayland compositor | Run within a Wayland session (e.g. Aegis, Sway, Hyprland, or GNOME Wayland). |
| Vulkan device initialization fails | Missing GPU drivers or loader | Ensure Vulkan ICD loader and drivers (Mesa / proprietary) are installed and `vulkaninfo` succeeds. |

## See Also

- [Cross-Repository Development](cross-repository-development.md) — Detailed
  worktree lifecycle, commit protection, and release promotion.
- [Optics Migration](optics-migration.md) — Architecture history and workspace
  structure.
- [README](../../README.md) — User overview, features, and configuration.
