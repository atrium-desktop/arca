# Root test suites

This package is the single external-test entry point for the Arca workspace. It
is a workspace member but never a production dependency.

```text
tests/
  common/            shared helpers; re-exports test-support + headless UI harness
  integration/       one entry, one link: main.rs + modules
  e2e/               one entry, one link: main.rs + modules
```

The package's `native` feature (default) compiles the suites that link
`arca-ui`/optics-ui. Build the pure architecture, engine, XDG, and protocol
suites without the Optics toolchain:

```bash
cargo test -p arca-tests --no-default-features
```

Run everything:

```bash
cargo test --workspace
# or, with per-test processes, timeouts, and the binary setup script:
cargo nextest run -p arca-tests
```

The end-to-end modules spawn the production `arca` binary as a separate
process. `.config/nextest.toml` builds `--workspace --bins` once before the
tests that need it, because a test that runs `cargo build` itself blocks on the
target-directory lock the runner already holds. Running the suite outside
nextest means building the workspace binaries first (`cargo build --workspace
--bins`).

`test-support` supplies scratch directories, environment isolation, polling,
and workspace/binary lookup. Production crates must never declare this package or `test-support` as a
normal or build dependency (dev-dependencies for unit fixtures are allowed);
the architecture suite enforces that direction.
