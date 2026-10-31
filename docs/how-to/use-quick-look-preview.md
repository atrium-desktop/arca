# How to Use Quick Look Preview

This guide explains how to quickly inspect images, audio tracks, directories,
and text files using Arca's built-in Quick Look previewer.

---

## Trigger and Dismiss Quick Look

1. Select any file or directory in the listing.
2. Press `Space` to display the Quick Look modal overlay.
3. The preview overlay animates into view over the current directory listing.
4. To dismiss Quick Look:
   - Press `Space` again.
   - Press `Esc`.
   - Click anywhere outside the preview card.

Quick Look is a dismissable modal layer: until it closes, `Esc` and outside clicks
belong to it, and only then fall through to the filter, the location bar or the
chooser.

---

## Inspect Images

When triggered on raster images (`.png`, `.jpeg`, `.jpg`):

- **Image Preview**: Arca decodes the image off-thread and renders it scaled to fit
  the preview viewport while preserving aspect ratio.
- **Metadata**: Quick Look displays pixel dimensions (width × height) and total file
  size.
- **Cache**: Decoded images are cached in an LRU memory buffer and on disk under
  `$XDG_CACHE_HOME/thumbnails/normal/` (the FreeDesktop Thumbnail Managing Standard
  location, shared with other desktop applications), ensuring instant re-inspection.

---

## Inspect Audio Metadata and Artwork

When triggered on audio files (`.mp3`, `.flac`):

- **Cover Art**: Quick Look parses ID3v2 APIC frames or FLAC `PICTURE` blocks to
  extract and display embedded album art.
- **File Specs**: Shows the resolved MIME type and file size.

Quick Look does not yet decode audio tags (title, artist, album, duration); only the
embedded artwork is extracted.

---

## Inspect Directories

When triggered on a directory:

- Quick Look calculates and displays the item count inside the directory (capped at
  "More than 10,000 items").
- Displays the last modification date.

---

## Inspect Text Files and Source Code

When triggered on plain text files (`.txt`, `.md`, `.rs`, `.toml`, `.json`):

- Quick Look verifies that file contents look like text (no NUL bytes, few control
  characters).
- Displays up to the first 20 KiB of the file in a scrollable pane, ending with
  `… Preview truncated` when the file is longer.
- Displays size, modification time and MIME type.

## Special files

Device nodes, FIFOs and sockets are described (`Named pipe`, `Socket`,
`Character device`, `Block device`) but never opened — reading them can block
indefinitely, so Quick Look refuses to read anything that is not a regular file.

---

## Continuous Preview with Keyboard Navigation

While the Quick Look overlay is open:

- Press `Down Arrow` or `Up Arrow` to move selection to the next or previous file in
  the underlying folder.
- Quick Look immediately updates to inspect the newly selected item without closing
  and reopening the preview window.
