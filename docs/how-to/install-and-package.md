# How to Install and Package Arca

This guide explains how to install pre-built packages or compile and install Arca
from source on various Linux distributions.

---

## Prerequisites

Arca requires:
- A running Wayland compositor (Tessera, Sway, Hyprland, GNOME on Wayland, etc.)
- A GPU with Vulkan support and installed Vulkan drivers (e.g. Mesa RADV, Mesa ANV,
  or NVIDIA proprietary)
- The **Optics** shared runtime libraries (`libflux.so`, `liblens.so`, `libiris.so`)

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

If Optics is not yet installed on your system, build and install it first:

```bash
git clone https://github.com/aegis-shell/optics.git
cd optics
meson setup build --prefix=/usr
meson compile -C build
sudo meson install -C build
```

Verify that `pkg-config` locates the Optics libraries:

```bash
pkg-config --modversion flux lens iris
```

### Step 2: Build Arca

Clone the Arca repository and compile the release binary:

```bash
git clone https://github.com/aegis-shell/arca.git
cd arca
cargo build --release
```

### Step 3: Install Binary and Desktop Assets

Install the compiled binary and desktop assets to system directories:

```bash
# Install binary
sudo install -Dm755 target/release/arca /usr/local/bin/arca

# Install desktop entry
sudo install -Dm644 assets/org.tessera.Arca.desktop /usr/local/share/applications/org.tessera.Arca.desktop

# Update system desktop database
sudo update-desktop-database /usr/local/share/applications
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
