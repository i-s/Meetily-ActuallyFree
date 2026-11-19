# Issue 16 confirmed vocabulary implementation plan

> **For agentic workers:** Use superpowers:executing-plans; the user explicitly prohibits subagents and assigns independent review to the coordinator.

**Goal:** Save transcript corrections independently, then allow explicit «Запомнить» confirmation for a durable personal vocabulary consumed by ASR and the isolated subscription Codex CLI.

**Architecture:** Extend the existing global vocabulary repository, preserving its legacy read/ASR APIs. Atomic append is idempotent; replacement uses compare-and-swap against the loaded value to protect concurrent confirmation. A post-save confirmation component never mutates transcript rows. Native Codex commands load vocabulary themselves, with a separate approved-term origin and no client-provided learning.

**Tech Stack:** Rust/SQLite/Tauri, React, Bun, portable Rust test harnesses.

**Spec:** Live https://github.com/i-s/Meetily-ActuallyFree/issues/16, read 2026-11-18 (no comments), plus the user's explicit execution constraints.

## Global constraints

- Base `226ff989997e49a404ab8b684e6168f453f31c4f`; integrate local #13 `e40f6298424ad706e7e91ad434e7d69be6aba8e2` preserving histories. Integration commit `7d2f6b4cfd42b56c84aba4d4f0712b8c6e6c196a` is separate from implementation.
- Preserve Notes and selected-transcript APIs, revision CAS, speaker labels/timing and immutable history.
- No suggestions (#17), queue (#15), subagents, real services/credentials, packaging, push, releases or physical/user-data operations. No heavy builds without coordinator allocation.
- Whisper hints are bounded guidance, not guaranteed recognition or model training. Parakeet does not support hints. Physical same-audio and signed-in semantic qualification are deferred.

## Review focus

- Correction succeeds but projection refresh fails: confirmation cannot resave or roll back text.
- Concurrent settings replacement and confirmation: no lost term; stale writes fail visibly.
- Retry/double confirmation: one case-insensitive entry, first spelling preserved.
- Older meetings/version selection: learning is independent of row lifecycle and cannot overwrite selected text or labels.
- Long RU/EN vocabulary: no silent empty dictionary; visible storage/token limits, separate bounded Codex source.

### Task 1: Durable atomic vocabulary operations

Files: production `database/repositories/vocabulary.rs`, native API registrations, portable SQLite tests.
- [x] Add failing tests for RU/EN/abbreviations, duplicate retry, stale replacements, concurrent appends, overflow rollback and reopen persistence.
- [x] Run isolated SQLite test crate, confirm failures for absent CAS/confirmation APIs.
- [x] Add global compare-and-swap replacement and validated single-term confirmation without changing legacy reads. Refuse destructive unversioned replacement when loaded state is not supplied by the editor.
- [x] Run tests; preserve transcript/version integration tests unchanged.

### Task 2: Explicit post-correction confirmation and vocabulary editing

Files: `TranscriptVersions.tsx`, new confirmation component, `TranscriptSettings.tsx`, Bun component tests.
- [x] Add failing component tests: save only corrects; explicit confirmation alone appends; decline leaves correction; retries do not correct again; navigation/concurrent edit boundaries.
- [x] After successful native correction expose an empty user-entered term field and «Запомнить»/decline. No inferred or automatic term.
- [x] Use CAS for global settings replacement; allow editing/deleting vocabulary even with Parakeet selected. Explain storage, Whisper 224-token prefix truncation and engine limitations.
- [x] Run isolated affected Bun tests.

### Task 3: Approved vocabulary context and qualification

Files: `codex_cli/context.rs`, `service.rs`, `commands.rs`, portable fake-CLI tests and harness.
- [x] Add failing tests that approved vocabulary is a separate untrusted spelling-only source, not speech/Notes/evidence; reject oversized source rather than dropping it.
- [x] Add shared `ApprovedVocabulary` context and service entrypoint while retaining original callers. Native command loads committed global terms; future summary callers reuse the same source/builder. No legacy paid/provider fallback.
- [x] Exercise production fake CLI with mixed terms and verify synthetic response spelling, no real service calls. Verify existing live/repeated ASR vocabulary wiring with local source/consumer tests; do not claim model accuracy.
- [x] Update feature note/development map with data flow, tests, exact limitations and deferred gates; run diff checks and targeted verification, commit locally, report each AC and SHAs to coordinator using `t3_thread_send(mode:auto)`.


## Execution rulings

- Explicit user implementation brief governs execution; no additional design or
  subagent gate was introduced. Coordinator owns independent review.
- Legacy provenance cannot be inferred from `get_global`; migration adds a
  proof-bearing subset and durable revision. Only explicit review/confirmation
  approves terms for Codex; ASR legacy behavior is preserved.
- Settings Save requires both loaded value and durable revision. Snapshot
  identity is `vocabulary:global:<revision>`; retries do not increment it.
- #15's normal summary caller remains independently owned. Shared prompt helper
  and isolated fake-process coverage supplied here; normal summary integration
  and real-service/physical acceptance are deferred, not claimed passed.
- The existing settings regression fixture needed the complete new native read
  DTO. No production error suppression or changed test expectation was used.
