# Issue #19 audio cleanup implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Work is inline; no agents or approval round per full-send instruction.

**Goal:** Show saved audio usage and explicitly delete selected old audio while retaining every text/queue artifact.

**Architecture:** Extend normal library UI with audio inventory and review/confirm. Native process-owned one-shot plan snapshots exact file identities. At execution acquire existing offline and recording lifecycle admission, then a SQLite immediate transaction; revalidate ownership, job/version state and files before each unlink. No recursive removal, DB content deletion, queue mutation or metadata rewriting.

**Tech Stack:** Rust std filesystem, SQLite/sqlx, Tauri, React/Bun portable tests.

**Spec:** Live i-s/Meetily-ActuallyFree #19 (2026-11-18, no comments); docs/ISSUE19_AUDIO_CLEANUP.md.

## Global constraints
- Accepted base 79c0c1107bbfb8d62d2a4ddbd4daedd137332cbb; inspect d7ed5d5 queue read-only; reconcile #18 convergence when supplied, do not recreate merge.
- Disposable generated fixtures only; no user files, models, packaging, push, Actions, Mac or heavy build.
- Preserve transcripts, corrections/versions, Notes, summaries, tasks, queue results/checkpoints and unrelated recordings.
- Explicit scoped consent; no selected-by-default files. Thirty days is an application suggestion, never a user-approved storage limit.

## Review focus
- Queue state changes between preview and confirm: immediate transaction recheck and protect pending/inflight jobs.
- Folder aliases, links, unexpected audio and checkpoints: refuse ambiguous scope, no recursive deletion.
- Partial unlink failure or changed files: return per-file outcomes and only observed successful unlink bytes, no disk-free promise.
- Concurrent UI/reload/double confirm: bounded expiring native single-use plans, generation guards and fresh retry review.
- Deleted audio: normal audio resolution reports unavailable; retained text/search/export are independent.

### Task 1: Native conservative inventory and deletion
- [x] Add generated fixture tests for inventory, explicit plan scope, changed file, symlink/shared folder, recovery/job transitions and partial failure.
- [x] Implement audio_cleanup.rs with inventory/prepare/execute and native command wrappers; register commands.
- [x] Use shared offline/lifecycle locks and SQLite immediate fence; verify narrow production module in portable harness.
- [x] Commit with required attribution.

### Task 2: Normal library review/consent UI
- [x] Add component tests: total/proposal rule, empty default selection, exact confirmation list/preserved materials, no delete before consent, partial errors and retry.
- [x] Implement AudioStorage panel/dialog in normal meetings page; refresh workspace material observations after results.
- [x] Run isolated Bun tests and lightweight static checks only.
- [x] Commit with required attribution.

### Task 3: Integration and handoff
- [x] Reconcile committed #18 convergence head when supplied, retain its canonical summary bridge.
- [x] Document AC/evidence/limits and architecture map seam.
- [ ] Verify diff, test evidence, attributed local commits and clean worktree; send exact SHAs and integration seam to coordinator.

## Inline execution ledger — 2026-11-19

Native: 12 generated-fixture tests pass; frontend: all 54 isolated files pass.
Author self-review only under explicit no-agents instruction. No heavy/native/Next
build or Mac qualification. Ruling: reuse existing offline + lifecycle locks and
SQLite immediate reservation rather than add a parallel job-lease subsystem;
this fences current accepted native readers and durable queue writers without
changing queue artifacts. Cost/limit: external filesystem actors are not atomic
participants, and capture-start latency during admitted deletion is unqualified.
Ruling: protect all pending/deferred/waiting queue audio and ambiguous legacy
folders; this intentionally retains more storage rather than guessing ownership.
Task 3 convergence waits for #18's supplied committed head; do not duplicate its
accepted-history merge or canonical-summary bridge.

## R1 review ledger — 2026-11-19

Standalone implementation committed at 15f8ca70288367482e89f95fd75c8715a614ad85.
P1: actual Stop saver + extracted production lock helpers reproduced a cycle;
native finalized cleanup now uses its existing lifecycle owner rather than
waiting for offline. Independent cleanup/recovery still use offline, and both
cleanup owners reuse the same file proofs. Shutdown 45/45 and full recovery 83/83.
P2: mounted 23s/1.5x focus/replacement red tests now pass; observe path + native
revision without disposing unchanged elements, scope native events, fence stale
checks/callbacks and apply selected rate to newly created elements. Playback 3/3,
isolated frontend 54/54 files, narrow native 14/14.
Diarization: aborting an actual Rust command waiter exposed a guard/reader lifetime
gap; native-task ownership fixes it. This is not evidence that Tauri closes/reloads
actually cancel commands; that transport behavior and real media remain unqualified.
Ruling: keep finalization-specific internal admission, not a new lease subsystem;
independent callers must never reuse the lifecycle-owned helper without that owner.
Bridge 2d388cf15adb95f2d37ab1e9ff17b82df723e328 is supplied and accepted upstream;
standalone R1 must be rechecked/accepted before the separate history-preserving
integration commit and combined regression. No owner/dependency relay is needed.


## Combined accepted bridge ledger — 2026-11-19

Exact parents: standalone R1 9cd59f429a50506cd38da45a7c55792a10c734eb and
accepted bridge 2d388cf15adb95f2d37ab1e9ff17b82df723e328. Three conflicts
resolved with native task + inner original-folder identity policy, version audio
revision + vocabulary UI, and library audio revision + canonical subscriptions.
Ruling: fix unchanged Notes readiness in the shared store rather than weaken
canonical tuple validation or republish after cleanup — the mounted normal
navigation test proved an unnecessary save invalidated the summary; cost if
wrong is a missed Notes edit, bounded by explicit changed-conversion/formatting
and full recovery/CAS frontend regressions. Store red (19 vs 18 writes) → green;
mounted cleanup + both exports green. No delegates used, per explicit constraint.
Fresh results: frontend 73/73 files; cleanup native 43; Stop/shutdown 48; targeted
recovery 17 (68 filtered); native queue 4 (50 filtered); queue/projection 28; guarded
Stop 15; start authority 39, all passing. No ignored model/acoustic tests ran.
See docs/ISSUE19_BRIDGE_INTEGRATION.md for exact boundaries and remaining limits.
