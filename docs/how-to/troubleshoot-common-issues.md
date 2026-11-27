# How to Troubleshoot Common Issues

This guide provides diagnostic steps and solutions for common runtime, display,
and configuration problems encountered when running Arca.

---

## Wayland Connection Errors

### Symptom: `Failed to connect to Wayland display`

```text
arca: failed to connect to Wayland display: No such file or directory
```

### Cause
Arca requires an active Wayland compositor session. It does not run directly on X11
without XWayland, nor can it start without a valid compositor socket.

### Resolution
1. Verify that your desktop session is running on Wayland:
   ```bash
   echo $WAYLAND_DISPLAY
   ```
   This should output a socket name such as `wayland-0` or `wayland-1`.
2. If connecting via SSH or a subshell, pass the socket and runtime directory:
   ```bash
   export WAYLAND_DISPLAY=wayland-0
   export XDG_RUNTIME_DIR=/run/user/$(id -u)
   arca
   ```

---

## Vulkan and GPU Initialization Failures

### Symptom: `Vulkan initialization failed` or crash on startup

```text
optics_gfx: failed to find suitable Vulkan physical device
```

### Cause
The Optics graphics backend (`optics-gfx`) requires Vulkan 1.2+ with swapchain and surface
extensions.

### Resolution
1. Verify that Vulkan ICD drivers are installed:
   ```bash
   # On Arch Linux:
   pacman -S vulkan-radeon # for AMD
   pacman -S vulkan-intel  # for Intel
   pacman -S nvidia-utils  # for NVIDIA

   # On Debian / Ubuntu:
   apt install mesa-vulkan-drivers
   ```
2. Test device enumeration using `vulkaninfo`:
   ```bash
   vulkaninfo --summary
   ```
3. If running in a virtual machine (e.g. QEMU/KVM), ensure `virtio-gpu` or `virpipe`
   with host Vulkan acceleration is enabled.

---

## Portal FileChooser Not Opening for Sandboxed Apps

### Symptom: Flatpak or browser file picker opens GTK dialog or fails silently

### Cause
The session portal daemon (`xdg-desktop-portal`) does not know to route FileChooser
requests to Arca, or the portal configuration defaults to another prompter.

### Resolution
1. Create or edit `~/.config/xdg-desktop-portal/portals.conf`:
   ```ini
   [preferred]
   default=tessera
   org.freedesktop.impl.portal.FileChooser=arca
   ```
2. Restart the user portal daemon:
   ```bash
   systemctl --user restart xdg-desktop-portal
   ```
3. Test Arca's prompter directly in your terminal:
   ```bash
   arca --choose-file $HOME
   ```

---

## Thumbnails Fail to Generate

### Symptom: Images show generic file icons instead of previews

### Resolution
1. Check `~/.config/arca/arca.conf` and ensure `show_thumbnails` is enabled:
   ```ini
   show_thumbnails = true
   ```
2. Ensure the thumbnail cache directory exists and is writable:
   ```bash
   mkdir -p ~/.cache/thumbnails/normal
   touch ~/.cache/thumbnails/normal/test && rm ~/.cache/thumbnails/normal/test
   ```
3. Verify image format: Arca decodes PNG and JPEG images, and FLAC/MP3 cover art.
   Other formats (such as SVG, WebP, or AVIF) currently display standard MIME icons.

---

## Window Stalls While Clicking Rapidly

### Symptom: the window stops responding for a second or more during repeated
clicks, then catches up

### Cause
A frame is doing synchronous work on the UI thread. Historical causes in Arca
were a directory double-click requesting an XDG activation token (a up-to-one-
second roundtrip that navigation never needs) and a right-click re-scanning the
entire `.desktop` database. Slow network directories and continuously changing
watched directories can also make navigation/refresh expensive.

### Resolution
1. Ask Arca what it saw. While the window is still frozen, run from a
   terminal:
   ```bash
   arca --diagnostics
   grep -E 'arca::(alert|apm)' ~/.local/state/arca/logs/arca.log | tail
   ```
   - `ui_stall … in <operation>` names the work that blocked the window
     (for example `fs.read_dir` on a slow mount, `ui.activation_token`).
   - `frame_storm` means the window is spinning without drawing — usually a
     lost compositor connection; restart Arca and include the log in a report.
   See the [Observability Reference](../reference/observability.md).
2. Use a current build: directory opens no longer request activation tokens and
   "Open with" results are cached per MIME type.
3. If the stall happens only in one directory, check whether it is on a network
   mount or receives continuous writes (downloads, build output). Those
   directories make the synchronous directory read and watcher refresh visible.
4. Capture a profile while reproducing and look for
   `optics_window_create_activation_token`, `discover_applications`,
   `read_dir`, or plan replay; report the stack with your bug report.
5. Run the headless stress suites to separate an app-side stall from a
   compositor/GPU stall:
   ```bash
   cargo test -p arca-tests interaction_stress
   cargo test -p arca-tests virtualization
   ```

---

## Window Frozen: No Response at All

### Symptom: the window stops redrawing and never recovers; one CPU core is pinned

### Cause
The UI thread is stuck in a loop. Either Arca's frame build blocks (named by
`ui_stall … in <operation>`), or the toolkit loops after the build returned:
layout, retained-store maintenance, or render inside Optics. The second kind
is reported as `ui_stall … outside the frame build`. Example: the Optics
v0.0.86 store-shrink bug (fixed in the next Optics release) rehashed a
surviving population into a table too small to hold it and spun forever
after you left a large thumbnail grid.

### Resolution
1. **Keep the frozen process running.** Restarting destroys the evidence.
2. **Collect what Arca captured automatically:**
   ```bash
   arca --diagnostics                          # alerts + newest stall captures
   ls -t ~/.local/state/arca/stalls/ | head -3
   grep -E 'arca::(alert|capture)' ~/.local/state/arca/logs/arca.log | tail
   ```
   With stall capture on (debug builds by default, or
   `ARCA_STALL_CAPTURE=1`), `stalls/stall-*.txt` holds every thread's stack
   from inside the freeze. Thread 1 (`arca`) is the UI thread; its innermost
   frames are the loop. See
   [Stall Stack Capture](../reference/observability.md#stall-stack-capture).
3. **Confirm the shape without a debugger:**
   ```bash
   pid=$(pgrep -x arca)
   for t in /proc/$pid/task/*; do
     printf '%-18s %s utime=%s\n' "$(cat $t/comm)" "$(awk '{print $3}' $t/stat)" "$(awk '{print $14}' $t/stat)"
   done
   grep voluntary_ctxt /proc/$pid/task/$pid/status
   ```
   A main thread in state `R`, with `utime` climbing and voluntary context
   switches not moving, is a user-space spin, not an I/O wait.
4. **If no capture exists** (release build without `ARCA_STALL_CAPTURE`, no
   `gdb` installed, or `ptrace_scope` ≥ 2), take one by hand:
   ```bash
   sudo gdb -p "$pid" -batch -ex 'thread apply 1 bt 40' -ex 'thread apply all bt 15' > arca-bt.txt 2>&1
   ```
   Without root, `ptrace_scope=1` rejects the attach because the debugger is
   not Arca's ancestor.
5. **Frames in `liboptics_*`** point at the toolkit. In development builds
   these libraries load from `../optics/build` (see
   [Local Optics Development](../dev/optics-local-development.md)); fix
   there, rebuild with `meson compile -C ../optics/build`, and restart Arca.
   Debug Optics builds also validate the retained store every frame and abort
   with `optics-ui: retained store invariant violated: …` instead of hanging.
6. Report the capture file, the `arca --diagnostics` output, and the steps
   that led to the freeze.

---

## Corrupted or Unresponsive Configuration

### Symptom: Application crashes immediately upon reading config

### Resolution
1. Move the current configuration file aside:
   ```bash
   mv ~/.config/arca/arca.conf ~/.config/arca/arca.conf.bak
   ```
2. Launch Arca:
   ```bash
   arca
   ```
   Arca generates clean default configuration settings automatically.
