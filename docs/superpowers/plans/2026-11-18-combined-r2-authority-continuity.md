# Combined R2 authority continuity plan

> Execute inline with superpowers:executing-plans; no delegates.

**Goal:** Correct the three R2 findings on `044a43a` while retaining manual controls and source qualification gates.
**Spec:** User's Combined R2 request; `docs/ISSUE10_11_INTEGRATION.md`.
**Architecture:** Native RAII startup admission owns sticky Stop eligibility before capture awaits and transfers it to the committed lease. Live Settings changes do not restart a healthy monitor; real disable/pause/restart still revokes authority. Distinct routed manual Stop intents have opaque IDs and a frozen native generation, separate from automated expected leases; native generation checks prevent delayed manual actions from stopping later takes.
**Tech stack:** Existing Rust/Tokio authorization, React/TypeScript Controls, Bun/Python generated native bridge.

## Constraints

Same worktree; #12/#14 and `226ff98` remain separate. No heavy build, DMG, Actions, push, delegates or Mac operations. No qualification bypass or new recording/save engine.

## Approach choices

Revalidating current Active at commit loses history; retain provisional native authority instead. Suppressing `suspend` alone would preserve genuinely revoked state; avoid unnecessary restart while retaining actual revocation paths. A single first-wins Stop slot loses explicit manual actions; use bounded distinct intent records, exact target generations and native final guards instead of unbound delayed manual fallback.

## Tasks

- [x] Reproduce admitted source/global off/on before lease publication for both sources. Add failure/abandonment cleanup controls and valid-End-during-start control.
- [x] Implement RAII admitted Stop authority and transfer its sticky flags at native commit; verify old generation/Unknown/keep/End controls.
- [x] Extend native fixture through actual Settings command, full start_monitor function and lifecycle publication (stub only disk/worker scheduling/scanning transport). Reproduce unrelated/Discord-only changes revoking Zoom A.
- [x] Restart only when enabling/resuming detection, not for ordinary live settings changes; test inverse source, actual source/global disable and later End.
- [x] Reproduce guarded A plus distinct manual B while false/remount, successful A, manual retry and delayed next-generation action.
- [x] Add bounded persisted Stop intents with IDs; freeze manual target once at action, retain immutable generation through sync/remount, and check it atomically at native Stop without requiring source/Labs permission.
- [x] Run combined bridge, isolated frontend, native Stop/start/lifecycle suites and standalone TypeScript sequentially. No Next/full native build.
- [x] Update architecture/evidence, author self-review (delegates prohibited), commit and send exact SHA/disposition/logs/limits to coordinator.

## Review focus

- Off/on or qualification/competing transitions during admitted startup cannot be erased at commit.
- Failed/cancelled startup releases only its own provisional authority.
- Settings command publication/restart, not a helper-only approximation, exercises the Zoom suspension path.
- Manual intent is never rebound after async directory/state/transport operations, including remount and response loss.
- Same ID/guarded lease delivery deduplicates; distinct explicit manual actions do not disappear behind stale automation.
- Queue/capacity/storage failures fail closed, never into a delayed unguarded Stop.


## Execution record — 2026-11-18

All work executed inline in the existing `meetily/issue-10-batch` worktree on
`044a43aa1753f4f8c77f2afb3ee90315da0b1074`. No separate spec was invented:
the user's three-finding request is binding; the linked integration note owns
architecture and qualification limits.

- Ruling: use this tracked execution record rather than recreate a skill scratch
  workspace after compaction — preserves the existing task/evidence identity;
  cost is less granular task-script bookkeeping, not weaker test coverage.
- Ruling: bounded queue storage/capacity failure is fail-closed, without R1's
  transient event fallback — no delayed unbound manual Stop; cost is requiring
  explicit retry/direct manual button if sessionStorage is unavailable.
- Startup and actual Settings red: 41 pass / 6 fail / 426 assertions. Runtime
  failures reproduced both-source source/Labs off/on resurrection and unrelated
  Settings restart revocation. RAII admission and restart gating fix those paths.
- Manual Controls red: 3 pass / 1 fail. Old 044a43a Controls + launcher replay
  through native bridge: 0 pass / 1 fail (59 filtered), 4 assertions, stale-A/manual-B
  expected 2 saves but got 1. Exact original components, no compile/import failure.
- Final bridge: 60 pass / 0 fail / 513 assertions; focused Controls 4 / 0 / 26.
- Portable native Stop: 15 pass; start authority/source/settings: 35 pass; lifecycle/
  provenance: 38 pass. No ignored model/audio tests were used as evidence.
- Full isolated frontend: 43/43 files, 219 tests passed; standalone TypeScript exit 0.
  First rerun found an entrypoint mock counting wakeups rather than consuming
  bound Stop intent; corrected native-state fixture/consumer, focused 2 pass / 28
  assertions, then full rerun green. Injected transport/model/store failure logging
  is expected in negative tests, not a claim of pristine stderr.
- Initial shell invocations failed to find bunx/node; final checks used installed
  Bun directly and the existing portable Rust toolchain. No new dependency install.
- Final review: author self-review (delegates explicitly prohibited). Reviewed
  provisional/committed mutex ownership, Drop cleanup, Settings lock order and live
  worker reads, argument forwarding, full-lease vs generation claim, queue bounds,
  same-ID receipts, remount/late delivery and explicit retry. No unresolved scoped
  correctness findings or deferred minors. Not an independent reviewer approval.
- Verification logs: `/tmp/issue1011-r2-{native-red,manual-red,old-controls-red,
  combined-final,manual-green,entrypoints-final,frontend-final,stop-final,start-final,
  lifecycle-final,tsc}.log`. Commands: portable Python scripts for Stop/start/lifecycle
  and meeting-native integration; Bun `frontend/scripts/test-isolated.mjs`; Bun
  TypeScript `tsc --project frontend/tsconfig.json --noEmit`. Rust invocations
  serialized, offline, using repository lock dependencies.
- Limits: synthetic qualified source/device/save fixtures only. No real desktop
  Tauri IPC/macOS SDK compile, audio/model/acoustic/physical qualification or install.
  No heavy build/Next/DMG/Actions/push/delegate/Mac/private-data operations.
  Real source classes remain unqualified, physical UNKNOWN/#6 remain open.
  #12/#14 at 226ff98 were not imported. Local commit and coordinator handoff only.
