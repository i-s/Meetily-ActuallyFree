# Issue #17: local vocabulary suggestions

## Intent and approved scope

Implement live issue https://github.com/i-s/Meetily-ActuallyFree/issues/17,
**[macOS 13/16] Предложения пополнить словарь** (read 2026-11-18, zero comments),
on accepted base ff957416929742730e7d26e4f0de3ee4dfbd59ed. The user approved
bounded local RU/EN suggestions in saved meetings, with exact committed-source
spelling/excerpt, transient rejection, explicit #16 approval, stale-context guards,
and no AI/queue dependency.

## Approaches

1. Subscription Codex suggestions: can infer ordinary names/terms, but needs
   service availability, provider qualification and shared #15 scheduling.
2. Conservative deterministic local candidates: no service, model, queue or
   inference; explainable source spelling, but deliberately incomplete coverage.
   **Selected** under the approved conditional-AI scope.
3. Suggestions inferred from arbitrary edits: rejected; conflates #16 manual
   corrections with automatic learning and cannot prove intended spelling.

## Design

Meeting details → Transcript versions → “Предложить термины” scans only the
selected committed transcript, on request. Recognize acronyms (RU/EN), internal
CamelCase and letter+digit technical tokens; do not infer identities, normalize
spellings, split compounds, or claim correctness. Display at most five exact
spellings with a bounded excerpt and transcript line/time. Show bounds/incomplete
coverage and keep manual vocabulary entry available on empty/error.

Read at most 100,000 UTF-16 code units and 2,000 rows; retain at most 100 distinct
candidates. Existing global/meeting terms and in-panel suppressed keys are
excluded case-insensitively. No timers, notifications, focus stealing, recording
hooks, Notes reads/writes or background scan. Dismissals/close are memory-only;
no durable rejection history is promised. Closing suppresses all displayed terms
for this mounted meeting panel. Switching meeting remounts it; revision/version
changes clear candidates and invalidate in-flight reads. A fresh scan re-reads
committed context. Sparse candidates are acceptable, not a complete term finder.

Native suggestion-specific read-only context getter caps row identities and SQL-substrings
text to100,000 Unicode scalars+lookahead before Rust/IPC materialization; returns no
settings/source_map. Frontend independently caps100,000 UTF-16 code units. The
generic #14 snapshot stays intact. The getter does not call lazy version initialization. The
existing version browser initializes legacy meetings normally. Additive explicit
suggestion confirmation checks selected version/revision and exact source row
inside #16's existing BEGIN IMMEDIATE dictionary transaction, then uses the same
normalization, capacity limit, first-approved-spelling and approval-revision logic.
No new dictionary or migration. Manual #16 API remains unchanged. Concurrent
version changes are rejected before any dictionary write; post-commit changes do
not revoke a confirmation the user already made.

## Constraints and anti-goals

Preserve Notes ownership, transcript/history/source data and summary behavior.
Never promote suggestions automatically or reapprove legacy terms during scanning.
No Codex invocation, API fallback, credentials, audio, models or #15 queue changes.
One assigned worktree; local attributed commits only; no delegates, heavy builds,
Mac operations, packaging, Actions, push, PR, merge, release or closure.

## Verification

RED/GREEN pure candidate tests; real component tests through TranscriptVersions
with mocked Tauri transport, held read/confirm promises and real controls; isolated
SQLite tests compiling actual repositories/native command bodies for zero writes,
atomic stale rejection, dedup/approval, reopen and unchanged Notes/history. Re-run
focused inherited #16 and #12/#14 frontend suites separately. No full desktop/Next
build. Physical M4 Pro/M1 Pro usability, recording/Notes responsiveness and real
RU/EN transcript usefulness remain unqualified; synthetic tests do not prove ASR
accuracy or real-service behavior (no service is used here).
