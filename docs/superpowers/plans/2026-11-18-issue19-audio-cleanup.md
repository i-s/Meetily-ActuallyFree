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
- [ ] Commit with required attribution.

### Task 2: Normal library review/consent UI
- [x] Add component tests: total/proposal rule, empty default selection, exact confirmation list/preserved materials, no delete before consent, partial errors and retry.
- [x] Implement AudioStorage panel/dialog in normal meetings page; refresh workspace material observations after results.
- [x] Run isolated Bun tests and lightweight static checks only.
- [ ] Commit with required attribution.

### Task 3: Integration and handoff
- [ ] Reconcile committed #18 convergence head when supplied, retain its canonical summary bridge.
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
