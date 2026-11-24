---
id: ADR-0020
title: "Asynchronous Directory Navigation at the UI Boundary"
status: accepted
date: 2026-11-24
scope: [engine/state, presentation/ui, performance/input-latency]
superseded_by: null
negative_knowledge: true
---

# 0020. Asynchronous Directory Navigation at the UI Boundary

- Status: Accepted
- Date: 2026-11-24
- Deciders: Arca Architecture Working Group
- Consulted: Optics UI Maintainers
- References: [Interaction Latency](../dev/interaction-latency.md), [ADR-0019](0019-explicit-test-resources.md)

---

## Context and Problem Statement

`AppState::navigate` read and stat the whole target directory inside the frame
that received the click, then rebuilt Miller columns, re-armed inotify watches,
and (in the Trash) enumerated `.trashinfo` records. On a large directory, a
network mount, or a directory changing during the read, that work blocks the
UI thread and reads as a freeze. The earlier fix removed the synchronous XDG
activation-token roundtrip for directory opens, but the directory read itself
remained on the frame path.

The engine is deliberately synchronous and headless: the CLI, the portal
chooser, and every state-machine test depend on `navigate` returning with the
new listing. Removing that API would be a breaking change without solving the
UI problem.

---

## Decision Drivers

- Driver 1: **No directory I/O in the interactive frame.** A click must not
  wait for a filesystem read.
- Driver 2: **Stale results must never publish.** Rapid clicking must resolve
  to the last request and the tab that requested it.
- Driver 3: **Keep the synchronous engine contract.** CLI, chooser, tests, and
  embedders continue to call `navigate` and read the result immediately.
- Driver 4: **Deterministic headless tests.** A test can pin the synchronous
  policy; the asynchronous policy is exercised explicitly.
- Driver 5: **No hung worker queue.** A read stuck on one mount must not block
  navigation to another directory forever.

---

## Architectural Decisions

### 1. Engine exposes a request/poll pair

- `AppState::begin_navigate`, `begin_go_back`, `begin_go_forward`, and
  `begin_go_up` accept a request and return immediately.
- `AppState::poll_navigation` is non-blocking; it publishes a listing when the
  worker completes, or reports a disconnected worker.
- `AppState::is_navigating` tells the host to keep requesting frames.
- `navigate`/`go_back`/`go_forward`/`go_up` stay synchronous and cancel any
  in-flight request, so a later synchronous caller cannot be overwritten by a
  stale result.

### 2. One request owns one worker

Each request spawns a named `arca-navigation` thread and owns its receiver.
Starting a newer request drops the older receiver; the older worker's `send`
fails harmlessly and its result is discarded. A worker blocked on one mount
does not delay a newer request, because it is not sharing a queue.

### 3. Results are applied by tab identity

`begin_set_dir` records the target tab's stable id. `poll_navigation` resolves
that id before applying the listing; if the tab was closed, the result is
dropped. Applying a background tab's listing does not rebuild the active
Miller columns or Trash index.

### 4. The interactive window enables the asynchronous policy

`UiApp` defaults to asynchronous navigation. The application shell (`run`)
uses that default; the chooser and deterministic headless tests call
`sync_navigation()`. The test harness can opt in with
`set_async_navigation(true)`; the integration suite exercises directory
double-click, back/forward, and the publish path under the production policy.

### 5. Navigation is applied with the same semantics as the synchronous path

`set_dir` and the async apply path share `apply_listing`: cwd, filter clearing,
selection reset, Miller rebuild (active tab only), watcher sync, and Trash
index rebuild. A read error publishes the path with `read_error`, exactly as
the synchronous path does.

---

## Consequences

### Positive

- Clicking into a slow or network directory no longer blocks the frame; the
  previous listing stays interactive and the status bar reports loading.
- Rapid navigation is last-writer-wins and cannot land in the wrong tab.
- CLI/chooser/tests keep a simple synchronous contract.

### Negative & Trade-offs

- One thread per navigation request. A pathologically hung filesystem leaves a
  blocked thread until the syscall returns; the UI remains responsive and a
  newer request still proceeds. This is the deliberate trade-off over a fixed
  worker pool, where one hung mount would block every later request.
- Miller **ancestor** columns are still rebuilt synchronously when the active
  listing publishes (up to three directory reads). The active directory, which
  the user clicked, is the expensive/blocking one and is now off-thread.
- Tab switching and watcher-driven refresh still read synchronously; they are
  less frequent and are the next profiling targets.
- A test that uses `UiApp::new` without `sync_navigation()` must pump frames
  until `is_navigating()` is false.

---

## Rejected Alternatives & Negative Knowledge

### 1. Make the whole engine asynchronous

- **Why considered**: one code path.
- **Why rejected**: the CLI, chooser, and hundreds of deterministic tests rely
  on `navigate` returning with the listing. A request/poll layer at the UI
  boundary buys the interactive guarantee without deleting that contract.

### 2. Blocking read with a timeout

- **Why considered**: keeps the code synchronous.
- **Why rejected**: a filesystem read cannot be cancelled safely; a timeout
  leaves a thread inside the syscall and may still publish a late result unless
  a generation check is added. It also still stalls the frame for the timeout.

### 3. A single worker queue

- **Why considered**: bounded threads.
- **Why rejected**: one hung mount would block every later navigation request.
  Thread-per-request with a latest-receiver rule keeps unrelated navigation
  responsive.

### 4. Keep only the synchronous path and document it

- **Why considered**: minimal change.
- **Why rejected**: it leaves the reported freeze cause in place and makes the
  interaction-latency invariant unenforceable.

---

## Links

- [ADR 0017: Root Test Architecture](0017-synchronize-with-optics-v0.0.86-and-root-test-architecture.md)
- [ADR 0019: Explicit Test Resources](0019-explicit-test-resources.md)
- [Interaction Latency](../dev/interaction-latency.md)
- [Automated Testing](../dev/testing.md)
