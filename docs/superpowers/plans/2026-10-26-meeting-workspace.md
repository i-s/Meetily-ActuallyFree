# Meeting workspace implementation plan

> **For agentic workers:** Use executing-plans for the assigned task. Coordinate shared interfaces before editing another task's files.

**Goal:** Implement agreed issue #3: durable per-recording notes/chat, immutable summary inputs and visible Ask AI context.
**Architecture:** A native-owned meeting ID exists from Start. SQLite remains authoritative for notes, live transcripts, saved chat and summary snapshots. Shared frontend notes/chat components use the same ID during and after capture.
**Tech Stack:** Tauri/Rust/SQLx SQLite, React/Next/TypeScript, existing providers and BlockNote editor.
**Spec:** ../specs/2026-10-26-meeting-workspace.md

## Global constraints
- One Start/Stop = one meeting; Pause retains ID; next Start differs.
- Notes survive silence, <10 seconds, Stop, restart and crash recovery.
- Summary snapshots all transcript + complete notes, never chat; editing does not regenerate.
- Ask AI defaults to last 10,000 Unicode characters of transcript; configurable N or all. Notes are unbounded by this transcript limit.
- Preserve Speakers during recording; tasks belong inside Summary. No legacy-data migration, new tools/search, merge, release, or version bump.

## Review focus
- Late autosave/chat completion after navigation must never update another meeting.
- Start/Stop and crash recovery must reuse the durable ID, even with no speech.
- Snapshot and provider failure must preserve old summary and saved notes.
- Unicode/context cut inside a turn must retain honest turn timing and exact character offsets.
- Deleted meetings must not be recreated by late background completions.

### Task 1: Recording identity and persistence
- [x] Add regression tests for two starts, pause, silent/short stop and recovered identity; observe baseline failures.
- [x] Update native recording lifecycle and existing transcript save/recovery; create durable meeting before editing and expose `meetingId` in RecordingStateContext (`meeting_id` in native snapshots). Persist transcript updates to this ID. Reuse ID on Stop/recovery; remove under-10-second discard.
- [x] Verify lifecycle tests including last pending notes flush before Stop; coordinate frontend flush helper with Task 3.

### Task 2: Persistent chat and context
- [x] Add native storage/context tests for 10,000/default, configurable N/all, Unicode cut, full notes, restart/history and cascading delete.
- [x] Reuse live_assistant provider resolution; implement persisted per-meeting chat and context metadata API, with backend-owned history and request lifetime. Provide typed command contracts to Task 3 immediately.
- [x] Persist user request before inference and terminal answer/error after inference; preserve range with every answer and recover interrupted requests visibly.
- [x] Verify real SQL/reopen/delete, prompt source labeling and provider-fixture behavior.

### Task 3: Shared notes/chat panels
- [x] Add tests for meeting-scoped saves/navigation, immediate typing then generate/stop, persistent chat/range display and tab layout.
- [x] Reuse NotesEditor and ChatThread; share meeting notes editor and durable saver between LivePanel and MeetingDocument. Expose `flushMeetingNotes(meetingId): Promise<void>` for Stop and Summary. Complete notes are saved without truncation. Errors remain visible with retry and per-ID draft recovery.
- [x] LivePanel consumes RecordingStateContext.meetingId; saved panel receives existing meetingId. Tabs Meeting notes/Summary/Ask AI; Speakers also retained live, tasks in Summary.
- [x] Integrate context settings and transcript boundary through Task 2 contracts; verify navigation and source/range labels.

### Task 4: Immutable summary snapshot
- [x] Add SQL/prompt tests for full transcript/notes snapshot, editing during generation and chat exclusion.
- [x] Snapshot all persisted transcript and notes transactionally in existing summary command before worker dispatch. Pass immutable data through provider prompt and persist notes snapshot metadata for stale indicator. Keep prior snapshot associated with prior summary on failed regeneration.
- [x] Flush frontend notes before Generate/Regenerate, show changed-notes notice without auto-generation, allow notes-only summary.
- [x] Verify generation/recovery hooks and native fixtures.

### Task 5: Integration and qualification
- [x] Run existing frontend baseline groups separately, targeted regressions, TypeScript and production build; run available native/portable tests against real repository modules.
- [x] Independently review lifecycle boundaries and acceptance coverage, fix concrete findings and rerun affected checks.
- [x] Update DEVELOPMENT_MAP and feature qualification note with exact evidence and limitations. Push a reviewable stacked draft PR based on PR #2, without merging or releasing.
