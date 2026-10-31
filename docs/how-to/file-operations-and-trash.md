# How to Manage Files and Use Trash

This guide explains how to create, rename, copy, move, delete, and restore files
in Arca while complying with the FreeDesktop Trash specification.

---

## Create a New Directory

1. Open the folder where you want to create a new subfolder.
2. Press `Ctrl+Shift+N`.
3. An inline text entry field appears, pre-filled with `New Folder`.
4. Type the new folder name and press `Enter`.
5. If a folder with that name already exists, Arca automatically deduplicates the name:
   `New Folder`, then `New Folder 2`, `New Folder 3`, …

---

## Rename a File or Folder Inline

1. Select the item you wish to rename using the mouse or arrow keys.
2. Press `Ctrl+E` (or right-click and select **Rename**).
3. The row's label transforms into an inline editable text field containing the whole
   current name, ready to edit.
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
   suffix (`file (copy).txt`, `file (copy 2).txt`, …) rather than clobbering existing
   data.

Transfers of 8 MiB or more run on a background worker: the window keeps painting and
the status bar shows `Transferring <name>: NN% (copied/total)`. Copying or moving a
directory into itself (or into one of its own descendants) is refused with
`cannot copy or move '<path>' inside itself`.

### Drag and drop

Dragging a row onto a folder row, a sidebar bookmark, or a sidebar place moves the
selection there; hold `Ctrl` while dropping to copy instead. Dropping onto "Drop files here to add a bookmark" adds a bookmark
instead. Drags out of Arca publish a standard `text/uri-list`, and drops into Arca
accept `text/uri-list` and `x-special/gnome-copied-files` from other applications.

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

1. In the left sidebar, click **Trash**. This opens the trash listing
   (`$XDG_DATA_HOME/Trash/files`), not the bookkeeping root.
2. In List view the `Size` column shows the item's **original directory** and the
   `Modified` column shows its **deletion date**, both read from the companion
   `.trashinfo` record.
3. To restore one or more items:
   - Select them (click, `Ctrl`-click, or `Shift`-click for a range).
   - Right-click and choose **Restore** (or **Restore N items**).
   - Arca reads the `.trashinfo` record and moves each item back to its recorded
     original path, recreating missing parent directories.
4. To destroy items without restoring, right-click and choose
   **Delete permanently**. To clear everything, right-click the **Trash** entry in
   the sidebar and choose **Empty Trash**.

Deleting a symlink records the *link's* path, so restoring puts the link back where it
was instead of beside its target.

---

## Open Files with External Applications

1. Double-click any file (or select it and press `Enter`).
2. Arca queries the desktop MIME database (`mimeapps.list` and `.desktop` files)
   and spawns the default associated application.
3. If no desktop handler is found, Arca attempts to open the file with `xdg-open`.
