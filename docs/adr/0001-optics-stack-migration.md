# 0001. Optics Stack Migration and Workspace Modularization

- Status: Accepted
- Date: 2026-08-20
- Deciders: Ming Li
- Consulted: Tessera desktop engineering team
- Informed: Arca contributors

## Context and Problem Statement

Arca originally relied on a legacy, bespoke C platform shim (`c_src/platform.c`) generating raw Wayland protocol code (xdg-shell, xdg-decoration, text-input) via custom `build.rs` logic and an unversioned `flux-ui` C toolkit binding. Maintaining raw Wayland protocols, windowing, and GPU context shims created significant maintenance friction, prevented integration with the unified Tessera desktop design system, and lacked a structured separation between business logic and UI presentation.

## Decision Drivers

- Driver 1: Eliminating fragile custom C Wayland shims and unmaintained UI bindings.
- Driver 2: Architectural cohesion with the Tessera companion desktop stack (optics: flux / lens / iris).
- Driver 3: Separation of concerns: testable UI-free business logic vs thin immediate-mode presentation.
- Driver 4: Predictable developer workflow and cross-repository development.

## Considered Options

- Option 1: Maintain bespoke C Wayland shim and modernize `flux-ui` bindings locally.
- Option 2: Adopt standard GTK4 / libadwaita stack.
- Option 3: Migrate to the optics monorepo stack (`flux` Vulkan renderer, `lens` immediate-mode UI, `iris` Wayland application runtime) and modularize the workspace.

## Decision Outcome

Chosen option: "Option 3", because optics provides native Wayland integration, unified GPU rendering with Vulkan, and strict performance bounds, while matching the visual language of the Tessera desktop ecosystem.

The codebase was modularized into three workspace crates:
- `arca-core`: Business logic with zero GUI dependencies (directory model, bookmarks, sorting/filtering, file operations, trash, configuration, thumbnail extraction).
- `arca-ui`: Presentation layer consuming `AppState` per frame via `lens` immediate-mode UI and `iris` windowing.
- `arca`: Thin entrypoint binary linking the stack with rpath-relayed build mechanics.

### Positive Consequences

- All hand-written Wayland C shim files and ad-hoc FFI code were eliminated.
- Business logic is 100% unit-testable in isolation without a display server or GPU.
- Presentation logic can be validated headlessly using `lens::Ui::headless()`.
- Consistent light/dark theming and styling aligned with the Tessera design tokens.

### Negative Consequences

- Requires optics C libraries (iris, lens, flux) installed on the host or available via cross-repo meson build trees.
- Mitigation: Dynamic linking via `DEP_IRIS_RS_RPATHS` and `.cargo/config.toml` patch mechanics for seamless development.

## Links

- [Optics Development Worktree Workflow](../dev/optics-dev-worktree.md)
- [Optics Migration Contributor Guide](../dev/optics-migration.md)
