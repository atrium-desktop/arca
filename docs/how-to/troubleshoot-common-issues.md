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
1. Use a current build: directory opens no longer request activation tokens and
   "Open with" results are cached per MIME type.
2. If the stall happens only in one directory, check whether it is on a network
   mount or receives continuous writes (downloads, build output). Those
   directories make the synchronous directory read and watcher refresh visible.
3. Capture a profile while reproducing and look for
   `optics_window_create_activation_token`, `discover_applications`,
   `read_dir`, or plan replay; report the stack with your bug report.
4. Run the headless stress suites to separate an app-side stall from a
   compositor/GPU stall:
   ```bash
   cargo test -p arca-tests interaction_stress
   cargo test -p arca-tests virtualization
   ```

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
