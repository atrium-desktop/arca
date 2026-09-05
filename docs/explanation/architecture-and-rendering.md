# Architecture and Virtualized Rendering

This document explains the conceptual architecture of Arca and how it achieves
bounded memory consumption and consistent 60+ FPS rendering on Wayland.

---

## Workspace Decoupling

Arca separates domain business logic from immediate-mode UI rendering across three
distinct tiers:

```text
+--------------------------------------------------------------+
|                         arca (CLI)                           |
|  Entrypoint, argument parsing, portal prompter dispatch     |
+--------------------------------------------------------------+
                               |
            +------------------+------------------+
            |                                     |
            v                                     v
+-----------------------+             +------------------------+
|      arca-core        |             |        arca-ui         |
|  Filesystem traversal |             | Immediate-mode UI loop |
|  Trash specification  |<------------| Layout & virtual rows  |
|  LRU thumbnail cache  |  AppState   | Lens / Iris Wayland    |
|  Config persistence   |             | GPU texture management |
+-----------------------+             +------------------------+
```

1. **Zero-GUI Core**:
   The core crate contains pure business logic. It has no dependencies on Wayland,
   Vulkan, or graphical widget frameworks. This allows all filesystem algorithms,
   sorting rules, trash metadata parsing, and path canonicalization to be verified
   rapidly in headless unit tests.

2. **Immediate-Mode UI**:
   The user interface crate renders an immutable view of the application state
   on every frame. Rather than mutating a persistent tree of stateful UI widgets,
   the view function constructs layout primitives afresh per frame based on
   user input and filesystem events.

---

## The Immediate-Mode Frame Budget

Arca is built on the Optics graphics engine (`flux` for Vulkan drawing, `lens` for
immediate-mode layout, and `iris` for Wayland event handling).

Lens allocates per-frame widget layouts, text shapes, and vertex draw calls inside
a pre-allocated linear memory arena sized at 1 MiB. If an application attempts to
allocate more widgets than the arena can hold in a single frame, Lens drops subsequent
draw calls, resulting in blank icons, clipped labels, or missing rows.

In a file manager, directories often contain tens of thousands of items (such as
`/usr/bin` or software repositories). Generating UI nodes for every item would
instantly exhaust the 1 MiB arena budget.

---

## Viewport Virtualization

To maintain fluid scrolling and guarantee that frame allocations remain constant
regardless of folder size, all listing views in Arca use **viewport virtualization**:

```text
Full Directory (10,000 files)
+------------------------------------+
| row 0000: file-0000.txt            | (unrendered - skipped)
| ...                                |
+====================================+ <--- Viewport Top Offset
| row 0142: file-0142.txt            | [ Rendered Window ]
| row 0143: file-0143.txt            | [ Only ~30 rows   ]
| row 0144: file-0144.txt            | [ generated into  ]
| row 0145: file-0145.txt            | [ Lens arena      ]
+====================================+ <--- Viewport Bottom Offset
| ...                                |
| row 9999: file-9999.txt            | (unrendered - skipped)
+------------------------------------+
```

### Calculation Mechanism

1. **Uniform Row Height**: Each presentation mode (List, Grid, Miller) calculates a
   deterministic row or tile height.
2. **Scroll Offset Query**: The rendering loop queries the current vertical scroll
   offset from the layout container.
3. **Index Range Slicing**: The UI calculates the first visible index (`offset / row_height`)
   and the visible count (`viewport_height / row_height + overscan`).
4. **Bounded Generation**: Draw calls and widget nodes are constructed only for the
   visible range. The scroll container receives dummy spacing before and after the
   visible window to preserve the accurate scrollbar thumb geometry.

As a result, viewing a folder with 50,000 files consumes identical frame memory and
render time to viewing a folder with 50 files.
