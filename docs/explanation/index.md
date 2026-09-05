# Explanation

Explanation documents provide conceptual deep dives into Arca's design philosophy,
architectural boundaries, and underlying mechanisms.

## Topics

- [Architecture and Virtualized Rendering](architecture-and-rendering.md) — How
  Arca isolates the business core from immediate-mode UI and bounds memory usage
  within the Lens 1 MiB arena.
- [Thumbnail Extraction and Caching Pipeline](thumbnail-pipeline.md) — Two-tier
  LRU memory and disk cache architecture, off-thread image decoding, and audio
  metadata extraction.
- [Portal Chooser Architecture and Isolation](portal-chooser-design.md) — Why
  Arca functions as an isolated, transient prompter process for XDG Desktop Portal.
