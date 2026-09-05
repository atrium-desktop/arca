# Portal Chooser Architecture and Isolation

This document explains the security and architecture model behind Arca's role as
an XDG Desktop Portal file chooser prompter on Wayland.

---

## The Wayland Sandboxing Boundary

On modern Wayland systems, containerized and sandboxed applications (such as Flatpak
packages) run without ambient access to the user's `$HOME` directory. When an application
wants to open or save a file, it cannot display an in-process GTK or Qt file dialog
browsing the host filesystem, as it lacks directory read permissions.

Instead, the application invokes the `org.freedesktop.portal.FileChooser` D-Bus
interface provided by `xdg-desktop-portal`.

```text
+---------------------+
| Sandboxed App       | (Flatpak / Browser / Editor)
+---------------------+
           | D-Bus Call: OpenFile(parent_window, filters, options)
           v
+---------------------+
| xdg-desktop-portal  | (Session Portal Daemon)
+---------------------+
           | Private stdio pipe: PrompterPacket (Contract v6)
           v
+---------------------+
| Arca Prompter       | (Host Permissions: full filesystem access)
| arca --chooser-promp|
+---------------------+
           | User selects file / confirms
           v
+---------------------+
| xdg-desktop-portal  | (Exposes file to app via Document Portal FUSE)
+---------------------+
```

---

## Why External Prompter Processes?

Rather than embedding complex file management UI directly inside the persistent
system portal daemon, the Tessera desktop architecture uses **transient prompter
processes**:

1. **Process Isolation**: The prompter process runs with the user's ambient privileges
   in a fresh, short-lived address space.
2. **Zero Lingering State**: Once the user accepts or cancels the dialog, the prompter
   process flushes its response and exits immediately. No idle GUI memory is retained
   when dialogs are closed.
3. **Crash Protection**: If a complex file format or corrupted folder causes an issue,
   only the transient prompter terminates; the desktop session and portal daemon
   remain unaffected.

---

## Standard I/O Protocol Contract

The communication between the portal daemon and Arca uses anonymous UNIX pipes
connected to standard input and standard output:

- **No Public D-Bus Names**: The prompter does not request or claim well-known D-Bus
  bus names. This eliminates bus activation race conditions and permission spoofing.
- **Atomic Responses**: Responses are emitted as single, newline-terminated JSON
  objects. Once the payload is written, standard output is closed and the process
  terminates.
- **Direct CLI Parity**: The same prompter UI logic powers standalone CLI invocations
  (`arca --choose-file`), allowing scripting and shell pipelines to leverage the
  dialog without portal daemon dependencies.
