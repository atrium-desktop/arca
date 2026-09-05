# How to Customize Appearance and Settings

This guide explains how to switch color themes, configure thumbnail generation,
set default listing modes, and manage configuration keys for Arca.

---

## Switch Color Themes

Arca natively follows the Wayland system color preference (light or dark), but you
can override this setting manually:

1. Click the **Settings** (gear) button on the right side of the toolbar.
2. An appearance menu opens with three choices:
   - **System** — Follows the Wayland compositor preference (e.g., via
     `xdg-desktop-portal` settings) live without restarting.
   - **Light** — Forces the Tessera light color scheme.
   - **Dark** — Forces the Tessera dark color scheme.
3. Select your preference. The UI recolors instantly across all open windows and
   tabs.
4. Your choice is automatically persisted to `~/.config/arca/arca.conf`.

---

## Toggle Thumbnail Generation

Thumbnails for images and music album art are generated off-thread to ensure 60+ FPS
scrolling. To toggle thumbnails:

1. Open `~/.config/arca/arca.conf` in your text editor.
2. Set `show_thumbnails`:
   ```ini
   show_thumbnails = false
   ```
3. When set to `false`, Arca displays lightweight generic MIME icons instead of
   decoding image pixels or audio tags. This is useful on low-power devices or
   metered storage.

---

## Set Default View Mode

To configure whether Arca opens folders in Grid, List, or Miller column view:

1. In the toolbar, click your preferred view button (**Grid**, **List**, or
   **Miller**).
2. Arca automatically remembers the last active view mode in `arca.conf`:
   ```ini
   view = miller
   ```
3. Alternatively, pass a flag when launching from the command line or desktop launcher:
   ```bash
   arca --grid ~/Pictures
   arca --list ~/Downloads
   arca --miller ~/Code
   ```

---

## Edit Configuration Manually

Arca stores configuration in `$XDG_CONFIG_HOME/arca/arca.conf` (defaults to
`~/.config/arca/arca.conf`).

The file uses standard `key = value` syntax:

```ini
# Show or hide hidden files (files starting with '.')
show_hidden = false

# Primary sort attribute: name | size | modified
sort = name

# Sort direction: true (ascending) | false (descending)
sort_ascending = true

# Theme mode: system | light | dark
theme = system

# Default view mode: grid | list | miller
view = list

# Off-thread thumbnail generation: true | false
show_thumbnails = true

# Pinned custom bookmarks
bookmark = /home/username/work/projects
bookmark = /mnt/data/storage
```

---

## Clear Thumbnail Cache

Decoded thumbnails are cached on disk under `$XDG_CACHE_HOME/arca/thumbnails/` (typically
`~/.cache/arca/thumbnails/`).

To purge thumbnail cache files and free disk space:

```bash
rm -rf ~/.cache/arca/thumbnails/*
```

Arca automatically regenerates thumbnails on demand when directories are revisited.
