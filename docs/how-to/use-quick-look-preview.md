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

---

## Inspect Images

When triggered on raster images (`.png`, `.jpeg`, `.jpg`):

- **Image Preview**: Arca decodes the image off-thread and renders it scaled to fit
  the preview viewport while preserving aspect ratio.
- **Metadata**: Quick Look displays pixel dimensions (width × height) and total file
  size.
- **Cache**: Decoded images are cached in an LRU memory buffer and on disk under
  `~/.cache/arca/thumbnails/`, ensuring instant re-inspection.

---

## Inspect Audio Metadata and Artwork

When triggered on audio files (`.mp3`, `.flac`):

- **Cover Art**: Quick Look parses ID3v2 APIC frames or FLAC `PICTURE` blocks to
  extract and display embedded album art.
- **Track Details**: Displays the embedded track title, artist, album name, and duration
  when available.
- **File Specs**: Shows audio format and file size.

---

## Inspect Directories

When triggered on a directory:

- Quick Look calculates and displays the item count inside the directory (number of
  subfolders and files).
- Displays last modification date and filesystem permissions.

---

## Inspect Text Files and Source Code

When triggered on plain text files (`.txt`, `.md`, `.rs`, `.toml`, `.json`):

- Quick Look verifies that file contents are valid UTF-8 and contain no binary null
  bytes.
- Displays the first lines of text in a clean monospace font with bounded height.
- Displays line count and file encoding.

---

## Continuous Preview with Keyboard Navigation

While the Quick Look overlay is open:

- Press `Down Arrow` or `Up Arrow` to move selection to the next or previous file in
  the underlying folder.
- Quick Look immediately updates to inspect the newly selected item without closing
  and reopening the preview window.
