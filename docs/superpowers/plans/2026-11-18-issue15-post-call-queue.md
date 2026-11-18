# Issue 15 automatic post-call queue implementation plan

> **For agentic workers:** use superpowers:executing-plans. No subagents; coordinator owns independent review.

**Goal:** Automatically retain and process saved calls through local refinement and subscription Codex summaries, without changing selected text or personal Notes.

**Architecture:** SQLite owns one automatic job per saved meeting, an epoch-fenced claim, reusable candidate identity and version/revision-bound summary artifacts. One native scheduler runs outside WebView lifetime. Save/recovery transactions enqueue; recording startup takes a priority reservation, cancels the owned background operation and waits for its inference boundary before live model initialization. No power-source gate.

**Tech Stack:** Existing Rust/Tokio/sqlx/SQLite, Tauri, React/Bun, #13 isolated Codex subprocess adapter.

**Spec:** Live https://github.com/i-s/Meetily-ActuallyFree/issues/15 and explicit coordinator brief. Base 226ff989997e49a404ab8b684e6168f453f31c4f; #13 integration a203a83 (e40f629, reviewed code 7f017db).

## Global constraints
- No paid/generic provider fallback, fake live session, automatic candidate selection, Notes overwrite, user audio/data or real CLI service calls.
- No heavy desktop/production builds without allocation; portable synthetic tests only here.
- Keep original Notes and selected-version APIs; #10 policy resolution belongs in shared native diarization path; neutral in-person source labels remain mandatory.
- Keep live/refined/corrected snapshots and source provenance. Generated summaries are separate artifacts tied to meeting + selected version + transcript revision + Notes revision; stale work cannot publish.

## Approaches
Frontend orchestration was rejected because WebViews can disappear and duplicate work. A separate external daemon was rejected as unnecessary lifecycle/packaging scope. Native process-owned scheduler + SQLite persistence builds on existing worker/model ownership without adding a service.

## Review focus
- Crash after candidate creation or publication: deterministic candidate identity, persisted artifacts, claim fencing and idempotent transaction.
- Recording begins during background reservation: startup priority reservation fences the scheduler before cancellation and model initialization.
- Notes or transcript changes during CLI: compare complete revision tuple again in publication transaction; retain old artifacts.
- CLI offline/limit/error: retain local candidate, block summary until explicit retry, never spin or fallback.
- Save reply lost/recovery replay: enqueue in both branches of existing idempotent save transaction.

### Task 1: Durable job repository and synthetic SQLite tests
Files: new migration, `post_call/store.rs`, portable runner and native queue tests; modify transcript save repository.
- [ ] Add failing tests for transactional save/recovery enqueue, claim fencing, restart, defer/retry, deterministic candidate reuse and stale publication.
- [ ] Implement schema/repository and connect existing normal/recovery save transactions.
- [ ] Run portable tests offline, one cargo job; commit with implementation when integrated.

### Task 2: Native scheduler and recording priority
Files: `post_call/mod.rs`, `post_call/worker.rs`, existing retranscription, recording commands, database startup/activation, lib registration.
Interfaces: queue uses existing local ASR/speaker worker through an awaited saved-meeting entrypoint; queue has sole ownership of its cancellation token. Recording startup holds a reservation until initialization completes.
- [ ] Exercise persisted transitions with synthetic worker outcomes and cancellation.
- [ ] Add deterministic candidate reservation and awaited worker reuse; scheduler recovers active claims only on native startup.
- [ ] Fence starts, drain owned background inference before capture initialization, retain completed stages and restart interrupted stage.

### Task 3: Saved-meeting Codex summary + visible queue UI
Files: `post_call/summary.rs`, queue commands and panel mounted in shared layout; tests.
Interfaces: reuse `codex_cli::cli::generate` and isolated auth home directly; no live request DTO or generic provider resolver.
- [ ] Add failing synthetic summary tests covering four sections, exact transcript citations, evidence-backed owner/deadline and Notes separation.
- [ ] Implement bounded prompt, validated structured response and atomic revision-fenced publication.
- [ ] Show durable stages, candidate review link, stored summary results, defer/resume/cancel/retry and errors. Do not replace existing summary editor.

### Task 4: Verification and handoff
- [ ] Run portable native tests, isolated affected frontend tests and standalone typecheck if available, no heavy build.
- [ ] Update development map and feature note with interfaces, test evidence and deferred real Mac/model/signed-in qualification.
- [ ] Commit locally; send coordinator each AC, base/integration/head SHA, worktree, scope/tests/limits and integration needs, then end turn.
