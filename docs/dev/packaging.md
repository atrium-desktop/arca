# Distribution Packaging

This guide covers building distribution packages (Arch Linux, Debian/Ubuntu,
Fedora, and Flatpak) for Arca from source releases.

Daily source-tree development does not require packaging or installing Arca to the
system; see [Optics Development Worktree](optics-dev-worktree.md) instead.

---

## 1. Dependency Contract

Arca has two distinct dependency layers: system-level native libraries and Rust
dependencies.

### System Native Libraries (discovered via `pkg-config`)

Arca links against the **Optics** C libraries via their official Rust bindings:
- `optics-app` — Application shell that runs an `optics-ui` interface in an
  `optics-window` window.
- `optics-window` — Wayland windowing, platform surface, and frame cadence.
- `optics-ui` and `optics-ui-sys` — Immediate-mode UI layout and primitives.
- `optics-gfx` — GPU Vulkan rendering backend.
- `optics-svg` — Vector-document compiler used by `optics-ui` to parse runtime
  SVG icons (including Arca's own icon set).

Arca's release artifact embeds these modules (ADR-0014), while development
builds follow the host's default. The link mode is a build input handled
upstream by `optics-build` (ADR-0014 / ADR-0015):

| Invocation | Result |
| :--- | :--- |
| no request | shared-preferred (Optics v0.0.83): `liboptics_*.so` when the probe finds it, else `liboptics_*.a` on a static-only tree |
| `static` Cargo feature | require the archives; fail when the host has none |
| `OPTICS_STATIC=1` | require the archives; overrides the feature and the default |
| `OPTICS_DYNAMIC=1` | force the shared objects |

A release build therefore needs the development files *and* the static
archives and pins the mode explicitly: `pkg-config` must resolve each module's
`.pc` file, and `liboptics_*.a` must exist in the corresponding `libdir`. Optics
builds `default_library=both` by default, so a normal `meson setup` satisfies
this; a package that splits `optics` into a `-devel`/`-static` subpackage must
expose the archives at build time.

Verify that the native development files are installed on the packaging host:

```bash
pkg-config --modversion optics-app optics-window optics-ui optics-gfx optics-svg
pkg-config --variable=libdir optics-ui     # must contain liboptics_ui.a
```

The system package must also provide:
- `wayland-client` (>= 1.22)
- `vulkan-loader` (Vulkan 1.3+ loader and ICD drivers: Mesa RADV, ANV, or proprietary)
- `xkbcommon` (>= 1.5.0)
- The font stack `optics-text` links at runtime (`fontconfig`, `freetype`,
  `harfbuzz`, `fribidi`)

Only Optics is embedded; the libraries above stay dynamic runtime dependencies.
`OPTICS_DYNAMIC=1` is not a packaging mode: it exists for development
worktrees and yields a binary that needs the Optics runtime libraries.

### Rust Dependencies

In canonical packaging mode:
- `Cargo.lock` must resolve against the published optics release tag.
- Downstream packagers must not enable `.cargo/config.toml` path patches pointing to
  local worktrees.
- Approved external crates are strictly limited:
  - `image` (with features `png` and `jpeg` only)
  - `serde` and `serde_json` (used for the XDG desktop portal prompter protocol)

---

## 2. Reproducible Source Preparation

For reproducible, offline distribution builds:

1. Ensure no local `.cargo/config.toml` patch overrides exist:
   ```bash
   test ! -e .cargo/config.toml
   ```
2. Verify that `Cargo.lock` evaluates cleanly without modifying source references:
   ```bash
   cargo metadata --locked --format-version 1 > /dev/null
   ```
3. Prepare vendored dependencies for offline build environments:
   ```bash
   cargo vendor --locked vendor
   ```
4. Save the emitted Cargo vendor configuration into `.cargo/config.toml` within the
   package build root:
   ```toml
   [source.crates-io]
   replace-with = "vendored-sources"

   [source.vendored-sources]
   directory = "vendor"
   ```

---

## 3. Build Flags & Compilation

Execute the workspace release build with frozen dependency validation:

```bash
# Offline vendored build
OPTICS_STATIC=1 cargo build --frozen --offline --release --package arca

# Or online build using distribution pre-cached cargo tree
OPTICS_STATIC=1 cargo build --locked --release --package arca
```

Run headless tests during packaging to verify frame invariants:

```bash
cargo test --frozen --offline --workspace
```

Confirm that the produced binary embeds Optics, then gate the package on it.
The release invocation pins the mode (ADR-0014) instead of relying on what the
build host happens to provide:

```bash
OPTICS_STATIC=1 cargo build --locked --release --package arca
scripts/check-optics-link.sh --expect static target/release/arca
```

`OPTICS_STATIC=1` makes the `optics-*-sys` build scripts require the
`liboptics_*.a` archives and fail with the remediation when the packaging host
exposes only `liboptics_*.so`. The gate itself needs the binutils inspection
tools (`readelf`, `nm`, `strings`), so `binutils` belongs in the build
dependencies. It also reads the symbol table, so run it before any strip step:
`strip --strip-debug` keeps that evidence, and a fully stripped artifact needs
`--allow-unverified-stripped`. The script fails on a `liboptics_*.so`
`DT_NEEDED` entry, on an Optics runpath (including a `/usr/local/lib` that holds
`liboptics_*.so`), or on a binary with no Optics code at all, so a misconfigured
build cannot ship silently.

---

## 4. Install Manifest

A distribution package installs the following files:

| Source Path | Target Destination | Permissions | Purpose |
|-------------|--------------------|-------------|---------|
| `target/release/arca` | `/usr/bin/arca` | `0755` | Primary executable |
| `assets/org.tessera.Arca.desktop` | `/usr/share/applications/org.tessera.Arca.desktop` | `0644` | XDG Desktop entry |
| `assets/arca.desktop` | `/usr/share/applications/arca.desktop` | `0644` | Legacy compatibility symlink |
| `assets/org.tessera.Arca.metainfo.xml` | `/usr/share/metainfo/org.tessera.Arca.metainfo.xml` | `0644` | AppStream metadata |
| `assets/icons/folder.svg` | `/usr/share/icons/hicolor/scalable/apps/arca.svg` | `0644` | Scalable application icon |
| `assets/icons/*.svg` | `/usr/share/arca/icons/*.svg` | `0644` | Bundled UI icon set |
| `LICENSE` | `/usr/share/licenses/arca/LICENSE` | `0644` | License file |

### Desktop Entry & MIME Associations

The desktop entry registers Arca as a default handler for directories:

```ini
[Desktop Entry]
Type=Application
Name=Arca
GenericName=File Manager
Comment=Browse and manage files on Wayland
Exec=arca %U
Icon=arca
Terminal=false
Categories=System;FileTools;FileManager;Utility;Core;
MimeType=inode/directory;x-scheme-handler/file;
StartupNotify=true
StartupWMClass=arca
Keywords=files;folders;directory;manager;explore;
```

Update the host desktop database in post-install hooks:

```bash
update-desktop-database -q /usr/share/applications
gtk-update-icon-cache -qtf /usr/share/icons/hicolor
```

---

## 5. Portal Chooser Integration

Arca includes an integrated XDG Desktop Portal file chooser prompter
(`org.freedesktop.impl.portal.FileChooser` v3).

In desktop environments implementing external prompters (e.g., Tessera Portal),
configure Arca as the active chooser prompter:

```ini
# /etc/xdg/xdg-desktop-portal/portals.conf or tessera-portal.conf
[preferred]
default=tessera
org.freedesktop.impl.portal.FileChooser=arca
```

When invoked by the portal daemon, Arca runs as:

```bash
arca --chooser-prompt
```

It communicates using the Tessera prompter contract v6 (JSON payload over standard input
and standard output), requiring zero external D-Bus daemon registration.

---

## 6. Distribution Recipes

### Arch Linux (`PKGBUILD`)

```bash
# Maintainer: Ming Li <me@hihusky.com>
pkgname=arca
pkgver=0.2.1
pkgrel=1
pkgdesc="A fast file manager for Wayland, built on the optics stack"
arch=('x86_64' 'aarch64')
url="https://github.com/aegis-shell/arca"
license=('MIT')
depends=('wayland' 'vulkan-icd-loader' 'libxkbcommon' 'fontconfig' 'freetype2' 'harfbuzz')
makedepends=('cargo' 'pkg-config' 'optics' 'binutils')
source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")
sha256sums=('SKIP')

prepare() {
    cd "$pkgname-$pkgver"
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$pkgname-$pkgver"
    OPTICS_STATIC=1 cargo build --frozen --release --package arca
}

check() {
    cd "$pkgname-$pkgver"
    cargo test --frozen --workspace
    # The release artifact embeds the Optics modules (ADR-0014): the
    # installed binary must not depend on the Optics runtime package.
    scripts/check-optics-link.sh --expect static target/release/arca
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/arca" "$pkgdir/usr/bin/arca"
    install -Dm644 "assets/org.tessera.Arca.desktop" "$pkgdir/usr/share/applications/org.tessera.Arca.desktop"
    ln -s "org.tessera.Arca.desktop" "$pkgdir/usr/share/applications/arca.desktop"
    install -Dm644 "assets/icons/folder.svg" "$pkgdir/usr/share/icons/hicolor/scalable/apps/arca.svg"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
```

### Debian / Ubuntu (`debian/rules` snippet)

```makefile
#!/usr/bin/make -f
export DH_VERBOSE = 1

%:
	dh $@ --buildsystem=cargo

override_dh_auto_build:
	OPTICS_STATIC=1 cargo build --release --locked --package arca

override_dh_auto_test:
	cargo test --release --locked --workspace
	# Run the link gate before any strip step. `strip --strip-debug` keeps the
	# symbol table the gate reads; a fully stripped binary needs
	# --allow-unverified-stripped.
	scripts/check-optics-link.sh --expect static target/release/arca

override_dh_auto_install:
	install -Dm755 target/release/arca debian/arca/usr/bin/arca
	install -Dm644 assets/org.tessera.Arca.desktop debian/arca/usr/share/applications/org.tessera.Arca.desktop
	install -Dm644 LICENSE debian/arca/usr/share/doc/arca/copyright
```

### Fedora / RHEL (`arca.spec` snippet)

```spec
Name:           arca
Version:        0.2.1
Release:        1%{?dist}
Summary:        A fast file manager for Wayland, built on the optics stack
License:        MIT
URL:            https://github.com/aegis-shell/arca
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust >= 1.85
BuildRequires:  binutils
# Static Optics link (ADR-0014): the pkgconfig packages must ship the
# liboptics_*.a archives, not only the shared objects.
BuildRequires:  pkgconfig(optics-app)
BuildRequires:  pkgconfig(optics-gfx)
BuildRequires:  pkgconfig(optics-ui)
BuildRequires:  pkgconfig(optics-window)
BuildRequires:  pkgconfig(optics-svg)
BuildRequires:  pkgconfig(wayland-client)
BuildRequires:  pkgconfig(vulkan)
BuildRequires:  pkgconfig(xkbcommon)
Requires:       vulkan-loader
Requires:       wayland
Requires:       xkbcommon
Requires:       fontconfig
Requires:       freetype
Requires:       harfbuzz

%description
Arca is a fast file manager for Wayland, designed as a companion application
for the Tessera desktop ecosystem while running standalone on any Wayland
compositor.

%prep
%autosetup -p1

%build
OPTICS_STATIC=1 cargo build --release --locked --package arca

%check
cargo test --release --locked --workspace
scripts/check-optics-link.sh --expect static target/release/arca

%install
install -Dm755 target/release/arca %{buildroot}%{_bindir}/arca
install -Dm644 assets/org.tessera.Arca.desktop %{buildroot}%{_datadir}/applications/org.tessera.Arca.desktop
install -Dm644 assets/org.tessera.Arca.metainfo.xml %{buildroot}%{_metainfodir}/org.tessera.Arca.metainfo.xml
install -Dm644 assets/icons/folder.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/arca.svg

%files
%license LICENSE
%{_bindir}/arca
%{_datadir}/applications/org.tessera.Arca.desktop
%{_metainfodir}/org.tessera.Arca.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/arca.svg
```

### Flatpak Manifest (`org.tessera.Arca.json` snippet)

```json
{
  "app-id": "org.tessera.Arca",
  "runtime": "org.freedesktop.Platform",
  "runtime-version": "24.08",
  "sdk": "org.freedesktop.Sdk",
  "sdk-extensions": [
    "org.freedesktop.Sdk.Extension.rust-stable"
  ],
  "command": "arca",
  "finish-args": [
    "--socket=wayland",
    "--device=dri",
    "--filesystem=host",
    "--filesystem=home",
    "--talk-name=org.freedesktop.portal.Desktop",
    "--talk-name=org.freedesktop.portal.FileChooser"
  ],
  "build-options": {
    "append-path": "/usr/lib/sdk/rust-stable/bin",
    "env": {
      "CARGO_HOME": "/run/build/arca/cargo"
    }
  },
  "modules": [
    {
      "name": "optics",
      "buildsystem": "meson",
      "sources": [
        {
          "type": "git",
          "url": "https://github.com/aegis-shell/optics.git",
          "tag": "v0.0.83"
        }
      ]
    },
    {
      "name": "arca",
      "buildsystem": "simple",
      "build-commands": [
        "cargo --offline fetch --manifest-path Cargo.toml",
        "OPTICS_STATIC=1 cargo --offline build --release --package arca",
        "scripts/check-optics-link.sh --expect static target/release/arca",
        "install -Dm755 target/release/arca /app/bin/arca",
        "install -Dm644 assets/org.tessera.Arca.desktop /app/share/applications/org.tessera.Arca.desktop",
        "install -Dm644 assets/org.tessera.Arca.metainfo.xml /app/share/metainfo/org.tessera.Arca.metainfo.xml",
        "install -Dm644 assets/icons/folder.svg /app/share/icons/hicolor/scalable/apps/org.tessera.Arca.svg"
      ],
      "sources": [
        {
          "type": "dir",
          "path": "."
        }
      ]
    }
  ]
}
```

---

## 7. Package Verification and Smoke Testing

Before distributing packages:

1. **Verify Binary Dependencies**:
   ```bash
   ldd /usr/bin/arca
   scripts/check-optics-link.sh --expect static /usr/bin/arca
   ```
   The binary must not list any `liboptics_*` entry: Optics is embedded
   (ADR-0014). `libvulkan.so.1`, `libwayland-client.so.0`, `libxkbcommon.so.0`,
   and the font stack resolve from the distribution packages and must not
   report `not found`.

2. **Verify Desktop File Syntax**:
   ```bash
   desktop-file-validate /usr/share/applications/org.tessera.Arca.desktop
   ```

3. **Verify CLI Picker Mode**:
   ```bash
   arca --help
   arca --version
   ```
