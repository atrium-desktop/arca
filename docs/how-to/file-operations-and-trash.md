# How to Manage Files and Use Trash

This guide explains how to create, rename, copy, move, delete, and restore files
in Arca while complying with the FreeDesktop Trash specification.

---

## Create a New Directory

1. Open the folder where you want to create a new subfolder.
2. Press `Ctrl+Shift+N` (or right-click in empty space and select **New Folder**).
3. An inline text entry field appears with the placeholder `New Folder`.
4. Type the new folder name and press `Enter`.
5. If a folder with that name already exists, Arca automatically deduplicates the name
   (e.g., `New Folder (1)`).

---

## Rename a File or Folder Inline

1. Select the item you wish to rename using the mouse or arrow keys.
2. Press `Ctrl+E` (or right-click and select **Rename**).
3. The row's label transforms into an inline editable text field, with the filename
   stem pre-selected (excluding the file extension).
4. Edit the name.
5. Press `Enter` to commit the rename.
6. Press `Esc` or click outside the text field to cancel the rename without changes.
7. Note: Renaming refuses to silently overwrite existing files. If a conflict occurs,
   an error notification is shown.

---

## Copy, Cut, and Paste Files

Arca supports clipboard operations conforming to the XDG clipboard specification:

1. Select one or more files in the active view:
   - Click an item to select it.
   - Hold `Ctrl` and click to toggle individual items into the selection.
   - Hold `Shift` and click or use `Shift+Down Arrow` for contiguous range selection.
2. Press `Ctrl+C` to copy the selected items, or `Ctrl+X` to cut them.
3. Navigate to the destination directory (in the current tab or another tab).
4. Press `Ctrl+V` to paste.
5. If a file with identical name exists in the destination, Arca appends a numeric
   suffix (e.g., `file (copy 1).txt`) rather than clobbering existing data.

---

## Move Files to Trash

Arca follows the FreeDesktop.org Trash specification:

1. Select the items you want to delete.
2. Press `Delete` (or right-click and choose **Move to Trash**).
3. The items disappear from the directory view and move to `~/.local/share/Trash/files/`.
4. A corresponding `.trashinfo` metadata record is created in
   `~/.local/share/Trash/info/`, storing the original absolute path and deletion
   timestamp.

---

## Inspect and Restore Trashed Items

1. In the left sidebar, click **Trash** (or press `Ctrl+L` and enter `trash:///`).
2. The trash listing shows all deleted files, their original locations, and deletion dates.
3. To restore an item:
   - Select the file.
   - Right-click and choose **Restore**.
   - Arca reads the companion `.trashinfo` file and moves the file back to its
     original location on disk.
4. If the original parent directory was deleted, Arca recreates the directory
   structure before restoring the file.

---

## Open Files with External Applications

1. Double-click any file (or select it and press `Enter`).
2. Arca queries the desktop MIME database (`mimeapps.list` and `.desktop` files)
   and spawns the default associated application.
3. If no desktop handler is found, Arca attempts to open the file with `xdg-open`.
