# How to Navigate and Browse Directories

This guide explains how to jump to arbitrary paths, traverse folder hierarchies,
filter visible items, and organize bookmarks in Arca.

---

## Jump to an Exact Path

To open any folder directly by path:

1. Press `Ctrl+L` (or click the breadcrumb bar). The path field becomes an editable
   text box with the current path selected.
2. Type or paste the destination path (for example, `/etc/xdg` or `~/projects`).
   Tilde (`~`) expands automatically to your home directory.
3. Press `Enter` to navigate to the location.
4. Press `Esc` while editing to cancel and restore the original path view.

---

## Traverse History and Parent Folders

Use navigation keys to move through your visited locations:

- **Up to Parent**: Press `Backspace` or `Alt+Up Arrow` to move up to the enclosing
  directory.
- **Back in History**: Press `Alt+Left Arrow` to return to previously visited locations.
- **Forward in History**: Press `Alt+Right Arrow` to advance forward after moving
  backward.
- **Reload Directory**: Press `Ctrl+R` to refresh the current directory listing from
  disk.

---

## Filter Visible Files in Real Time

To locate specific files in large directories without scrolling:

1. Press `Ctrl+F` to focus the filter field in the toolbar.
2. Type a substring (for example, `config` or `.png`). Filtering is case-insensitive.
3. The listing immediately narrows to items matching the query. Subdirectories
   containing matching items remain accessible.
4. Press `Down Arrow` or `Up Arrow` to move selection directly into the filtered
   results.
5. Press `Esc` or click the clear icon to remove the filter and show all files.

---

## Toggle Hidden Dotfiles

By default, files and directories beginning with a dot (`.`) are hidden.

- Press `Ctrl+H` to toggle visibility of hidden files.
- The setting persists across sessions in `~/.config/arca/arca.conf`.

---

## Manage Sidebar Bookmarks

The sidebar provides immediate access to standard XDG user directories and custom
pins.

### Add a Custom Bookmark

1. Navigate to the directory you want to bookmark.
2. In the breadcrumb bar or context menu, select **Bookmark Location**.
3. The folder appears under the **Bookmarks** section in the left sidebar.

### Remove a Bookmark

1. Right-click the custom bookmark in the sidebar.
2. Select **Remove Bookmark**.
3. Note: Built-in system locations (Home, Documents, Downloads, Trash) cannot be
   removed from the sidebar.

---

## Work with Tabs

Keep multiple workspaces open side-by-side using tabs:

1. Press `Ctrl+T` to open a new tab.
2. Press `Ctrl+Tab` to cycle forward through open tabs.
3. Press `Ctrl+Shift+Tab` to cycle backward.
4. Press `Ctrl+1` through `Ctrl+9` to jump directly to tab 1 through 9.
5. Press `Ctrl+W` to close the active tab. Closing the last remaining tab exits
   the application.
