---
id: ADR-0014
title: "Optics Link Mode: Library-Level Flexibility, Static Release Artifact"
status: accepted
date: 2026-11-20
scope: [build/dependencies, build/link-mode, packaging/distribution]
superseded_by: null
negative_knowledge: true
---

# 0014. Optics Link Mode: Library-Level Flexibility, Static Release Artifact

- Status: Accepted
- Date: 2026-11-20
- Deciders: Arca Architecture Working Group
- Consulted: Optics App/Window/UI/Gfx Maintainers, Distribution Packagers
- Extends: [ADR-0013](0013-synchronize-with-optics-v0.0.82-semantic-module-naming.md), Optics ADR-0169 / ADR-0170 / ADR-0172
- Amended by: [ADR-0015](0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md) (shared-preferred default and the additive `static` feature in Optics v0.0.83)

---

## Context and Problem Statement

Arca consumes the Optics C stack through the official binding crates
(`optics-app`, `optics-window`, `optics-ui`, `optics-ui-sys`, `optics-gfx-sys`,
`optics-motion`), and the `optics-*-sys` build scripts discover the native
libraries through `pkg-config`.

Optics is a general-purpose stack with many embedders, and its build helper
already models the link mode as a **build input** rather than a property of the
library (`crates/optics-build/src/lib.rs`):

| Build input | `optics-build` behaviour |
| :--- | :--- |
| `OPTICS_STATIC=1` | require the `liboptics_*.a` archives; fail if absent |
| `OPTICS_DYNAMIC=1` | force the `liboptics_*.so` shared objects |
| `static` Cargo feature | request the archives; the environment still overrides it |
| neither | shared-preferred: shared objects if the probe finds them, else the archives on a static-only tree (Optics v0.0.83) |

Optics `v0.0.82` used a static-preferred default instead; `v0.0.83` switched
the default to shared-preferred and added the additive `static` feature.
ADR-0015 records that amendment. The flexibility is correct and must stay. The
problem is on Arca's side: the default build is *silent*, so the same commit
yields different artifacts depending on whether the build host happens to ship
archives, and everything downstream (loader paths, packaging dependencies,
runtime failure modes) changes with it:

1. A binary linked against the shared stack carries four `DT_NEEDED` entries
   (`liboptics_app.so.0.0.83`, `liboptics_window`, `liboptics_ui`,
   `liboptics_gfx`), so `/usr/bin/arca` fails to start on a host whose Optics
   package is missing or is a different pre-1.0 patch release.
2. The shared branch makes the wrapper crates publish build-tree directories,
   which Arca's `build.rs` files turn into `-Wl,-rpath`: a distributable
   artifact would embed a developer path such as
   `/data/projects/optics/build/libs/app`.
3. Arca's `build.rs` files relay those rpaths and nothing else. On a static
   branch they relay nothing, so their silence could not distinguish "the
   archives were linked" from "nobody looked".

An early draft of this decision tried to fix that by making Arca's own build
scripts *reject* the shared branch. That was wrong: it moved a release-policy
constraint into the library's consumer and removed a capability Optics
deliberately offers. It also broke a legitimate workflow — a developer on a
host with only the runtime package could no longer run `cargo test`.

Arca is a single self-contained desktop binary, and its Optics dependency is
pinned by tag to a pre-1.0 ABI that changes in patch releases. The *release*
artifact should embed it. Development builds should follow the host.

---

## Decision Drivers

- Driver 1: **Every mode stays available**: `OPTICS_STATIC=1`,
  `OPTICS_DYNAMIC=1`, the `static` Cargo feature, and the shared-preferred
  default must all work for any Arca build; no Arca-specific rule may take a
  mode away.
- Driver 2: **The mode is chosen by the build, not the library**: link
  selection is a caller decision expressed through the upstream switches and
  feature, not something the Optics crates bake in.
- Driver 3: **Release artifacts are reproducible**: the shipped binary must
  embed the Optics modules regardless of what the release host happens to have
  installed.
- Driver 4: **Development stays unblocked**: a shared-only host must still
  build, test, and run, without the developer discovering a new environment
  variable first.
- Driver 5: **Mechanical enforcement**: the release pipeline must verify the
  actual ELF output, not the intent recorded in prose.

---

## Architectural Decisions

### 1. `optics-build` Keeps Full Link-Mode Flexibility

The upstream helper is not patched, wrapped, or shadowed by Arca:
`OPTICS_STATIC`, `OPTICS_DYNAMIC`, the additive `static` feature, and the
shared-preferred default all behave as documented, for Arca and for any other
embedder. Any consumer that needs the shared stack passes `OPTICS_DYNAMIC=1`;
any consumer that needs a guarantee passes `OPTICS_STATIC=1` or enables
`static`. Arca's manifest deliberately does not enable `static`, so a
downstream packager can still force the shared branch.

### 2. Arca's Build Scripts Only React to the Outcome

`crates/arca/build.rs` and `crates/arca-ui/build.rs` keep their original
single responsibility: when the probe chose the shared objects, the wrapper
crates publish their directories as `DEP_*_RPATHS`, and the scripts relay them
into `-Wl,-rpath` (with `--disable-new-dtags` so transitive dependencies
resolve). When the probe chose the archives the metadata is empty and no rpath
is emitted.

Neither script inspects or overrides the mode, so a build cannot fail because
Arca disagreed with a legitimate Optics configuration.

### 3. The Release Pipeline Pins the Mode

Reproducibility is a property of the release build, so that is where the pin
lives:

```bash
OPTICS_STATIC=1 cargo build --release -p arca
scripts/check-optics-link.sh --expect static target/release/arca
```

`OPTICS_STATIC=1` delegates the requirement to `optics-build`, which already
fails with the correct remediation when no archive exists
(`optics-build/src/lib.rs:469`). The script then asserts the artifact instead of
trusting the environment. CI runs the same pair, and `docs/dev/packaging.md`
makes it the packaging contract.

### 4. Link Modes Are Verified, Not Assumed

`scripts/check-optics-link.sh` inspects the ELF output and reports the effective
mode:

| Observed | Detected mode |
| :--- | :--- |
| `liboptics_*.so` in `DT_NEEDED` | dynamic |
| no Optics `DT_NEEDED` entry, `optics_app_run` in the symbol table | static |
| no Optics `DT_NEEDED` entry, stripped, `optics_*` references still present | static (unverified) |
| none of the above | unknown (a build failure, not a mode) |

With `--expect static` it additionally rejects an Optics runpath — any runpath
naming an Optics tree, or any runpath directory that actually holds
`liboptics_*.so`, so an install into `/usr/local/lib` is not missed; with
`--expect dynamic` it rejects an embedded stack. Without `--expect` it reports
and succeeds, so development builds can use it as a diagnostic.

The stripped case matters because distribution packaging strips binaries by
policy. After `strip` the symbol table is gone, so the gate cannot verify the
embedded symbols, and `--expect static` fails rather than claiming evidence it
cannot see. Packagers either run the gate before stripping, use
`strip --strip-debug` (which preserves `.symtab`), or pass
`--allow-unverified-stripped` to accept the weaker evidence — no Optics
`DT_NEEDED` entry, no Optics runpath, and at least one `optics_*` name left in
the binary — explicitly.

Static linking is scoped to **Optics**. `libc`, the Vulkan loader, the Wayland
client, `xkbcommon`, and the font stack (`fontconfig`, `freetype`, `harfbuzz`,
`fribidi`) remain shared dependencies of the process: a fully static
glibc/Vulkan binary does not link (`-static-pie` fails on TLS symbols in
`liboptics_gfx.a` and on the Vulkan loader's `dlopen` contract), and a
statically linked `libvulkan` cannot load ICD drivers. `optics-motion` is pure
Safe Rust and needs no archive at all (Optics ADR-0158).

### Normative Invariants Established

- `[INV-BUILD-01] Release Link Mode`: the *release* `arca` binary must embed
  the Optics native stack — no `liboptics_*.so` `DT_NEEDED` entry, no Optics
  build-tree runpath, `optics_app_run` present. Development builds are exempt
  and may report either mode. Enforced by
  `scripts/check-optics-link.sh --expect static` in the release pipeline and
  the CI job `test`.

---

## Consequences

### Positive

- Optics keeps supporting both modes for every embedder; Arca adds no policy on
  top of it.
- The shipped binary is self-contained: it starts on a host without an Optics
  runtime package and cannot load a mismatched pre-1.0 module with a different
  descriptor layout.
- No developer path leaks into a distributable artifact; `readelf -d` on the
  release binary shows only system `DT_NEEDED` entries.
- A shared-only host still builds, tests, and runs, because the default probe
  and `OPTICS_DYNAMIC=1` both remain valid.
- Distribution packaging loses a runtime dependency and its version-coupling
  constraint.

### Negative & Trade-offs

- The static release binary is larger (≈5.4 MiB versus ≈3.0 MiB for a thin
  shared binary on x86_64, because `liboptics_gfx.a` is 6.8 MiB of object files)
  and each Optics C change forces a relink of the Rust binaries.
- The release host must have the static archives: `OPTICS_STATIC=1` turns their
  absence into a build failure. A distro package that ships only
  `liboptics_*.so` cannot produce Arca's release artifact without also exposing
  the archives at build time (`-Ddefault_library=both`).
- Because development builds may be dynamic, a developer's local binary can be
  misread as "the release configuration". `scripts/check-optics-link.sh` makes
  the difference explicit; release notes should quote its `--expect static`
  result.
- Duplicate copies of Optics exist when several statically linked applications
  share a desktop; the per-process memory win of shared modules is given up in
  the release artifact deliberately.
- A full static process image remains impossible on this stack (glibc, Vulkan
  ICD discovery); the pin is scoped to Optics.

---

## Rejected Alternatives & Negative Knowledge

### 1. Make Arca's build scripts require the static branch

- **Why considered**: catches a misconfigured release host at `cargo build`
  time instead of at packaging time.
- **Why rejected**: it takes a capability away from Optics' consumers and
  blocks legitimate development on shared-only hosts. The correct place for a
  release-only requirement is the release invocation (`OPTICS_STATIC=1`), which
  upstream already implements, plus an artifact assertion.

### 2. Have Arca `[patch]` a stricter `optics-build`

- **Why considered**: one place to change, applies to every workspace member.
- **Why rejected**: patching an upstream helper so it enforces a downstream
  policy is exactly the coupling this ADR removes, and the local patch template
  is already reserved for source selection during cross-repository development.

### 3. Rely on the default probe and stop there

- **Why considered**: zero changes; `optics-build` links whichever flavour the
  host provides.
- **Why rejected**: the fallback is silent, so the artifact depends on the build
  host. A CI image with only `liboptics_*.so` would keep producing the shared
  binary and the runpath while every check still passed. (In Optics v0.0.83 the
  default became shared-preferred, which makes this failure mode *more* likely,
  not less.)

### 4. `RUSTFLAGS="-C target-feature=+crt-static"` for a fully static binary

- **Why considered**: one flag, no manifest change, no runtime dependencies at
  all.
- **Why rejected**: it does not link. `liboptics_gfx.a` references
  `__tls_get_addr@@GLIBC_2.3`, so the linker reports a DSO missing from the
  command line, and a statically linked glibc breaks the Vulkan loader's
  `dlopen` of ICD drivers. Verified on x86_64 with clang 22 / binutils 2.46.

### 5. Musl target (`x86_64-unknown-linux-musl`)

- **Why considered**: a portable static process image without glibc.
- **Why rejected**: Optics links glibc-oriented system libraries here (font
  stack, `libsystemd`, Wayland client) and no musl archives or `musl-gcc` cross
  toolchain are available; a musl port is an Optics-level effort, not an Arca
  manifest change.

### 6. Keep the shared default and ship an rpath of `$ORIGIN/../lib`

- **Why considered**: smaller binary and one shared Optics copy per system.
- **Why rejected**: for the *release* artifact it preserves the pre-1.0 ABI
  coupling — a mismatched `liboptics_ui.so.0.0.83` cannot satisfy a build from a
  different patch release — and reintroduces a loader search path into the
  package. It stays available as a development configuration.

### 7. Vendor the Optics archives into the repository

- **Why considered**: makes the release build independent of any installed
  Optics.
- **Why rejected**: 12 MiB of binary artifacts would have to be committed and
  kept in lockstep with the tagged bindings, and license/provenance review
  would be bypassed. The archives remain a release-host prerequisite
  discovered through `pkg-config`, exactly like the headers.

---

## Links

- [ADR 0013: Synchronize with Optics v0.0.82](0013-synchronize-with-optics-v0.0.82-semantic-module-naming.md)
- [ADR 0015: Synchronize with Optics v0.0.83 and Shared-Preferred Linking](0015-synchronize-with-optics-v0.0.83-and-shared-preferred-linking.md)
- [Cross-Repository Development Workflow](../dev/cross-repository-development.md)
- [Build & Run Guide](../dev/build-and-run.md)
- [Distribution Packaging](../dev/packaging.md)
- [Distribution Packaging (public)](../how-to/install-and-package.md)
