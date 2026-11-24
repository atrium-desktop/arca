# test-support

Shared infrastructure for Arca's test suites. This crate is a test-only
workspace member (`publish = false`) and is never a production dependency.

It exists so three layers share one implementation of:

- **`fs`** — unique scratch directories with RAII cleanup.
- **`env`** — serialized, restoring process-environment mutations for XDG
  variables.
- **`poll`** — bounded polling for asynchronous filesystem/watcher/worker
  effects.
- **`workspace`** — workspace/crate paths and built-binary lookup.

The crate is deliberately **std-only**. The native-free CI lane builds it with
`--no-default-features` on hosts that cannot build the Optics C stack.

See [`docs/dev/testing.md`](../../docs/dev/testing.md) and
[`tests/README.md`](../../tests/README.md).
