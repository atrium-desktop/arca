# Contributor Documentation

Material for project contributors: build and architecture notes and the
governance policy that organizes this `docs/` tree.

For the user pitch and quick start, see the root
[README](../../README.md). For crate layout, build and test commands,
and contributor conventions, see [AGENTS.md](../../AGENTS.md).

## Architecture and history

| Page | Topic |
|------|-------|
| [Optics migration](optics-migration.md) | Port from the legacy flux/flux-ui stack to the optics monorepo, workspace layout, binding mechanics, testing strategy, and known limitations |
| [Cross-repository development](cross-repository-development.md) | Linked `lantern-dev` worktree on a local `dev` branch, Cargo `[patch]` against the sibling optics checkout, and the canonical tagged-release workflow |

## Documentation governance

The [documentation](documentation/index.md) directory defines how
documentation is routed, written, and reviewed in this repository. Read
it before adding, moving, or removing any documentation.
