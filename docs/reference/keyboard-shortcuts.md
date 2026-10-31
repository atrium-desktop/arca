# Keyboard Shortcuts Reference

This document details all keyboard shortcuts and navigation controls available
in Arca.

---

## Global and Application Controls

| Shortcut | Context | Action |
|----------|---------|--------|
| `Esc` | Global | Dismiss the innermost layer: inline rename → focused text field → Quick Look → running transfer (cancels it) → chooser. |
| `Ctrl+R` / `F5` | Global | Refresh current directory listing from disk. |
| `Ctrl+H` | Global | Toggle visibility of hidden files and folders. |
| `Ctrl+Z` | Global | Undo the last reversible file operation. |
| `Ctrl+Shift+Z` / `Ctrl+Y` | Global | Redo the last undone file operation. |
| `Ctrl+A` | Listing | Select every visible item. |
| `F11` | Global | Toggle fullscreen. |
| `Ctrl+Q` | Global | Close the window. |

---

## Directory Navigation

| Shortcut | Context | Action |
|----------|---------|--------|
| `Up Arrow` / `Down Arrow` | Listing | Move selection up or down. |
| `Left Arrow` / `Right Arrow` | Grid | Move selection horizontally across grid items. |
| `Left Arrow` | Miller columns | Navigate to parent column. |
| `Right Arrow` | Miller columns | Expand selected directory into child column. |
| `Enter` | Listing | Open selected directory or launch default application for file. |
| `Backspace` | Listing | Navigate up to parent directory. |
| `Alt+Up Arrow` | Listing | Navigate up to parent directory. |
| `Alt+Left Arrow` | Global | Navigate backward in directory history. |
| `Alt+Right Arrow` | Global | Navigate forward in directory history. |
| `Ctrl+L` | Global | Focus location breadcrumb bar for path entry. |
| `Ctrl+F` / `F3` | Global | Focus filter field to search visible files. |

---

## Tabs

| Shortcut | Context | Action |
|----------|---------|--------|
| `Ctrl+T` | Global | Open a new tab cloning the current location. |
| `Ctrl+W` | Global | Close the active tab. Closing the last tab closes the window. |
| `Ctrl+Tab` | Global | Switch to next tab. |
| `Ctrl+Shift+Tab` | Global | Switch to previous tab. |
| `Ctrl+1` … `Ctrl+9` | Global | Switch directly to tab 1 through 9. |

---

## File Operations

| Shortcut | Context | Action |
|----------|---------|--------|
| `Ctrl+Shift+N` | Listing | Create a new folder with inline rename. |
| `Ctrl+N` | Listing | Create a new folder with inline rename (alias). |
| `Ctrl+E` / `F2` | Listing | Inline rename currently selected item. |
| `Delete` | Listing | Move selected items to Trash. |
| `Shift+Delete` | Listing | Delete selected items permanently, bypassing Trash. |
| `Ctrl+C` | Listing | Copy selected items to clipboard. |
| `Ctrl+X` | Listing | Cut selected items to clipboard. |
| `Ctrl+V` | Listing | Paste items from clipboard into current folder. |

---

## Quick Look Preview

| Shortcut | Context | Action |
|----------|---------|--------|
| `Space` | Listing | Open or close Quick Look preview overlay. |
| `Up Arrow` / `Down Arrow` | Quick Look open | Preview previous or next file in folder continuously. |

---

## Context Menus

| Shortcut | Context | Action |
|----------|---------|--------|
| `Right-click` (row) | Listing | Context menu: Open, Quick Look, Open in New Tab, bookmark toggle, Rename, Copy, Cut, Paste, Copy Path, Open With, Move to Trash. |
| `Right-click` (sidebar) | Sidebar | Context menu: Open in New Tab, Copy Path, Remove from Bookmarks, Empty Trash. |

In a Trash listing the last group becomes **Restore** and **Delete permanently**;
"Move to Trash" is never offered there.

---

## Text Field Focus Rule

While any text entry field has active focus (such as the location bar, filter
field, or inline rename):
- Alphanumeric keys and editing shortcuts belong strictly to the text widget.
- `Enter` commits the value.
- `Esc` cancels or dismisses focus.
- Application-level navigation shortcuts resume only when focus returns to the
  listing.
