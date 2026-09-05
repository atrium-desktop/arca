# Getting Started with Arca

In this tutorial, you will launch Arca, navigate directories using both the
mouse and the keyboard, switch between different view modes, open and close
tabs, and preview files with Quick Look.

---

## Prerequisites

Before beginning, ensure you have:
- A running Wayland desktop session (such as Tessera, Sway, or GNOME Wayland).
- The `arca` binary built or installed on your system.

---

## Step 1: Launch Arca

Open your terminal and run:

```bash
arca
```

You will see the Arca window appear on your display. The window contains:
- A top toolbar with navigation buttons (Back, Forward, Up), view mode toggles,
  and a settings menu.
- A path breadcrumb bar indicating your current working directory.
- A left sidebar displaying standard bookmarks (Home, Documents, Downloads,
  Music, Pictures, Videos, and Trash).
- A central listing displaying files and directories in your home folder.

---

## Step 2: Navigate with Keyboard Shortcuts

Arca is optimized for fast, keyboard-first navigation.

1. Press `Down Arrow` or `Up Arrow` to move the selection across files in the
   listing. Notice the active selection updates immediately.
2. Highlight any subdirectory and press `Enter`. Arca opens that directory.
   The breadcrumb bar at the top updates to display the new path.
3. Press `Backspace` (or `Alt+Up Arrow`) to return to the parent directory.
4. Press `Alt+Left Arrow` to navigate backward in your history, and
   `Alt+Right Arrow` to move forward again.

---

## Step 3: Switch View Modes

Arca provides three distinct viewing styles suited for different workflows:

1. Look at the view switcher buttons in the toolbar.
2. Click the **Grid** button (or launch with `arca --grid`). Files now appear as
   large icons with thumbnails. Use `Left Arrow` and `Right Arrow` to move
   horizontally across the grid.
3. Click the **List** button. Files now appear in a compact table displaying
   file name, size, and last modified date.
4. Click the **Miller** button (or launch with `arca --miller`). Directories now
   appear in cascading horizontal columns. Press `Right Arrow` to expand child
   directories into adjacent columns while keeping your full ancestry path
   visible.

---

## Step 4: Work with Tabs

You can manage multiple directories simultaneously using tabs:

1. Press `Ctrl+T` to open a new tab. A second tab appears in the tab strip,
   cloning your current location.
2. In the new tab, navigate to another folder (for example, `Downloads`).
3. Press `Ctrl+Tab` to switch back to your first tab.
4. Press `Ctrl+Shift+Tab` to return to the second tab.
5. When finished, press `Ctrl+W` to close the active tab.

---

## Step 5: Inspect Files with Quick Look

Arca lets you inspect files without launching heavy external applications:

1. Select an image file (`.png` or `.jpeg`).
2. Press `Space`. An animated Quick Look preview opens, displaying a high-resolution
   rendering of the image alongside dimensions and file size.
3. Select an audio file (`.mp3` or `.flac`) and press `Space`. Quick Look extracts
   and displays the embedded album artwork, artist name, and track title.
4. Press `Space` again to close the preview overlay.

---

## Step 6: Filter and Find Files

1. Press `Ctrl+F` while viewing any folder. The search filter field receives focus.
2. Type part of a filename (such as `doc` or `.rs`).
3. The listing updates in real time, showing only items that match your query.
4. Press `Esc` to clear the filter and return to the complete directory listing.

---

## What's Next?

Congratulations! You have mastered the fundamentals of Arca. To dive deeper into
specific workflows and features, explore:

- [How to Navigate and Browse](../how-to/navigate-and-browse.md)
- [How to Manage Files and Use Trash](../how-to/file-operations-and-trash.md)
- [How to Customize Appearance and Settings](../how-to/customize-appearance-and-settings.md)
- [Configuration Reference](../reference/configuration.md)
