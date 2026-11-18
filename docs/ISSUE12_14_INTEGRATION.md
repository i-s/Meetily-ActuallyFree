# Combined Notes and transcript-version integration (#12 + #14)

## Ownership and preparation barrier

The library's Notes target stays `meeting:<id>` across transcript selection,
correction and projection refresh. `MeetingDocument` mounts the real
`MeetingNotesEditor`, backed by `notes-store`; it does not create a second Notes
autosaver. Transcript refresh neither reloads the page nor replaces the Notes
component's key or content. Notes remain `source: user_notes`, never speech or a
transcript snapshot.

Before the actual `TranscriptVersions` selector or correction invokes native SQL,
the page temporarily disables editing and drains its registered document barrier:

1. `NotesEditor` journals cloned rich JSON synchronously and converts that exact
   document asynchronously. Its flush observes the newest conversion, propagates
   failure, and does not publish an older result after an editable transition.
2. `MeetingNotesEditor` waits for conversion, then calls `notesForContext(target)`.
   The store owns serialized writes, revision CAS and committed native readback.
   Pending conversion, save failure, quota lock, paused recovery and a competing
   writer block the operation; the barrier never grants replacement consent.
3. Summary conversion and the existing summary autosaver must also complete.
   Only then can native selection/correction proceed. A summary backend operation
   independently blocks mutation in SQL while pending.

On failure the selector releases the temporary editing lock. Existing Notes Retry,
explicit reviewed replacement, and read-only recovery controls remain responsible
for resolving the failure. Choosing a transcript is not permission to merge two
recording/meeting recovery drafts or bypass quota recovery.

## Native save and revision boundaries

Normal save and idempotent recovery retain both transaction responsibilities:
`transcript_versions::ensure` establishes the selected live snapshot, and
`notes_document::attach` attaches recording-folder Notes before commit. Later
writes through `recording:<folder>` resolve to the meeting. Attachment retries do
not overwrite later library edits. Distinct local recovery journals stay distinct
until explicitly reviewed; recording aliases are not transcript identities.

Notes revision CAS is independent of transcript selection revision CAS. Selected
transcript snapshots retain text, timing, speaker links and capture-source maps;
Notes JSON/Markdown are not copied into them. Summary archiving on selection does
not archive or clear Notes. Existing #9 recovery and source/provenance ownership
remain unchanged.

## Qualification and limits

`frontend/tests/transcript-versions/page-editors.test.tsx` drives the real page,
Notes component/store/editor wrapper, version selector and correction controls.
BlockNote's engine/view and native transport are mocked. Ten cases cover formatted
RU/EN edits followed immediately by select/correct before 800 ms, held conversion,
held native write, storage failure/Retry, competing writer with explicit reviewed
replacement, pending/failed summary, conversion failure, quota/read-only recovery,
and distinct alias journals. These are software-boundary tests, not physical UI
or filesystem durability qualification.

`frontend/tests/notes/editor-draft.test.tsx` additionally checks immediate rich JSON
publication and newest-conversion ordering across editable transitions. Run all
mock-heavy frontend files in separate Bun invocations.

`frontend/tests/native/notes-versions.rs`, included by
`frontend/scripts/test-recovery-database-portable.py`, executes production
repositories with real SQLite and the complete checked-in migration sequence.
It covers normal and recovered save, candidate creation/selection/correction,
selected snapshot metadata/source maps, independent Notes CAS, pending-summary
rollback, and closing/reopening a synthetic on-disk database. The wider harness
also uses real FFmpeg and production recovery bodies; Tauri/device boundaries are
stubbed. A fresh store or database reopen is not a physical Mac restart.

Physical Mac WebView/BlockNote interaction, power loss, recording/ASR continuity,
real models and same-audio RU/EN accuracy remain unqualified. No model/audio test,
installer, local Mac installation or release is established by these tests.

The separate #10/#11 branch is not merged here. Its accepted identity-policy
cross-gate must later exercise original recording metadata before temporary WAV
conversion, malformed metadata and generated composite labels through candidate
creation and selection. Shared-room identity remains outside MVP. No automatic
provider substitution is introduced.
