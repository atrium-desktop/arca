# Thumbnail Extraction and Caching Pipeline

This document explains the conceptual design of Arca's off-thread thumbnail
extraction pipeline, two-tier cache hierarchy, and asynchronous GPU texture upload.

---

## Design Goals

- **Zero Main-Thread Blocking**: Decoding 4K images or extracting audio cover art
  must never stall the Wayland event loop or cause frame drops.
- **Bounded Memory**: High-resolution decoded pixel buffers must not cause memory leaks
  or unbounded RAM growth when browsing photo directories.
- **Cache Persistence**: Previously inspected thumbnails must load instantaneously
  across application restarts without re-reading image files from disk.

---

## Architectural Pipeline

The thumbnail system is split across two domains: worker extraction in the core
layer, and texture ownership in the UI layer.

```text
[ Viewport Request ]
         |
         v
+------------------+     Miss     +--------------------+
| In-Memory Cache  | ------------>|  Disk Cache Check  |
| (GPU / RGBA LRU) |              | (thumbnails/normal)|
+------------------+              +--------------------+
         | Hit                              |
         |                                  | Miss
         v                                  v
[ Immediate Render ]              +--------------------+
                                  | Background Worker  |
                                  | (Off-Thread Queue) |
                                  +--------------------+
                                            |
                                            | Decode PNG/JPEG/Tags
                                            v
                                  +--------------------+
                                  | Write Cache File   |
                                  | & Upload to GPU    |
                                  +--------------------+
```

---

## Two-Tier Cache Model

### 1. In-Memory LRU Cache
Decoded pixel buffers ready for GPU upload reside in an in-memory Least Recently Used
(LRU) cache bounded by both total entry count and maximum byte capacity. When the
cache reaches capacity, older items are evicted automatically.

### 2. On-Disk Cache (Thumbnail Managing Standard)
When a thumbnail is successfully decoded, a downscaled PNG representation is saved to
the shared FreeDesktop cache directory (`$XDG_CACHE_HOME/thumbnails/normal/`). The
directory is shared with every other desktop application, so Arca follows the
[Thumbnail Managing Standard](https://specifications.freedesktop.org/thumbnail-spec/latest/)
exactly (implemented in `arca_xdg::thumbnail`):
- **Name**: `MD5(URI)` + `.png`, where the URI is the absolute, lexically normalized
  path escaped the way GLib's `g_filename_to_uri` does. Any other escaping would hash
  `Song (Live).flac` differently from Nautilus and fork the cache. Symlinks are not
  resolved, matching GIO.
- **Metadata**: every entry carries `tEXt` chunks `Thumb::URI` and `Thumb::MTime`
  (mandatory), plus `Thumb::Size`, `Thumb::Mimetype` and `Software`.
- **Validity**: an entry is used only when its `Thumb::URI` equals the source URI and
  its `Thumb::MTime` equals the source mtime in whole seconds. The PNG file's own
  mtime is ignored. Entries written by other conforming applications are reused.
- **Writing**: a `0600` temporary file in the target directory (`0700`) is renamed
  into place, so readers never see a partial PNG.
- **No recursion**: files inside `$XDG_CACHE_HOME/thumbnails/` are decoded for display
  but never cached.
- **No pruning**: the cache belongs to the desktop, so Arca never deletes entries.
  Desktop environments and `rm` (see *Customize Appearance and Settings*) handle cleanup.
- When navigating back to a previously visited folder, Arca loads the pre-scaled
  cache files, bypassing expensive full-resolution image decoding.

### Request ordering
Only cards in the virtualized visible window request thumbnails. Workers take the
**newest** queued request first, because during a scroll the newest requests are the
rows now in view. When more than 256 requests are pending, the oldest is dropped.

---

## Audio Metadata Extraction

Audio files often embed cover art in metadata headers:
- **FLAC Files**: The extractor parses the FLAC metadata block stream, locates block
  type 6 (`PICTURE`), extracts MIME headers, and decodes the embedded image bytes.
- **MP3 Files**: The extractor parses ID3v2 frames, searches for `APIC` (Attached
  Picture) frames, and extracts the embedded JPEG or PNG cover.

If an audio file lacks embedded artwork, Arca marks the item with a negative cache
tombstone to prevent repeated unproductive file header scans.
