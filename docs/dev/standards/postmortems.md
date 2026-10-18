# Incident Postmortem Standards

This document establishes the blameless postmortem standard, timeline reconstruction methodology, and systemic defense analysis for incidents in Arca.

---

## 1. Blameless Culture & Intent

Incidents are opportunities to identify systemic fragility, missing test automation, and architectural gaps.
- **Strictly Blameless**: Postmortems must NEVER assign personal blame or attribute root causes to "human error" or "developer negligence".
- **Focus on Systemic Safeguards**: The central question is always: *Why did the compiler, automated test suite, or architecture allow an unsafe state to manifest?*

---

## 2. Postmortem Structure

Every postmortem recorded in `docs/dev/postmortems/` must follow this structure:

1. **Executive Summary**: High-level impact (affected users, file integrity status, crash frequency, platform environments).
2. **Timeline of Events (UTC)**:
   - $T_0$: Introduction of defect (commit, dependency bump, Optics binding update).
   - $T_{\text{impact}}$: First manifestation in release or test run.
   - $T_{\text{detect}}$: Detection by monitoring, automated tests, or user report.
   - $T_{\text{mitigate}}$: Mitigation or workaround applied.
   - $T_{\text{resolve}}$: Permanent fix verified with automated regression test.
3. **Detection Gaps**: Why did unit tests in `arca-engine`, headless frame tests in `arca-ui`, or CI fail to detect this before release?
4. **Trigger & Underlying Root Cause**: Detailed technical analysis of the state-machine failure, FFI boundary issue, arena budget exhaustion, or filesystem race condition.
5. **Action Items & Defense-in-Depth**:
   - Preventative actions (e.g., adding a compiler typestate, Cargo dependency firewall, or bounded arena slice).
   - Detection actions (e.g., adding a specific regression test in `tests::*`).
   - Each action item must have an owner and a verifiable completion criterion.
