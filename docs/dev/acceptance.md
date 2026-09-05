# Acceptance

This document defines the real, end-to-end delivery acceptance procedure for Arca.

It answers: **Can an authentic user launch, configure, and complete core file
management journeys and critical acceptance scenarios through standard product
interfaces on Wayland?**

---

## Scope & Prerequisites

- **Scope**: Core user workflows (file navigation, view modes, tab management,
  file operations, trash/restore, Quick Look preview, and portal chooser integration)
  and deliverable criteria for production releases.
- **Prerequisites**:
  - A running Wayland compositor (e.g., Tessera, Sway, GNOME Wayland).
  - Optics runtime libraries installed or linked (`flux`, `lens`, `iris`).
  - Vulkan-capable graphics drivers and device permissions.
  - A test directory with standard files (text files, images, music files with embedded
    artwork, subdirectories).

---

## Launch & User Entry Point

1. Start Arca from a terminal or application launcher:
   ```bash
   arca
   ```
2. Alternatively, specify an initial directory and view mode:
   ```bash
   arca --miller ~/Documents
   ```
3. The Arca window appears on the active Wayland output, displaying the toolbar,
   sidebar bookmarks, location bar, and file listing.

---

## Core User Journeys

### Journey 1: Directory Browsing and View Switching

#### Steps
1. Launch `arca ~/Pictures`.
2. Inspect the file listing. Click the **Grid** button on the toolbar (or verify
   thumbnail view).
3. Switch to **List** view using the toolbar button. Verify sort columns (Name,
   Size, Modified).
4. Click the **Name** header to toggle ascending/descending order.
5. Switch to **Miller** columns view. Press `Right Arrow` to expand a subdirectory
   and `Left Arrow` to navigate back up.
6. Press `Ctrl+H` to toggle visibility of hidden dotfiles.

#### Expected Outcome
- Directory entries render fluidly in all three modes.
- Hidden files appear and disappear instantaneously without layout glitches.
- Miller columns track the active breadcrumb path.

#### Pass / Fail Criteria
- **Pass**: All three view modes transition without stutter; keyboard arrows navigate
  fluidly.
- **Fail**: Blank entries, crash on view switch, or failure to render column contents.

---

### Journey 2: File Management, Trash, and Undo/Restore

#### Steps
1. Navigate to a scratch directory (e.g. `~/tmp/test-arca`).
2. Press `Ctrl+Shift+N` to create a new folder. An inline edit field appears.
3. Type `work-archive` and press `Enter`.
4. Press `Ctrl+E` to rename the folder to `work-projects`. Press `Enter`.
5. Select an unwanted file and press `Delete`.
6. Navigate to Trash via sidebar or `~/.local/share/Trash`.
7. Right-click the trashed file and select **Restore** (or verify metadata file
  preservation per FreeDesktop Trash specification).

#### Expected Outcome
- New folders are created instantly; inline renames commit cleanly on `Enter`.
- Trashed items move into `~/.local/share/Trash/files` with companion `.trashinfo` metadata.
- Restored items return to their original parent path.

#### Pass / Fail Criteria
- **Pass**: File operations reflect immediately in the filesystem and UI view; zero data corruption.
- **Fail**: Failed rename, unhandled filesystem error, or broken trash info.

---

### Journey 3: Quick Look Inspection

#### Steps
1. Navigate to a folder containing a `.png` image, a `.flac` or `.mp3` audio file,
   and a text file.
2. Select the image and press `Space`.
3. Quick Look popup opens, displaying image resolution, file size, and decoded thumbnail.
4. Press `Down Arrow` to select the audio file.
5. Quick Look updates, displaying track metadata (artist, title) and embedded cover art.
6. Press `Down Arrow` to select the text file. Quick Look displays text preview.
7. Press `Space` or click outside to dismiss Quick Look.

#### Expected Outcome
- Quick Look appears smoothly, extracts preview information off-thread, and caches thumbnails.

#### Pass / Fail Criteria
- **Pass**: Previews load within 200ms; UI remains completely responsive during decoding.
- **Fail**: UI freezes during thumbnail decoding, or popup fails to dismiss.

---

### Journey 4: Portal Chooser / Prompter Integration

#### Steps
1. Invoke Arca in standalone picker mode:
   ```bash
   arca --choose-file --title "Select Document" ~/Documents
   ```
2. The picker dialog opens modally. Select a file and click **Open** (or press `Enter`).
3. Verify the chosen absolute path is printed to standard output and the process exits with code 0.
4. Run in prompter mode via JSON pipe:
   ```bash
   echo '{"method":"open_file","title":"Open","directory":"/tmp","multiple":false}' | arca --chooser-prompt
   ```
5. Choose a file or cancel.

#### Expected Outcome
- Standard output receives valid JSON response `{"status":"success","uris":[...]}` or `{"status":"cancelled"}`.

#### Pass / Fail Criteria
- **Pass**: Correct JSON response is emitted; process terminates immediately with zero zombie threads.
- **Fail**: Hang on exit, malformed JSON output, or unhandled pipe errors.

---

## Acceptance Scenario Matrix

| Scenario / State | Verification Path / Trigger | Expected Result | Status |
|------------------|-----------------------------|-----------------|--------|
| Normal Execution | Follow Journey 1 steps | Smooth rendering across Grid, List, Miller views | Pass |
| Large Directory (10k items) | Run `arca /usr/bin` (or large generated directory) | Virtualized listing renders without frame arena overflow or FPS drops | Pass |
| Empty Directory | Open empty folder (`mkdir /tmp/empty && arca /tmp/empty`) | Clean empty state banner displayed without errors | Pass |
| Read-Only Filesystem | Open read-only mount (`/proc` or `/sys`) | New folder / Delete / Rename actions are disabled or report graceful error | Pass |
| Special Characters / Unicode | Browse folder with files like `日本語 文件 📁.tar.gz` | File names render accurately without clipping or encoding corruption | Pass |
| Quick Look Cache Hit | Re-open Quick Look on previously inspected 4K image | Instant preview load from LRU disk/memory cache | Pass |
| Chooser Cancel | Invoke `arca --choose-file`, press `Esc` | Process exits with code 1 and no stdout path | Pass |

---

## Final Acceptance Checklist

- [ ] Core journeys execute from cold start without manual intervention or test flags.
- [ ] Lens 1 MiB per-frame widget arena limit is respected under large directory stress.
- [ ] FreeDesktop specifications (Trash, XDG Base Directory, Portal) pass contract verification.
- [ ] Light / Dark live theme updates immediately when compositor settings toggle.
- [ ] All observable deliverables meet production performance and usability criteria.
