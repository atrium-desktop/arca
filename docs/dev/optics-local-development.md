# Local Optics Development

This guide defines how to develop and integrate Arca against a local sibling
**Optics** checkout. To keep canonical builds isolated from machine-local
overrides while keeping Rust bindings and native C libraries strictly in sync,
the workflow uses a linked Git worktree (`dev`) with a local Cargo `[patch]`
plus shared-library linking (`OPTICS_DYNAMIC = "1"`), and relies on the primary
`main` worktree (`../arca/`) and a read-only Git hook to protect the canonical
`Cargo.lock`.

## 1. Architecture and Invariants

| Concern | Primary worktree (`main`) | Development worktree (`dev`) |
|---------|---------------------------|------------------------------|
| Directory | `../arca/` | `../arca-dev/` |
| Branch | `main` | `dev` |
| `Cargo.toml` | Identical; `features = ["static"]` on native Optics deps | Identical |
| Rust bindings | Pinned Optics Git tag | Patched to `../optics/crates/*` via `.cargo/config.toml` |
| `Cargo.lock` | Canonical, committed | Locally rewritten by Cargo `[patch]`; never committed from `dev` |
| Cargo config | `.cargo/config.toml` absent (uses installed Optics) | Copied from [.cargo/optics-local.toml](../../.cargo/optics-local.toml) |
| Native discovery | System-installed Optics | Sibling `../optics` via `OPTICS_SOURCE_DIR` / `OPTICS_BUILD_DIR` |
| Native linkage | Static `liboptics_*.a` (default `static` feature) | Shared `../optics/build/` via `[env] OPTICS_DYNAMIC = "1"` + rpath relay |
| Target cache | Primary `target/` | Worktree-local `target/` |

### Core invariants

1. **Co-versioned Rust and C in `dev`.** Upstream `optics-build` probes C
   libraries with `pkg-config --exact-version` against `CARGO_PKG_VERSION`, and
   pre-1.0 Optics SONAMEs carry the full version. Patching the Optics crates to
   `../optics/crates/*` guarantees that Rust wrappers, `-sys` crates, C headers,
   and `liboptics_*.so` always come from the same local commit — even when
   `../optics` has bumped past the tag pinned in `Cargo.toml`.
2. **Primary `main` owns the canonical `Cargo.lock`.** Under a local `[patch]`,
   Cargo rewrites `Cargo.lock` with path entries. That local lockfile drift is
   an expected, isolated trade-off of the `dev` worktree; `.githooks/pre-commit`
   refuses staged `Cargo.lock` edits and release-shaped commits while the patch
   is active, without mutating the Git index.
3. **Identical manifests.** `main` and `dev` never diverge on `Cargo.toml`
   link settings or path dependencies. The manifest enables
   `features = ["static"]` against the tagged Git source; only the untracked
   `.cargo/config.toml` applies the local `[patch]` and `[env] OPTICS_DYNAMIC = "1"`.
4. **Optics upstream first.** `main` depends only on an immutable, publicly
   tagged Optics release, never on local or uncommitted Optics state.
5. **Always-buildable `main`.** Every commit on `main` must pass
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
2. **Enable local Optics patching, discovery, and hooks** inside `projects/arca-dev/`:
   ```bash
   cd ../arca-dev
   cp .cargo/optics-local.toml .cargo/config.toml
   git config core.hooksPath .githooks
   ```
   The template patches all Optics crates used by Arca to `../optics/crates/*`
   and sets `OPTICS_SOURCE_DIR`, `OPTICS_BUILD_DIR`, and `OPTICS_DYNAMIC = "1"`.
3. **Build the sibling Optics tree** (once, or after a fresh checkout):
   ```bash
   meson setup ../optics/build ../optics -Dtests=false -Dbuildtype=debugoptimized
   meson compile -C ../optics/build
   ```
   The default `-Ddefault_library=both` builds both `liboptics_*.so` (local
   iteration) and `liboptics_*.a` (static verification).
4. **Refresh the local lockfile when `../optics` is on a newer version** than
   the tag pinned in `Cargo.toml`:
   ```bash
   cargo update -p optics-app -p optics-window -p optics-ui \
     -p optics-ui-sys -p optics-gfx-sys -p optics-motion
   ```
   Cargo locks the patched crates to the sibling checkout's version. Leave the
   modified `Cargo.lock` unstaged in `arca-dev`.
5. **Verify discovery and link mode**:
   ```bash
   cargo build -p arca
   cargo tree -i optics-app                                         # must show ../optics/crates/optics-app
   scripts/check-optics-link.sh --expect dynamic target/debug/arca  # shared objects
   ```

## 3. Daily Development (`arca-dev`)

```bash
# Recompile native Optics after C or shader edits:
meson compile -C ../optics/build

# Rust work (no Rust relink needed after C/shader-only edits):
cargo check --workspace
cargo nextest run --workspace
cargo test --workspace

# After `../optics` bumps its workspace version (for example 0.0.86 -> 0.0.87):
cargo update -p optics-app -p optics-window -p optics-ui \
  -p optics-ui-sys -p optics-gfx-sys -p optics-motion

# Verify the production static closure on demand:
OPTICS_DYNAMIC=0 cargo build -p arca
scripts/check-optics-link.sh --expect static target/debug/arca
```

Do not run concurrent Meson/Ninja builds against `../optics/build` from
multiple terminals.

Because Cargo rewrites `Cargo.lock` under `[patch]`, `git status` in `arca-dev`
will normally show `M Cargo.lock`. Do **not** hide it with
`git update-index --assume-unchanged` (that breaks rebases and branch switches)
and do **not** stage it when committing in `arca-dev`. If you stage it by
mistake, `.githooks/pre-commit` refuses the commit cleanly and tells you to
unstage it.

A hang or crash whose frames are in `liboptics_*.so` is in this checkout's
native code, not in the tagged release, so fix and test it here
(`meson test -C ../optics/build`) before restarting Arca. Debug Optics builds
validate internal invariants (for example the retained store, ADR-0027) and
abort with the broken invariant instead of hanging. For a frozen window see
[Window Frozen](../how-to/troubleshoot-common-issues.md#window-frozen-no-response-at-all).

## 4. Feature-Level Promotion to `main`

`dev` holds fine-grained WIP commits built against local Optics; `main` holds
curated, individually buildable atomic commits with a canonical `Cargo.lock`.

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

# Confirm the graph resolves the remote tags (no local path entries):
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
git checkout -- Cargo.lock
git reset --hard main
cargo check --workspace   # local [patch] and [env] still point at ../optics
```

If `dev` is tracked on a remote, update it with
`git push --force-with-lease origin dev`.

## 5. Rebasing Unpromoted `dev` Work

```bash
cd ../arca && git pull --ff-only
cd ../arca-dev
git checkout -- Cargo.lock
git rebase main
cargo check --workspace
```

Restore the working tree copy of `Cargo.lock` before rebasing so Git never sees
a conflict with the locally patched lockfile; Cargo will regenerate the local
lockfile on the next build.

## 6. Git Hook

[.githooks/pre-commit](../../.githooks/pre-commit) is strictly read-only — it
never unstages files or mutates the Git index:

- always refuses a staged `.cargo/config.toml`;
- in local patch mode (`.cargo/config.toml` contains
  `[patch."https://github.com/ming2k/optics"]`), refuses staged `Cargo.lock`
  changes and release-shaped version bumps in `Cargo.toml`;
- in canonical mode (primary `main` worktree, no Optics `[patch]`), validates
  `cargo metadata --locked` whenever any `Cargo.toml` or `Cargo.lock` is staged.

## See Also

- [Setup](setup.md) — prerequisites and local troubleshooting.
- [Distribution Packaging](packaging.md) — vendoring, install manifest, and
  link-gate verification.
- [ADR-0024: Local Optics Source Patch and Read-Only Lockfile Guardrail](../adr/0024-local-optics-source-patch-and-readonly-lockfile-guardrail.md)
  — rationale for restoring co-versioned `[patch]` in `dev` with a read-only
  hook.
- [ADR-0018: Canonical Lockfile and Environment-Resolved Optics Checkout](../adr/0018-canonical-lockfile-and-environment-resolved-optics.md)
  — superseded experiment and lessons learned.
