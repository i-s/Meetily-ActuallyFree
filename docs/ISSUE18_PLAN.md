# Issue #18 library, search and Markdown implementation plan

**Goal:** Implement live issue **[macOS 14/16] Библиотека, поиск и Markdown** on accepted base `f6b9b2af77aada0543fb0c565034520fba815482` through existing ordinary callers.

**Architecture:** Reuse `/meetings`, global search, meeting details/audio and native save picker. A shared native library repository supplies material availability, Unicode text search and a transactional export snapshot. Both Markdown export entrypoints consume one shared source-labelled renderer, flushing revisioned Notes before capture. Snapshot time is explicit; subsequent edits do not rewrite captured bytes.

**Approaches:** Extending the fork's existing paths preserves ordinary navigation and data; adding a parallel library would leave the existing broken export reachable and duplicate ownership. Independent frontend reads cannot consistently capture a selected transcript plus summary during #14 selection. Use a native SQLite read transaction instead. No branch merge is required: #14's canonical selected projection remains `transcripts`, and selection archives/clears the former summary atomically.

**Constraints:** No data migration/deletion; no #19 work; no network/model invocation; no subagents, Mac/private operations, push/PR/Actions/DMG/release. Heavy verification waits for coordinator slot. Notes are user-authored thoughts, not recognized speech or established agreements. Original data stays untouched.

## Tasks

- [x] Add failing shared-export caller tests: Notes-only, empty/legacy/large transcript, current title/summary, missing audio, Notes conflicts, failed reads/writes, caller navigation while loading, safe exact picker destination.
- [x] Implement native `library` repository: transaction snapshot and material availability, Unicode matching with bounded batch scans/results. Register normal commands and connect existing global search without Q&A changes. Add portable native tests against actual repository SQL.
- [x] Replace duplicate detail/batch Markdown builders with shared snapshot renderer and explicit source/absence metadata. Add complete/Notes choices independent of summary availability. Preserve other formats and action export.
- [x] Show material states in existing library; make library search use text hits and route Notes/transcript matches. Guard stale responses/navigation; target Notes explicitly.
- [x] Inspect #14 read-only, define compatibility tests for canonical corrections/selection and summary clearing; do not merge it.
- [x] Run focused isolated frontend tests, portable native tests when permitted, standalone types/full isolated suite in coordinator slot. Document evidence/limits and interfaces, commit locally, notify coordinator.

## Review focus

- Cyrillic mixed case and literal `%`, `_`, backslash searches must match visible text, not SQL wildcards/editor JSON.
- Empty/no-summary/audio-deleted meetings remain navigable/exportable with explicit absences.
- Notes conversion/persistence failures cannot silently export stale Notes.
- Native snapshot must not combine pre-selection transcript with post-selection summary; exports use captured data even when later edits occur.
- Stale meeting requests and save-picker cancellation/failure cannot navigate to the wrong meeting or claim success.

Verification disposition: software behavior and focused callers implemented/tested; bounded Next at ef167805 compiled but timed out (124) during lint/type checking, so full production export and physical Mac qualification remain incomplete. R1 visible Notes search is fixed with parser-backed legacy fallback and exact normal-caller snippet tests; see ISSUE18_LIBRARY.md.
