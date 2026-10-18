---
id: ADR-0004
title: "0004. Optics v0.0.60 Motion and Premultiplied Image Upload Alignment"
status: accepted
date: 2026-10-17
---

# 0004. Optics v0.0.60 Motion and Premultiplied Image Upload Alignment

- Status: Accepted
- Date: 2026-10-17
- Deciders: Ming Li, Arca UI Architecture Team
- Consulted: Optics/Transit/Flux Maintainers, Tessera Design System Team
- Informed: Arca contributors

## Context and Problem Statement

Arca depends on the Optics graphics and UI engine (`flux`, `lens`, `iris`, `transit`). In earlier builds (aligned to Optics `v0.0.51`), several UI mechanisms relied on workarounds or duplicated in-tree mathematics:

1. **Ad-hoc Motion Mathematics**: Quick Look preview animation in `arca-ui` hand-rolled exponential approach formulas (`self.progress += (target - self.progress) * (dt * speed).min(1.0)`), manual clamping, and private cubic easing approximations (`1.0 - (1.0 - progress).powi(3)`), bypassing the centralized `transit` motion library.
2. **Manual CPU Premultiplication in Thumbnail Uploads**: In `arca-ui::thumbs`, decoded RGBA thumbnail buffers were cloned into heap scratch buffers and mutated pixel-by-pixel in software before calling `flux_image_create`, duplicating the canvas premultiplication convention in application space.
3. **Full-Pass Opacity Fades**: Prior to Optics ADR-0109, node opacity was wrapped in offscreen layer passes, creating unnecessary render-pass transitions during modal transitions.

With upstream Optics reaching `v0.0.60` (introducing the promoted standalone `transit` library with explicit `reduced_motion` parameters, command-level premultiplied opacity modulation under ADR-0109, and the `flux_image_update_region_premultiply` upload seam), Arca should adopt these first-class interfaces to ensure zero mathematical duplication and optimal GPU resource usage.

## Decision Drivers

- Driver 1: **Zero-Math Duplication Invariant**: All easing curves, spring dynamics, and exponential approach calculations must delegate to Optics `transit` (`transit::approach`, `transit::ease_out_cubic`). Application crates must never re-implement these primitives.
- Driver 2: **Clean Image Upload Pipeline**: Hardware texture uploads of straight RGBA data should delegate premultiplication to `flux_image_update_region_premultiply`, eliminating redundant CPU buffer copies and custom pixel loops.
- Driver 3: **Command-Level Opacity Modulation**: Modal card transitions leverage Optics ADR-0109 command-level opacity modulation (`frame.set_opacity(ease)`) for seamless enter/exit fading without extra render targets.
- Driver 4: **Global Lockstep Versioning**: Synchronize workspace manifests and local worktree patches with Optics `v0.0.60`.

## Considered Options

- Option 1: Keep custom in-tree math in `preview.rs` and manual software premultiplication in `thumbs.rs`, bumping only the version tags in `Cargo.toml`.
- Option 2: Adopt `transit` motion primitives and `flux_image_update_region_premultiply` across `arca-ui`, wiring `transit` into workspace dependencies, rpath relays, and test suites.

## Decision Outcome

Chosen option: **Option 2**, because it aligns Arca with the architectural contracts of the Optics monorepo, guarantees robust handling of display frequency fluctuations and `reduced_motion` accessibility modes, and eliminates redundant CPU memory traffic during thumbnail decoding.

### Implementation Specifics

1. **Optics Dependency Bump to `v0.0.60`**:
   - `Cargo.toml` updates `iris`, `lens`, `lens-sys`, `flux`, `flux-sys` to `v0.0.60` and adds `transit` / `transit-sys`.
   - `.cargo/optics-local.toml` and `.cargo/config.toml` include path patches for `transit` and `transit-sys`.
   - `build.rs` in `arca-ui` and `arca` relays `DEP_TRANSIT_RPATHS` and `DEP_TRANSIT_RS_RPATHS`.

2. **Transit Motion Adoption in Preview (Zero-Approach Reveal Motion)**:
   - `preview.rs` replaces ad-hoc approach logic with critically damped `transit::Spring` (`PREVIEW_SPRING = SpringParams::new(320.0, 1.0)`) integrated via `spring.advance_substep(target, PREVIEW_SPRING, dt, 1.0 / 120.0, reduced_motion)` (Optics ADR-0108).
   - Resolves immediately to the target state under `reduced_motion` without intermediate pacing.
   - Quick Look modal container sets `frame.set_opacity(ease)` during enter/exit animation (Optics ADR-0109).

3. **Optics Premultiplied Upload Seam in Thumbnails**:
   - `thumbs.rs` uploads raw decoded RGBA bytes directly via `flux_image_update_region_premultiply`, removing `thumb.rgba.clone()` and the manual chunked pixel loop.

4. **Elimination of Legacy FFI Shims in UI Icons**:
   - `icons.rs` migrates `selectable_icon_sized` to safe native `Frame::selectable_icon`, removing manual `CString` allocation and direct `lens_sys::lens_selectable` invocations.

### Positive Consequences

- Guaranteed convergence and proper reduced-motion behavior across all display refresh rates.
- Eliminated per-thumbnail heap allocations and software premultiplication loops.
- Clean architectural alignment with sibling projects (`tessera`, `aphrodite`, `sigil`).

### Negative Consequences

- Adds a direct dependency on `transit` in `arca-ui`.
- Mitigation: `transit` is a pure mathematical library with zero runtime overhead and already linked transitively in the Optics stack.

## Rejected Alternatives

### 1. Retaining in-tree math and bumping tags only
- **Why considered**: Minimal code change surface.
- **Why rejected**: Violates the Optics stack architectural invariant requiring centralized motion vocabulary and zero-math duplication. Hand-rolled formulas lack consistent clamping, sub-stepping safety, and canonical accessibility integration.

### 2. Full offscreen snapshot rendering for preview overlays
- **Why considered**: Capturing the preview into an offscreen render target before blitting with opacity.
- **Why rejected**: Incurs unnecessary GPU memory allocations and render pass barriers when Optics ADR-0109 provides command-level premultiplied opacity modulation directly in the main pass.

## Links

- [ADR 0001: Optics Stack Migration](0001-optics-stack-migration.md)
- [ADR 0003: Borderless Surfaces, Subtle Controls, and Visual Density Hierarchy](0003-borderless-chrome-and-dense-hierarchy.md)
- Optics ADR-0106: Monorepo Identity Unification — glyph, vista, and transit
- Optics ADR-0108: Motion Sub-stepping Integration and Render Target Bucket Allocation
- Optics ADR-0109: Premultiplied Command Opacity Modulation, Scissor-Bounded Layers, and Glyph Batching
