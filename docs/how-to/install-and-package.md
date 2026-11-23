# How to Install and Package Arca

This guide explains how to install pre-built packages or compile and install Arca
from source on various Linux distributions.

---

## Prerequisites

Arca requires:
- A running Wayland compositor (Tessera, Sway, Hyprland, GNOME on Wayland, etc.)
- A GPU with Vulkan support and installed Vulkan drivers (e.g. Mesa RADV, Mesa ANV,
  or NVIDIA proprietary)
- The system libraries Arca opens at runtime: the Vulkan loader, `libwayland-client`,
  `libxkbcommon`, and the font stack (`fontconfig`, `freetype`, `harfbuzz`, `fribidi`)

The **Optics** modules are compiled into the `arca` binary, so there is no Optics
runtime package to install. Building from source still needs the Optics
development files with their static archives (`liboptics_*.a`); see
[Build and Install from Source](#build-and-install-from-source).

---

## Install on Arch Linux

If using the Arch Linux user repository or pre-built packages:

```bash
# Using an AUR helper
yay -S arca

# Or build manually using makepkg
git clone https://aur.archlinux.org/arca.git
cd arca
makepkg -si
```

---

## Build and Install from Source

To compile and install Arca globally:

### Step 1: Install Optics C Libraries

Arca links the Optics modules statically for release builds, while the link mode
itself remains a build input. A default Meson build tree (which builds both
archives and shared objects) is the right input. Optics `v0.0.83` needs a C23
compiler with `#embed` support (GCC >= 15, Clang >= 19). Build and install the
headers plus archives first:

```bash
git clone https://github.com/aegis-shell/optics.git
cd optics
meson setup build --prefix=/usr --default-library=both
meson compile -C build
sudo meson install -C build
```

Verify that `pkg-config` locates the Optics libraries and their static archives:

```bash
pkg-config --modversion optics-app optics-window optics-ui optics-gfx
ls "$(pkg-config --variable=libdir optics-ui)"/liboptics_*.a
```

If the archives are missing, a build that pins the static mode stops with an
explicit message instead of silently producing a dynamically linked binary.
`OPTICS_DYNAMIC=1` builds against the shared objects instead, which then have to
be installed on the target host.

### Step 2: Build Arca

Clone the Arca repository and compile the release binary:

```bash
git clone https://github.com/aegis-shell/arca.git
cd arca
OPTICS_STATIC=1 cargo build --release
scripts/check-optics-link.sh --expect static target/release/arca
```

`OPTICS_STATIC=1` makes an installed-archive mismatch fail the build, and
`scripts/check-optics-link.sh --expect static` confirms that the binary embeds
the Optics modules and carries no Optics runtime path. `ldd target/release/arca`
should list only the system libraries named in the prerequisites.

### Step 3: Install Binary and Desktop Assets

Install the compiled binary and desktop assets to system directories:

```bash
# Install binary
sudo install -Dm755 target/release/arca /usr/local/bin/arca

# Install desktop entry and metainfo
sudo install -Dm644 assets/org.tessera.Arca.desktop /usr/local/share/applications/org.tessera.Arca.desktop
sudo install -Dm644 assets/org.tessera.Arca.metainfo.xml /usr/local/share/metainfo/org.tessera.Arca.metainfo.xml

# Install application icons
sudo install -Dm644 assets/arca.png /usr/local/share/pixmaps/arca.png
for size in 16 24 32 48 64 128 256 512; do
  sudo install -Dm644 "assets/icons/hicolor/${size}x${size}/apps/arca.png" \
    "/usr/local/share/icons/hicolor/${size}x${size}/apps/arca.png"
done

# Update system desktop and icon databases
sudo update-desktop-database /usr/local/share/applications
sudo gtk-update-icon-cache /usr/local/share/icons/hicolor/ -f -t 2>/dev/null || true
```

---

## Verify the Installation

Launch Arca from your terminal:

```bash
arca
```

Check the version output:

```bash
arca --version
```

Confirm that Optics is embedded rather than loaded at runtime:

```bash
ldd "$(command -v arca)" | grep optics   # expected: no output
```
