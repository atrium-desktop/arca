# Keyboard Shortcuts Reference

This document details all keyboard shortcuts and navigation controls available
in Arca.

---

## Global and Application Controls

| Shortcut | Context | Action |
|----------|---------|--------|
| `Esc` | Global | Reserved by `iris` runtime to quit the application (or cancel current modal/filter). |
| `Ctrl+R` | Global | Refresh current directory listing from disk. |
| `Ctrl+H` | Global | Toggle visibility of hidden files and folders. |

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
| `Ctrl+F` | Global | Focus filter field to search visible files. |

---

## Tabs

| Shortcut | Context | Action |
|----------|---------|--------|
| `Ctrl+T` | Global | Open a new tab cloning the current location. |
| `Ctrl+W` | Global | Close the active tab (quits app if last tab). |
| `Ctrl+Tab` | Global | Switch to next tab. |
| `Ctrl+Shift+Tab` | Global | Switch to previous tab. |
| `Ctrl+1` … `Ctrl+9` | Global | Switch directly to tab 1 through 9. |

---

## File Operations

| Shortcut | Context | Action |
|----------|---------|--------|
| `Ctrl+Shift+N` | Listing | Create a new folder with inline rename. |
| `Ctrl+E` | Listing | Inline rename currently selected item. |
| `Delete` | Listing | Move selected items to Trash. |
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

## Text Field Focus Rule

While any text entry field has active focus (such as the location bar, filter
field, or inline rename):
- Alphanumeric keys and editing shortcuts belong strictly to the text widget.
- `Enter` commits the value.
- `Esc` cancels or dismisses focus.
- Application-level navigation shortcuts resume only when focus returns to the
  listing.
