# Interaction Latency and Click-Stall Analysis

Status: living engineering note
Scope: `crates/arca-ui`, `optics-window` / `optics-ui` boundary, filesystem I/O

This note records why the window could feel "frozen" under repeated clicking,
which causes are fixed, and which are residual risks that need profiling before
a larger redesign.

---

## 1. Symptom

The report is not a crash: the window stops reacting for a noticeable period
while the user clicks rows, tabs, or toolbar controls rapidly, then resumes
(occasionally after cascading further clicks). That shape points at synchronous
work on the UI thread inside a frame, not at a deadlock in a worker.

---

## 2. Root causes found

### 2.1 A directory double-click requested an XDG activation token

`optics_window::window_create_activation_token` is synchronous. Its Wayland
implementation creates a private `wl_event_queue`, commits
`xdg_activation_v1`, and polls that queue for up to ~1 second (50 × 20 ms) before
giving up. Arca called it *before* knowing whether the target was a file or a
directory, so every double-click that merely navigated into a folder could pay
the full timeout on a compositor that delays or omits `done`. Repeated clicking
produced serial one-second stalls.

**Fixed:** `UiApp::open_child` resolves the entry first and requests a token
only for a non-navigable entry (an actual application launch). Enter, the
Miller right-arrow, row double-click, and the context-menu Open item all use the
same policy. Regression:
`app::tests::double_click_into_directory_does_not_panic_on_stale_rows` asserts
the token request count stays zero for a directory.

**Residual:** launching a file still performs the token request synchronously.
The token is a focus-stealing nicety, never a correctness requirement; if a
compositor is known to be slow, deferring launch to a worker after the token is
obtained (or dropping the token) is the next step.

### 2.2 Every right-click re-scanned the desktop-entry database

`applications_for_mime` walks every `applications/` directory in
`XDG_DATA_DIRS`, parses each `.desktop` file, then parses every `mimeapps.list`.
That work ran on the UI thread whenever a context menu opened. On a large or
automounted application directory, repeated right-clicks turn into a disk
latency storm.

**Fixed:** `UiApp` caches the resolved "Open with" list per MIME type for the
window's lifetime. Installing an application while the file manager is open is
not an interaction the cache must observe.

**Residual:** the first resolution for a MIME type is still synchronous, as is
`open_path_with_token` when launching. Moving discovery to a short-lived worker
keyed by MIME would remove it entirely.

### 2.3 Click handling accumulates an event backlog behind any stall

Once one frame blocks (2.1 or 2.2), Wayland input events queue. When the frame
finally returns, the backlog is replayed. Rapid clicks can form fresh
double-click pairs in that replay and trigger another stall, which is why the
symptom reads as "it keeps freezing while I click" rather than one isolated
pause.

A second amplifier was index-only double-click detection: clicking a row,
navigating (bookmark, location submit, folder open), and clicking the row at the
same position within 400 ms looked like a double-click on the *new* entry and
launched it. Rapid clicking across navigation therefore spawned applications
unintentionally, which itself produces long frames. Double-click detection is
now keyed by the **entry absolute path**, so neither a rebuilt visible index
space nor a same-named entry in another directory can replay a stale click pair.

**Mitigation:** remove the stalls above. A future hardening step is to coalesce
pointer events that arrive while a frame is overdue, but that belongs in
`optics-window`'s input accumulator, not in Arca.

### 2.4 Navigation no longer reads directories on the interactive UI thread

`AppState::navigate`/`set_dir` used to read and stat the entire directory,
rebuild up to four Miller columns, re-arm inotify watches, and (inside Trash)
enumerate `.trashinfo` records inside the build callback. A double-click on a
network mount or a very large directory could therefore block a frame.

**Fixed:** the interactive window uses the asynchronous navigation API
(`begin_navigate` / `poll_navigation`). Each request owns a named worker; a
newer request drops the older receiver, so only the last navigation can publish
and a hung mount cannot block navigation to another directory. The old listing
stays interactive with a "Loading …" status until the worker publishes. The
synchronous API remains for the CLI, chooser, and deterministic tests, and a
synchronous call cancels any pending request.

**Residual:** when the active listing publishes, Miller **ancestor** columns
(up to three smaller directories) are still read synchronously; tab switching
and watcher-driven refresh still read synchronously; startup reads the initial
directory synchronously. Those are the next profiling targets.

### 2.5 Watcher-driven refresh can starve frames in a busy directory

The inotify worker debounces events by 50–200 ms, but each delivered event
re-reads the directory. A directory that changes continuously (downloads,
build output, a synced folder) can keep the loop at full cadence doing
`read_dir` + stat + sort. This is correct but can consume a core and make
interaction feel sluggish.

Watch this if a profile shows `read_dir`/`sort_entries` under `drain_fs_events`.
The debounce is the tuning point; the refresh itself already preserves
selection by name.

### 2.6 Thumbnail uploads and the Optics image-plan bug

Thumbnail decoding is off-thread, but decoded pixels are uploaded on the UI
thread, and evicting the 513th upload releases an `optics_gfx_image`. Optics
`v0.0.85` fixed a class of DisplayList replay bug where a snapshot and the
original image resolved to different RenderPlan handles, and where the
`VkImage` lifetime was observed per wrapper rather than per shared owner. Those
bugs could produce missed passes, stale pixels, or a stalled frame in
image-heavy directories. Arca now pins `v0.0.86`, which includes the fix.

The store still releases on eviction; the Optics contract retains any snapshot
a recorded DisplayList captured, so release is safe as long as the device is
alive (Arca already tears textures down from the lifecycle stop hook).

---

## 3. Diagnostic recipe

1. Reproduce with a directory containing many entries and a few images.
2. Record a profile (`perf record -g -p $(pidof arca) -- sleep 20`) while
   clicking rapidly, then inspect stacks for:
   - `optics_window_create_activation_token` / `poll` (cause 2.1),
   - `discover_applications` / `parse_desktop_file` (2.2),
   - `read_dir` / `sort_entries` / `rebuild_miller_columns` (2.4),
   - `optics_gfx_image_update_region` / plan replay (2.6).
3. In a headless checkout, run the stress suites:
   ```bash
   cargo test -p arca-tests interaction_stress
   cargo test -p arca-tests virtualization
   ```
   Neither uses a compositor; a failure there is app-side, while a clean run
   with a live stall points at the platform/GPU boundary.

---

## 4. Guardrails in the tree

| Rule | Test |
| :--- | :--- |
| Directory opens never request an activation token | `app::tests::double_click_into_directory_does_not_panic_on_stale_rows` |
| Navigation cannot replay a stale double-click on a new entry | `app::tests::double_click_does_not_cross_a_navigation_to_a_different_entry` |
| Interactive navigation reads off-thread, publishes last-writer-wins, and keeps the history cursor | `state::tests::async_navigation*`, `ui_flows::asynchronous_navigation_never_blocks_the_frame_and_publishes` |
| Hundreds of clicks stay bounded and keep one selection | `interaction_stress::hundreds_of_row_clicks_are_bounded_and_keep_one_selection` |
| Repeated chrome interaction returns to quiescence | `interaction_stress::repeated_settings_and_view_changes_return_to_quiescence` |
| Repeated directory round-trips stay bounded | `interaction_stress::rapid_directory_double_clicks_are_bounded` |
| Large/scrolled listings stay virtualized | `virtualization::*` |
| A settled UI produces zero damage and no pending animation | `damage_and_quiescence::*` |
