# Local vocabulary suggestions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. No delegates are authorized.

**Goal:** Offer bounded exact-source terms from a saved meeting without learning until explicit confirmation.

**Architecture:** A pure local token selector feeds an inline meeting-keyed panel. Additive read-only native context and source-guarded confirmation reuse the selected projection and #16 dictionary transaction.

**Tech Stack:** TypeScript/React, Bun isolated tests, Rust/SQLx SQLite portable harness.

**Spec:** `docs/superpowers/specs/2026-11-18-vocabulary-suggestions-design.md`

## Global Constraints

- Preserve #16 explicit approval, migrations01600+01601 and manual API signatures.
- Preserve #12/#14 Notes, selected transcript/version edits and history; no queue/provider changes.
- One worktree, no delegates/heavy builds or external publication.
- Scan/decline/close perform zero persistent writes.

## Review Focus

- Cyrillic casing and spelling variants must not overwrite approved spellings.
- Large or single huge transcript lines must not cause unbounded UI scanning.
- Held reads/confirmations after navigation or revision switch must not publish stale candidates.
- Missing legacy selection/read failure must leave manual confirmation usable.
- Concurrent writers must reject stale source atomically, without partial approval.

### Task 1: Source-guarded native context and confirmation

**Files:** `frontend/src-tauri/src/database/vocabulary_suggestions.rs`,
`frontend/src-tauri/src/database/repositories/vocabulary.rs`,
`frontend/src-tauri/src/api/api.rs`, `frontend/src-tauri/src/lib.rs`,
`tests/vocabulary/{src/lib.rs,src/suggestions.rs}`.

**Interfaces:** `vocabulary_suggestions::context(pool, meeting) -> SuggestionContext`
(no ensure/settings; SQL-bounded2,000rows/100,000Unicode scalars);
`SuggestionSource { meeting_id, version_id, revision, transcript_id }`;
`VocabularyRepository::confirm_suggestion(pool, term, source)` reuses update_global;
`api_get_vocabulary_suggestion_context(meeting_id)` and
`api_confirm_vocabulary_suggestion(meeting_id, version_id, revision, transcript_id, term)`.

- [x] Add SQLite RED tests through production command extraction: read zero total_changes, scan on missing selection no initialization, stale version/revision/row/term rejected, successful confirm preserves all non-vocabulary data, duplicate/first approved spelling retained, both approval migrations present.
- [x] Run warm portable cargo harness, observe missing interfaces.
- [x] Implement readonly selected read and guarded append in existing transaction, register additive commands.
- [x] Re-run full vocabulary harness; all inherited and new tests pass.

### Task 2: Bounded candidate selector and ordinary UI

**Files:** `frontend/src/lib/vocabulary-suggestions.ts`,
`frontend/src/components/meeting/VocabularySuggestions.tsx`,
`frontend/src/components/MeetingDetails/TranscriptVersions.tsx`,
`frontend/tests/vocabulary/{suggestions.test.ts,suggestions-flow.test.tsx}`.

**Interfaces:** pure `suggestVocabulary(segments, excluded) -> { candidates, truncated }`;
`VocabularySuggestions({ meetingId, versionId, revision })`, keyed by meeting.
Each candidate retains exact term, transcriptId, excerpt and optional seconds.

- [x] Write selector RED tests for RU/EN acronym, CamelCase, letter+digit, exact source, case dedup/exclusions, punctuation, bounds and empty input.
- [x] Write real TranscriptVersions component RED tests: no scan on mount, zero writes scan/decline/close, explicit accept only, double click, failure/retry, empty/manual path, held response and meeting/version/revision switch, no Notes/editor barrier invocation.
- [x] Run each Bun test separately, observe missing module/UI.
- [x] Implement conservative bounded selector and inline panel, read-only context loading, generation cancellation and keyed lifecycle, native guarded acceptance, transient suppression and manual field.
- [x] Run new tests and inherited isolated vocabulary/version/editor tests separately.

### Task 3: Qualification and local handoff

**Files:** `docs/ISSUE17_VOCABULARY_SUGGESTIONS.md`, `docs/DEVELOPMENT_MAP.md`.

- [x] Document exact live issue/AC, source flow, reuse, API contracts, bounds, test evidence and physical/real-service limits.
- [x] Review full diff for scope, hidden writes, stale async publication and existing ownership preservation; run git diff --check and focused tests again.
- [x] Commit locally with required attribution. Report exact parent/head/tree, per-AC evidence, mocks and deferred gates to coordinator via mode:auto; no push/PR/release.
