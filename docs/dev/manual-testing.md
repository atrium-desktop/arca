# Manual Testing & Full-Stack Verification Guide

This document defines the comprehensive manual testing procedures, exploratory testing playbooks, and release verification runbooks for **Arca** — the fast, keyboard-first file manager and portal file chooser for Wayland.

It ensures that developers, QA engineers, and release gatekeepers can verify end-to-end functionality, user experience ergonomics, interactive approval workflows, and fault recovery from a clean cold start without synthetic test harnesses or internal backdoor bypasses.

---

## 1. Principles & Testing Philosophy

Manual verification complements automated testing (`cargo test --workspace`, headless frame tests) by focusing on cognitive, perceptual, and windowing dimensions that machines cannot evaluate:

1. **`[INV-VAL-01]` Real User Journey Parity**:
   All manual verifications must be performed through the public unified CLI interface (`arca`) or standard portal JSON stdio streams from a cold start.
2. **`[INV-CLI-01]` Unified Binary Singleton**:
   The workspace produces a single public executable: `arca`. Interactive file management, standalone CLI picking, and portal prompter loops are all executed through this single entrypoint.
3. **Ergonomics & Visual Polish**:
   Validate Wayland layout responsiveness, borderless chrome hierarchy, SVG icon clarity, CJK file name rendering, theme switching (System/Light/Dark), and spring/cubic easing animations powered by Optics (`lens`, `flux`, `transit`).
4. **Host Isolation Invariant**:
   Manual test runs must never pollute the operator's host configuration (`~/.config/arca`) or personal trash (`~/.local/share/Trash`). Every test session must explicitly run inside an isolated sandbox directory using temporary XDG environment variables (`XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`).

---

## 2. Environment Preparation & Sandbox Setup

### Prerequisites

- **Rust Toolchain**: Rust 1.85+ (pinned via `Cargo.toml`).
- **Wayland Compositor**: Active Wayland session (Tessera, Sway, GNOME Wayland, or Hyprland).
- **GPU Driver**: Vulkan 1.3 capable driver with direct rendering permissions.
- **Optics Libraries**: `flux`, `lens`, `iris` installed or linked via `build.rs` rpath-relay.

### Cold-Start Sandbox Setup

Run these commands in your shell to prepare an isolated environment with sample files:

```bash
# 1. Compile the unified binary
cargo build --release -p arca

# 2. Establish isolated XDG sandbox directory
export TEST_ROOT=$(mktemp -d /tmp/arca-sandbox.XXXXXX)
export XDG_CONFIG_HOME="$TEST_ROOT/config"
export XDG_DATA_HOME="$TEST_ROOT/data"
export XDG_CACHE_HOME="$TEST_ROOT/cache"
mkdir -p "$XDG_CONFIG_HOME/arca" "$XDG_DATA_HOME" "$XDG_CACHE_HOME"

# 3. Create sample test files
export TEST_DIR="$TEST_ROOT/workspace"
mkdir -p "$TEST_DIR/subfolder" "$TEST_DIR/empty_dir" "$TEST_DIR/.hidden_dir"
echo "Hello Arca" > "$TEST_DIR/sample.txt"
echo "Hidden content" > "$TEST_DIR/.hidden_file"
echo "Rust code" > "$TEST_DIR/main.rs"

# 4. Expose release binary on PATH
export PATH="$PWD/target/release:$PATH"

echo "Sandbox initialized at: $TEST_ROOT"
```

To clean up after testing:

```bash
rm -rf "$TEST_ROOT"
```

---

## 3. Step-by-Step Verification Scenarios

### Suite 1: Build & CLI Baseline Verification

#### Scenario 1.1: Binary Execution & Help Surface
- **Action**:
  ```bash
  arca --help
  arca --version
  ```
- **Expected Outcome**:
  - Help text renders cleanly with accurate usage patterns, subcommands, and view options (`--grid`, `--list`, `--miller`, `--chooser-prompt`).

#### Scenario 1.2: Standalone CLI File Pickers
- **Action**:
  ```bash
  # Standalone single file picker
  arca --choose-file "$TEST_DIR"
  ```
  Select `sample.txt` and press `Enter`.
- **Expected Outcome**:
  - The window closes and prints the absolute path of `sample.txt` to `stdout`.

---

### Suite 2: Interactive Desktop Window & Chrome Verification

#### Scenario 2.1: Window Launch and Visual Hierarchy
- **Action**:
  ```bash
  arca "$TEST_DIR"
  ```
- **Expected Outcome**:
  - The Arca window appears smoothly on the active Wayland output.
  - Borderless floating chrome renders with subtle titlebar, toolbar action buttons, sidebar bookmarks, and location bar.
  - No flickering or visual tearing on launch.

#### Scenario 2.2: Live Theme Switching
- **Action**:
  1. Click the **Settings** icon on the toolbar.
  2. Select **Dark**.
  3. Select **Light**.
  4. Select **System**.
- **Expected Outcome**:
  - The entire window background, text colors, and selection tokens adapt immediately without restarting the process.
  - Preference is recorded to `$XDG_CONFIG_HOME/arca/arca.conf`.

---

### Suite 3: Three View Modes & Keyboard Navigation

#### Scenario 3.1: Grid View
- **Action**:
  1. Click the **Grid** view button in the toolbar.
  2. Use arrow keys (`Up`, `Down`, `Left`, `Right`) to traverse items.
- **Expected Outcome**:
  - Icons render in a clean responsive grid with centered file labels.
  - Keyboard selection highlights accurately follow arrow navigation.

#### Scenario 3.2: List View & Column Sorting
- **Action**:
  1. Click the **List** view button.
  2. Click the **Name** header to toggle ascending/descending.
  3. Click the **Size** header to sort by size.
  4. Click the **Modified** header to sort by timestamp.
- **Expected Outcome**:
  - Table rows align with columns (Name, Size, Modified). Directories stay pinned to top regardless of sort direction.

#### Scenario 3.3: Miller Columns Navigation
- **Action**:
  1. Switch to **Miller** view (`--miller` or toolbar button).
  2. Navigate into `subfolder` with `Right Arrow`.
  3. Navigate back with `Left Arrow`.
- **Expected Outcome**:
  - Ancestor directory panes stay visible side-by-side, tracking the active path breadcrumbs.

#### Scenario 3.4: Hidden Dotfiles Toggle
- **Action**:
  1. Press `Ctrl+H`.
- **Expected Outcome**:
  - `.hidden_file` and `.hidden_dir` toggle visibility instantly without resetting scroll position.

---

### Suite 4: Multi-Tab Ergonomics

#### Scenario 4.1: Tab Lifecycle
- **Action**:
  1. Press `Ctrl+T` to open a new tab.
  2. Navigate to `$TEST_DIR/subfolder` in the new tab.
  3. Press `Ctrl+Tab` to cycle between tabs.
  4. Press `Ctrl+W` to close the active tab.
- **Expected Outcome**:
  - Each tab preserves independent working directory, selection, and history.
  - Closing a tab returns focus to the adjacent tab.

#### Scenario 4.2: Tab Drag-Reordering
- **Action**:
  1. Open three tabs (`Ctrl+T` twice).
  2. Click and drag the first tab label across to the third position.
- **Expected Outcome**:
  - Tab reorders smoothly under the pointer and settles into the new index cleanly.

---

### Suite 5: File Operations & FreeDesktop Trash

#### Scenario 5.1: New Folder Creation
- **Action**:
  1. Press `Ctrl+Shift+N`.
  2. Type `my-docs` and press `Enter`.
- **Expected Outcome**:
  - A new folder `my-docs` is created immediately on disk and selected in the listing.

#### Scenario 5.2: Safe Inline Rename
- **Action**:
  1. Select `my-docs` and press `Ctrl+E`.
  2. Edit name to `project-docs` and press `Enter`.
- **Expected Outcome**:
  - The folder renames without clobbering or losing selection.

#### Scenario 5.3: Move to Trash & Restore
- **Action**:
  1. Select `sample.txt` and press `Delete`.
  2. Inspect `$XDG_DATA_HOME/Trash/files/` and `$XDG_DATA_HOME/Trash/info/`.
  3. Open Trash in Arca and select **Restore** from the context menu.
- **Expected Outcome**:
  - `sample.txt` moves to trash with valid `.trashinfo` metadata (RFC 3339 timestamp).
  - Restore moves the file back to its original location with full data integrity.

---

### Suite 6: Quick Look Preview

#### Scenario 6.1: Text & Code Inspection
- **Action**:
  1. Select `main.rs` and press `Space`.
- **Expected Outcome**:
  - Quick Look overlay animates smoothly with spring/cubic easing.
  - Syntax/content preview displays text with file size and line information.
  - Pressing `Space` again or `Escape` closes the preview cleanly.

---

### Suite 7: XDG Desktop Portal File Chooser Prompter

#### Scenario 7.1: Prompter stdio Roundtrip
- **Action**:
  Simulate an XDG desktop portal daemon request over stdio:
  ```bash
  echo '{"title":"Select Document","multiple":false,"directory":false}' | arca --chooser-prompt
  ```
  Select `sample.txt` in the dialog and click **Open**.
- **Expected Outcome**:
  - Arca displays the portal chooser interface.
  - Upon selection, emits a JSON packet:
    ```json
    {"response":0,"results":{"uris":["file://.../sample.txt"]}}
    ```
  - Arca exits immediately with return code 0.
