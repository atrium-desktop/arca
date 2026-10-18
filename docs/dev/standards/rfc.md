# Request for Comments (RFC) Standards

This document defines the process for authoring, reviewing, and ratifying Request for Comments (RFC) proposals in Arca.

---

## 1. When to Author an RFC

While an **ADR** records an accepted architectural decision and its negative knowledge, an **RFC** is an upfront exploratory proposal used during early deliberation when:
- The problem space has multiple competing solutions with divergent trade-offs (e.g., alternative thumbnail caching strategies, virtualized layout math).
- The proposed change spans multiple major subsystems (e.g., introducing a new background indexing service, altering the portal IPC wire schema).
- Desktop ecosystem or team-wide alignment is required before investing in proof-of-concept code.

---

## 2. RFC Lifecycle

```text
  [ Proposed ] ──► [ Under Review ] ──► [ Approved ] ──► [ Migrated to ADR ]
                          │
                          └──► [ Closed / Withdrawn ]
```

1. **Proposed**: Draft RFC submitted for peer feedback.
2. **Under Review**: Active review period with scheduled architectural discussion.
3. **Approved**: Consensus achieved; implementation may proceed.
4. **Migrated to ADR**: Once the implementation lands and the decision is ratified in code, the RFC's conclusions are synthesized into a formal Architectural Decision Record (`docs/adr/`).
5. **Closed / Withdrawn**: Proposal rejected or superseded prior to implementation.

---

## 3. RFC Structure

An effective RFC must address:

1. **Motivation & User Stories**: What real-world user workflow or system bottleneck requires this change?
2. **Detailed Design**: Precise APIs, data models, IPC messages, and concurrency semantics.
3. **Performance & Frame Budget Impact**: How does the proposal affect the 1 MiB Lens frame arena budget, memory consumption, or thumbnail extraction throughput?
4. **Desktop Interoperability & Failure Modes**: How does the design interact with Wayland compositors, XDG portal daemons, or unreadable filesystems?
5. **Unresolved Questions**: Known unknowns explicitly listed for reviewer input.
