# Architecture and Virtualized Rendering

This document explains the conceptual architecture of Arca and how it achieves
bounded memory consumption and consistent 60+ FPS rendering on Wayland.

---

## Workspace Decoupling

Arca organizes domain business logic, UI rendering, and specification conformance across four concentric layers:

```text
┌──────────────────────────────────────────────────────────────┐
│                         arca (CLI)                           │
│  Entrypoint, argument parsing, portal prompter dispatch     │
└──────────────────────────────┬───────────────────────────────┘
                               │
            ┌──────────────────┴──────────────────┐
            │                                     │
            ▼                                     ▼
┌───────────────────────┐             ┌────────────────────────┐
│      arca-ui          │             │      arca-engine       │
│ Immediate-mode UI loop│             │ Filesystem traversal   │
│ Layout & virtual rows │◄────────────┤ Session state machine  │
│ Lens / Iris Wayland   │  AppState   │ LRU thumbnail cache    │
│ GPU texture management│             │ Config persistence     │
└───────────────────────┘             └───────────┬────────────┘
                                                  │
                                                  ▼
                                      ┌────────────────────────┐
                                      │       arca-xdg         │
                                      │ Pure-std XDG base/user │
                                      │ MIME, trash v1.0, URIs │
                                      └────────────────────────┘
```

1. **Zero-Dependency Specifications (`arca-xdg`)**:
   Pure-standard-library implementation of Freedesktop.org specifications: Base Directories, User Directories, RFC 8089 File URIs, Shared MIME-info with content sniffing, Desktop Entry & `mimeapps.list`, and FreeDesktop Trash v1.0. Strictly zero external dependencies.

2. **Zero-GUI Engine (`arca-engine`)**:
   The engine crate contains pure business logic and session state machines. It has no dependencies on Wayland, Vulkan, or graphical widget frameworks. This allows all filesystem algorithms, sorting rules, trash metadata parsing, and path canonicalization to be verified rapidly in headless unit tests.

3. **Immediate-Mode UI (`arca-ui`)**:
   The user interface crate renders an immutable view of the application state on every frame. Rather than mutating a persistent tree of stateful UI widgets, the view function constructs layout primitives afresh per frame based on user input and filesystem events.

4. **Unified Binary (`arca`)**:
   A thin orchestrator binary that routes CLI flags, standalone picker commands, and XDG desktop portal prompter loops.

---

## The Immediate-Mode Frame Budget

Arca is built on the Optics graphics engine (`flux` for Vulkan drawing, `lens` for
immediate-mode layout, `iris` for Wayland event handling, and `aperture` for the
application shell that binds the Lens UI to the Iris window).

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

---

## Selection Identity: Names, Not Indices

Selection is stored as a set of entry **names**. The visible list is rebuilt by every
directory read, sort toggle, hidden-file toggle and filter edit, so an index kept
across a rebuild would point at a different file — and the next `Delete` would trash
the wrong entry.

```text
selected_names: { "photos/2024", "report.pdf" }   ← authoritative
        │
        │  resync_selection(visible)   (after every entries/filter/sort change)
        ▼
selected_indices: { 3, 17 }                        ← derived cache, never written directly
cursor_name ──► selected (index)
anchor_name ──► anchor (index)
```

Two invariants hold by construction:

1. The selection is always a **subset of the visible set** — names hidden by the
   filter are dropped, because every file operation resolves through the same
   `visible()` list the user is looking at.
2. The **cursor is always a member of the selection** — there is no "focused but
   unselected" state in which `selected()` and `selected_entries()` could disagree.

---

## File Transfer: One Payload, One Code Path

Clipboard export, clipboard import and drag-and-drop all use
`arca_xdg::FileClipboard`, the XDG interchange type:

| Direction | Format |
| :--- | :--- |
| `Ctrl+C` / `Ctrl+X` → system clipboard | `x-special/gnome-copied-files` (carries the `copy`/`cut` verb) |
| Drag source → compositor and other apps | `text/uri-list` (RFC 2483) |
| System clipboard / external drop → Arca | both of the above, plus bare absolute paths |

The *action* travels either as the gnome-copied-files verb or as the caller-supplied
default for formats that cannot encode one. Paste and drop both funnel into a single
`transfer_into` routine, so collision naming, the "cannot move a directory into
itself" guard, and undo grouping are identical regardless of how the items arrived.

Transfers at or above 8 MiB are handed to the background IO worker; the completion
event commits one undo transaction and refreshes the listing.
