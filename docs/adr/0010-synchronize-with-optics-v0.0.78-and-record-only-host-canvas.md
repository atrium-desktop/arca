---
id: ADR-0010
title: "Synchronize with Optics v0.0.78 and Retire the Live-Canvas Host Path"
status: accepted
date: 2026-11-12
scope: [ui/arca-ui, render, dev/standards]
superseded_by: null
negative_knowledge: true
---

# 0010. Synchronize with Optics v0.0.78 and Retire the Live-Canvas Host Path

- Status: Accepted
- Date: 2026-11-12
- Deciders: Arca Architecture Working Group
- Consulted: Optics/Lens/Iris Maintainers
- Amends: [ADR-0009](0009-synchronize-with-optics-v0.0.76-and-native-frame-damage.md)
- Extends: Optics ADR-0088 (immutable display lists), ADR-0089 (RenderPlan), ADR-0090 (owned resources), ADR-0124 (dual-thread presentation), ADR-0127 (decoupled host canvas recording), ADR-0157 (DisplayList wire protocol)

---

## Context and Problem Statement

Between `v0.0.76` and `v0.0.78`, Optics completed the decoupled presentation
migration and changed which host-canvas contracts exist:

1. **Recording is the only host canvas path (Optics ADR-0127 amendment)**:
   - `iris_paint_fn`, `iris_app_opts.paint`, the Rust `PaintHost` /
     `PaintCanvas` wrappers, and `iris_paint_mark_static` are deleted.
     Their record-based replacements, `iris_record_fn` and
     `iris_record_mark_static`, are the only spellings left; no deprecated
     forwarding aliases remain.
   - `Application::run_with_lifecycle` takes an optional `RecordHost`
     closure. Hosts record an immutable `flux_display_list` on the UI thread;
     Iris submits it under Lens chrome on the presentation worker.
   - The single-threaded synchronous paint fallback is gone; even the
     `prepare`-on-main-thread path submits the same recorded list, so every
     backend keeps the UI thread free of swapchain work.

2. **The activation-token alias is gone (bindings v0.0.78)**:
   - `iris_wayland_create_activation_token` is deleted.
     `iris_window_create_activation_token` is the only public entry point.

3. **Flux resource ownership became explicit (Optics ADR-0089, ADR-0090,
   ADR-0157)**:
   - RenderPlan is the frame execution IR, DisplayLists have a versioned wire
     format and dynamic image input slots, and images gained immutable
     snapshots with copy-on-write.
   - Imported dma-bufs now declare a borrowed or exclusive lease, and a list
     accounts for the image bytes it pins.
   - These contracts govern how Lens and Iris retain host images; they do not
     require a host to serialize or plan anything it does not own.

4. **Compatibility surfaces were removed across the stack**:
   - Prism's frost aliases, Glyph's single-page atlas view, and the
     `*_DEPRECATED` attribute macros are deleted. Pre-1.0 SONAMEs continue to
     carry the full patch version.

Arca is pure Lens chrome and never installed a paint callback, but the
workspace pinned `v0.0.76` and still named `iris::PaintHost` at both lifecycle
call sites. That spelling no longer compiles, and keeping a local alias would
hide the fact that host canvas content is now record-only.

---

## Decision Drivers

- Driver 1: **One Canvas Contract**: Arca must not retain a second,
  synchronous host-drawing path that no backend executes.
- Driver 2: **Thread Affinity**: All host content must be recorded on the UI
  thread and submitted by Iris; no Vulkan or swapchain work may leak onto the
  UI thread.
- Driver 3: **Explicit Image Ownership**: A host image that a published
  DisplayList retains must not be mutated behind the list's back.
- Driver 4: **Fail-Fast Packaging**: The full-version SONAME boundary remains
  stricter than a source-compatible alias and must not be papered over.
- Driver 5: **No Speculative Infrastructure**: A file manager that owns no
  display list and opens no custom canvas should not adopt serialization,
  RenderPlan management, or snapshot bookkeeping it cannot exercise.

---

## Architectural Decisions

### 1. Optics Dependency Floor Bump

All workspace dependencies for `iris`, `lens`, `lens-sys`, `flux`, `flux-sys`,
`transit`, and `transit-sys` are bumped to `v0.0.78`. The canonical
`Cargo.lock` records the tagged commit; local development continues to resolve
the sibling checkout through `.cargo/optics-local.toml`.

### 2. Arca Declares an Empty RecordHost

Both lifecycle entry points (`arca_ui::run` and `arca_ui::run_chooser`) pass
`None::<fn(iris::RecordHost)>` and build only Lens widgets. This is an
architectural statement, not a placeholder:

- Arca owns no canvas-sized document surface; directory listings, previews,
  and thumbnails are Lens widgets.
- Host image content is uploaded as a `flux_image` and drawn by Lens, so it
  already travels through Lens's display list rather than a host callback.
- Any future custom canvas must record through `flux::Encoder` into an
  immutable DisplayList; a live `flux_canvas` is never exposed to host code.

### 3. Thumbnail Images Stay Owned and Write-Once

`ThumbStore` creates each preview image, uploads its pixels once with
`flux_image_update_region_premultiply`, and releases it at shutdown. Because
the image is never updated after Lens retains it, no snapshot or dynamic
input slot is required; the write-once discipline keeps the retained handle
stable while a list references it:

- no snapshot is taken per thumbnail, so there is no extra COW allocation,
- an image update while a list retains it would require `flux_image_snapshot`
  first, or a dynamic image input slot bound at submit time,
- borrowed dma-buf imports are never treated as retained content.

### 4. Flux Wire and Plan APIs Are Not Adopted by the Application

DisplayList serialization, dynamic input slots, retained-content budgets, and
RenderPlan validation are Flux-level contracts used by Iris/Lens and by hosts
that persist or re-execute command streams. Arca neither persists rendered
commands nor opens its own frame, so it consumes none of those APIs directly.
For the same reason it does not call `iris_record_mark_static` or
`iris_request_frame_skip_render`: those declarations describe recorded host
canvas content, while Lens owns chrome damage and animation pacing. The
crate-level dependency stays on `flux-sys` only for image upload and
inspection.

### 5. Activation Tokens Use the Window API Only

Arca already called `iris::window_create_activation_token`; the deleted
`wayland_` alias has no call sites. This decision records the intended
portability boundary: activation tokens are a window concern, not a
Wayland-specific escape hatch.

---

## Consequences

### Positive

- Arca builds against the record-only Iris API without a compatibility shim.
- Chrome presentation stays on the same dual-thread pipeline as upstream
  Lens/Iris, with no UI-thread swapchain work.
- Thumbnail ownership remains simple: one allocation, one upload, one release,
  with no COW traffic for content that never changes.
- The application does not carry dead abstractions for display-list
  serialization or render-plan management it cannot use.

### Negative & Trade-offs

- Requires Optics `v0.0.78` or later; older installed libraries fail at load
  time through the full-version SONAME.
- A future custom canvas has no synchronous escape hatch. It must adopt the
  Encoder/DisplayList recording model, including the dynamic-slot and
  retained-budget contracts when it retains host images.

---

## Rejected Alternatives & Negative Knowledge

### 1. Keep a local `PaintHost` alias or a no-op paint closure

- **Why considered**: one-line migration at both call sites.
- **Why rejected**: the C callback and descriptor field are deleted, so an
  alias would answer only the compiler. Host canvas content must be recorded
  on the UI thread; preserving the old shape would invite code that assumes a
  live canvas that no backend provides.

### 2. Implement a RecordHost for thumbnails or previews

- **Why considered**: makes Arca use the new seam and could bypass Lens image
  widgets.
- **Why rejected**: Lens already composites host `flux_image`s and manages
  their damage and retention. A parallel recorder duplicates the pipeline and
  would need its own image-input bindings for no visible gain.

### 3. Snapshot every uploaded thumbnail

- **Why considered**: makes retention explicit and future-proof.
- **Why rejected**: snapshots exist for images that are updated while a list
  retains them. Arca's thumbnails are write-once, so snapshots would add an
  allocation and a second handle per preview without changing behavior.

### 4. Persist or serialize Lens display lists for tabs

- **Why considered**: DisplayList v1 offers a canonical wire format.
- **Why rejected**: tabs persist directory state, selection, and view mode,
  not rendered commands. Re-serializing widgets would freeze theme, layout,
  and damage decisions into stale bytes.

### 5. Stay on `v0.0.76` while the API still compiles

- **Why considered**: avoids touching both lifecycle call sites.
- **Why rejected**: `v0.0.76` bindings still expose `PaintHost`, so the
  project would be pinned to a deleted contract and could not consume the
  record-only fixes that shipped in the same release line.

---

## Links

- [ADR 0001: Optics Stack Migration](0001-optics-stack-migration.md)
- [ADR 0008: Synchronize with Optics v0.0.74](0008-synchronize-with-optics-v0.0.74-and-damage-quiescence.md)
- [ADR 0009: Synchronize with Optics v0.0.76](0009-synchronize-with-optics-v0.0.76-and-native-frame-damage.md)
- [Optics Migration History](../dev/optics-migration.md)
- Optics ADR-0088: Immutable Display Lists
- Optics ADR-0089: RenderPlan IR and Canvas Compilation
- Optics ADR-0090: Owned Resources and Immutable Snapshots
- Optics ADR-0124: Dual-Thread UI and Presentation Pipeline
- Optics ADR-0127: Decoupled Host Canvas Recording
- Optics ADR-0157: DisplayList Serialization Protocol
