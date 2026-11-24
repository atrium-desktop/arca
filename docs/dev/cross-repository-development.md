# Cross-Repository Development Workflow

This guide defines the dual-worktree workflow for developing Arca alongside
**Optics**. Both worktrees use the same committed Cargo graph and the same
canonical `Cargo.lock`; a linked development worktree only points the native
build scripts at a sibling Optics checkout and selects shared libraries for
fast iteration.

## 1. Architecture and Invariants

| Concern | Primary worktree (`main`) | Development worktree (`dev`) |
|---------|---------------------------|------------------------------|
| Directory | `../arca/` | `../arca-dev/` |
| Branch | `main` | `dev` |
| `Cargo.toml` | Identical; `features = ["static"]` on native Optics deps | Identical |
| Rust bindings | Pinned Optics Git tag | Same pinned Git tag |
| `Cargo.lock` | Canonical, committed | Same canonical lockfile |
| Cargo config | `.cargo/config.toml` absent (uses installed Optics) | Copied from [.cargo/optics-local.toml](../../.cargo/optics-local.toml) |
| Native discovery | System-installed Optics | Sibling `../optics` via `OPTICS_SOURCE_DIR` / `OPTICS_BUILD_DIR` |
| Native linkage | Static `liboptics_*.a` (default `static` feature) | Shared `../optics/build/` via `[env] OPTICS_DYNAMIC = "1"` + rpath relay |
| Target cache | Primary `target/` | Worktree-local `target/` |

### Core invariants

1. **One canonical Cargo graph.** The committed `Cargo.lock` resolves the
   tagged Optics Git sources in both worktrees. Local development never uses a
   `[patch]`, so `cargo check --locked` and `cargo metadata --locked` mean the
   same thing everywhere and the lockfile is committable from `dev`.
2. **Environment selects the native tree, not the dependency graph.** The
   `-sys` build scripts accept `OPTICS_SOURCE_DIR` and `OPTICS_BUILD_DIR`;
   `.cargo/config.toml` sets them to `../optics` / `../optics/build`. This keeps
   the Rust crate graph canonical while the C libraries come from a live
   checkout.
3. **Identical manifests.** `main` and `dev` never diverge on `Cargo.toml`
   link settings. The manifest enables `features = ["static"]`; only the
   untracked local `[env]` override selects `OPTICS_DYNAMIC = "1"`.
4. **Optics upstream first.** `main` depends only on an immutable, publicly
   tagged Optics release, never on local or uncommitted Optics state.
5. **Always-buildable main.** Every commit on `main` must pass
   `cargo check --locked --workspace`. The Optics tag bump in `Cargo.toml` and
   the regenerated canonical `Cargo.lock` land in the same atomic commit.
6. **Target isolation.** Keep separate `target/` directories per worktree; do
   not share `CARGO_TARGET_DIR` across canonical and local builds.

## 2. Workspace Setup

```text
projects/
├── arca/          # primary worktree on main (canonical)
├── arca-dev/      # linked worktree on dev (local Optics checkout)
└── optics/        # sibling Optics checkout with build/
```

1. **Create the linked worktree** from `projects/arca/`:
   ```bash
   git worktree add -b dev ../arca-dev main
   ```
2. **Enable local discovery and hooks** inside `projects/arca-dev/`:
   ```bash
   cd ../arca-dev
   cp .cargo/optics-local.toml .cargo/config.toml
   git config core.hooksPath .githooks
   ```
   The template sets `OPTICS_SOURCE_DIR`, `OPTICS_BUILD_DIR`, and
   `OPTICS_DYNAMIC = "1"`. It does not patch Cargo and does not change
   `Cargo.lock`.
3. **Build the sibling Optics tree** (once, or after a fresh checkout):
   ```bash
   meson setup ../optics/build ../optics -Dtests=false -Dbuildtype=debugoptimized
   meson compile -C ../optics/build
   ```
   The default `-Ddefault_library=both` builds both `liboptics_*.so` (local
   iteration) and `liboptics_*.a` (static verification).
4. **Verify discovery and link mode**:
   ```bash
   cargo build -p arca
   cargo tree -i optics-app                                         # must show the pinned git source
   scripts/check-optics-link.sh --expect dynamic target/debug/arca  # shared objects
   ```

## 3. Daily Development (`arca-dev`)

```bash
# Recompile native Optics after C or shader edits:
meson compile -C ../optics/build

# Rust work (no Cargo graph changes, no relink after native-only edits):
cargo check --workspace
cargo nextest run --workspace
cargo test --workspace

# Verify the production static closure on demand:
OPTICS_DYNAMIC=0 cargo build -p arca
scripts/check-optics-link.sh --expect static target/debug/arca
```

Do not run concurrent Meson/Ninja builds against `../optics/build` from
multiple terminals.

The pre-commit hook validates the canonical lockfile whenever a `Cargo.toml`
or `Cargo.lock` is staged. It refuses `.cargo/config.toml`, which is a
machine-local pointer and is gitignored. There is no local-mode lockfile
exclusion.

## 4. Feature-Level Promotion to `main`

`dev` holds fine-grained WIP commits built against local Optics; `main` holds
curated, individually buildable atomic commits.

### 4.1 Publish the Optics tag first

In `projects/optics/`:

```bash
cd ../optics
git checkout main && git pull --ff-only
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin vX.Y.Z
meson compile -C build
```

### 4.2 Squash, re-pin, verify, commit on `main`

In `projects/arca/` (primary worktree, no local `.cargo/config.toml`):

```bash
cd ../arca
git pull --ff-only
git merge --squash dev

# Update every Optics dependency tag in Cargo.toml to vX.Y.Z, then regenerate
# the canonical lockfile against the published tag:
cargo check --workspace
cargo check --locked --workspace

# Confirm the graph still resolves the remote tags:
cargo tree -i optics-app
cargo tree -i optics-gfx

# Full locked verification. Omit OPTICS_* when matching Optics is installed
# system-wide; otherwise point at the sibling build:
OPTICS_SOURCE_DIR=../optics OPTICS_BUILD_DIR=../optics/build cargo test --locked --workspace
OPTICS_DYNAMIC=0 OPTICS_SOURCE_DIR=../optics OPTICS_BUILD_DIR=../optics/build \
  cargo build --locked --release -p arca
scripts/check-optics-link.sh --expect static target/release/arca

git commit -m "feat(<scope>): <summary>

Adopt Optics vX.Y.Z."
git push origin main
```

### 4.3 Reset `dev` onto `main`

Because `dev`'s changes now live in the squash commit on `main`, reset rather
than rebase:

```bash
cd ../arca-dev
git reset --hard main
cargo check --workspace   # local env config still points at ../optics
```

If `dev` is tracked on a remote, update it with
`git push --force-with-lease origin dev`.

## 5. Rebasing Unpromoted `dev` Work

```bash
cd ../arca && git pull --ff-only
cd ../arca-dev
git rebase main
cargo check --locked --workspace
```

The lockfile does not need restoring: it is canonical and is a normal tracked
file in both worktrees.

## 6. Git Hook

[.githooks/pre-commit](../../.githooks/pre-commit):

- validates `cargo metadata --locked` whenever a manifest or lockfile is staged;
- refuses a staged `.cargo/config.toml`;
- does not unstage `Cargo.lock` or rewrite the index.

## See Also

- [Setup](setup.md) — prerequisites and local troubleshooting.
- [Distribution Packaging](packaging.md) — vendoring, install manifest, and
  link-gate verification.
- [ADR-0018: Canonical Lockfile and Environment-Resolved Optics Checkout](../adr/0018-canonical-lockfile-and-environment-resolved-optics.md)
  — rationale for replacing the `[patch]` workflow.
