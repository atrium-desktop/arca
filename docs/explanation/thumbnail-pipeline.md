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
| (GPU / RGBA LRU) |              |  (~/.cache/arca/)  |
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

### 2. On-Disk Cache
When a thumbnail is successfully decoded, a downscaled PNG representation is saved to
the user cache directory (`$XDG_CACHE_HOME/arca/thumbnails/`).
- The cache key incorporates the absolute path of the source file, file size, and
  last modification timestamp (`mtime`).
- If the source file is modified or replaced, the modification timestamp changes,
  invalidating the stale disk cache entry.
- When navigating back to a previously visited folder, Arca loads the pre-scaled
  cache files, bypassing expensive full-resolution image decoding.

---

## Audio Metadata Extraction

Audio files often embed cover art in metadata headers:
- **FLAC Files**: The extractor parses the FLAC metadata block stream, locates block
  type 6 (`PICTURE`), extracts MIME headers, and decodes the embedded image bytes.
- **MP3 Files**: The extractor parses ID3v2 frames, searches for `APIC` (Attached
  Picture) frames, and extracts the embedded JPEG or PNG cover.

If an audio file lacks embedded artwork, Arca marks the item with a negative cache
tombstone to prevent repeated unproductive file header scans.
