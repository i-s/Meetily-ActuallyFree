# Issue #12 — persistent user Notes

Live issue title, read together with its empty comments list on 2026-11-18:
**[macOS 08/16] Notes во время встречи**.
Software base: `4269de67c6747b2f5fce72596826c05dadbb3e1c`.
This is software implementation/portable qualification, not an installed or
physically qualified Mac build. Issue #9's audio/session recovery remains the
base; no packaging, publication, model invocation or physical operation is added.

## Entry points and ownership

- During recording: the existing **Notes** tab beside the transcript in
  `components/recording/LivePanel.tsx`. It stays mounted across tab switches;
  incoming ASR phrases do not replace or remount its document.
- Later: Home/library card → meeting details → **Your notes** in
  `components/meeting/MeetingDocument.tsx`. Both use
  `components/meeting/MeetingNotesEditor.tsx`, including errors and Retry.
- The native `get_meeting_folder_path` provides the unique recording identity.
  Never use title, speaker identity, current browser meeting, or one global
  browser-storage slot to associate notes. `recording:<folder>` resolves to
  `meeting:<id>` once the meeting's folder is saved or recovered.
- `notes-store.ts` owns a serialized writer per target outside React. The editor
  synchronously publishes an exact cloned BlockNote JSON draft before asynchronous
  Markdown conversion. Only the newest conversion may publish its Markdown.
  The latest complete document is debounced by 400 ms; this is not a capture,
  inference or transcript queue. Unmount/pagehide attempts to flush but is not the
  durability guarantee: the synchronous journal and native committed rows are.

The existing fork implementation `d8593b9` on
`origin/feat/meeting-notes-persistent-chat` was reviewed. Its editor JSON-before-
conversion/readiness safeguards and editor regression are reused. Its global
persistence/lifecycle assumptions were not cherry-picked over #9's accepted base.
No chat implementation or new provider was imported.

## Persistence and recovery

Migration `20261118001200_recording_notes.sql` adds an independent revision to
`meeting_notes` and creates `recording_notes` keyed by folder. Existing meeting
Notes remain intact at revision 0; meetings without Notes start empty.
Native writes take SQLite's writer lock before resolving a target, use optimistic
revision matching, and acknowledge only committed writes. An identical retry of a
lost response is idempotent. Different content at a stale revision is rejected,
not silently substituted or merged.

`TranscriptsRepository::save_with_recovery` attaches the matching folder's Notes
inside the normal save/recovery transaction. The same operation runs on the
idempotent existing-meeting recovery path. Normal saves with a native recording
folder also reuse its existing meeting on a lost-response retry, preserving the
first committed transcript and Notes instead of creating a duplicate. Rollback keeps the recording draft;
retry does not replace post-call Notes. Late live writes resolve to the committed
meeting row. Neither ASR segments nor summary generation writes these tables.
Source audio/checkpoint cleanup does not remove Notes.

Each edit is also journaled synchronously at `af-notes-v1:<target>` in localStorage,
with exact JSON, last complete Markdown, base revision and conversion-pending
flag. The native `api_notes_recording_target` lets a library view discover the
last-keystroke recording journal after Stop, reload, app restart or recovery.
A recovered incomplete conversion is redone before text is used for AI/export.
Successful persistence removes only the journal that was actually consumed;
another editor's newer alias journal is not deleted.

The UI distinguishes Loading, Unsaved changes, Saving, Saved locally and errors.
Failed drafts remain available, with Retry even without another edit. A revision
conflict offers the saved text for comparison and an explicit replacement of that
reviewed revision with the user's draft; a newer competing write still rejects
that replacement. Failed loads never manufacture an editable empty document.

Legacy `af-live-notes` had no session identity. It is no longer cleared on Start
or Stop, nor automatically attached. The live Notes tab exposes it read-only for
manual copying to the correct meeting. Guessing its association would risk data
corruption. Pre-existing saved meeting Notes need no manual migration.

## Integration interfaces for #13 / #15 / #18

- `api_read_notes_document(target)` → optional `{source: "user_notes", markdown,
  json, updatedAt, revision}`; targets are `recording:<folder>` or `meeting:<id>`.
  This reads committed SQLite state and never sends text to a model.
- `api_write_notes_document(target, markdown, json, expectedRevision)` → committed
  revision. A mismatch returns a visible conflict. Native consumers must preserve
  the revision contract rather than overwrite an editor's snapshot.
- `api_notes_recording_target(meetingId)` → optional recording target for the
  meeting's last-keystroke journal, including library/recovery transitions.
- `notesForContext(target)` in `lib/notes-store.ts` → current
  `{source: "user_notes", markdown, json, revision}` after flush. It rejects if
  loading, conversion, persistence or conflict failed; it never silently feeds
  stale Markdown. Long-running AI jobs should retain this immutable revisioned
  snapshot, separate from transcript inputs, and label it as the user's thoughts,
  not proof of a participant's promise or meeting agreement.
- Existing `getMeetingNotes` / `api_get_meeting_notes` readers remain compatible
  for saved meetings. `api_save_meeting_notes` is retained for legacy callers and
  advances the revision, so a new editor detects competing old callers.

No new AI provider is invoked here. The old live Ask AI implementation no longer
reads an unassociated browser Notes slot. Subscription-Codex-only Notes context
integration is downstream work; this issue does not authorize the old arbitrary
summary provider to receive the newly persisted Notes.

## Verification and limits

### Execution results (Linux, 2026-11-18)

- Final standalone TypeScript validation using Node 22.16.0 passed.
- All 12 targeted Notes tests passed (10 store/recovery/conflict tests, one
  production-editor JSON-before-conversion test, one normal live/library UI test).
- The first full isolated run passed 34/34 files before the additional editor
  regression was added. The final default-timeout run passed 34/35; the unchanged
  download-progress Settings test exceeded Bun's 5-second limit under concurrent
  build load. Its isolated rerun with `--timeout 60000` passed both tests.
- The imported editor test initially failed because it assumed a real timer had
  fired after a fixed sleep. It now advances the readiness callback deterministically
  and passes; no product behavior was changed to satisfy that timing assumption.
- The production `next build` reached its 30-minute limit in optimized compilation,
  before type/lint/export completion. It is **incomplete**, not a successful build.
- Five Notes-focused native tests passed, then all 15 native recovery regressions
  passed (the command/lifecycle tests overlap between these selections). The
  additional real on-disk SQLite close/reopen test also passed, preserving RU/EN
  Notes and revision. It took 486.77 seconds on this loaded environment;
  this is persistence evidence, not a target-Mac performance result. The first
  cold portable compilation exceeded 30 minutes; the cached continuation passed.
- A full desktop-native Cargo attempt was stopped after confirming this shell
  lacks `pkg-config`/GTK/WebKit prerequisites. Portable native qualification uses
  the real production bodies described below instead, not a desktop-build claim.

Portable test entry points:

```sh
# Run mock-heavy files in separate processes (the runner does so).
bun scripts/test-isolated.mjs                   # from frontend/
python3 frontend/scripts/test-recovery-database-portable.py notes
python3 frontend/scripts/test-recovery-database-portable.py recovery
```

The native portable harness compiles production command bodies and Notes/
transcript repository code, with real SQLite and synthetic FFmpeg audio. Only
Tauri transport/state, device wrappers and unrelated contact linking are stubbed.
Dependencies are pinned to Cargo.lock. It does not prove desktop ABI, window
lifecycle, acoustic performance, model behavior or target Mac responsiveness.

Coverage includes serialized writes and rapid/out-of-order conversions, immediate
JSON backup, error/retry without further editing, empty Notes, existing meetings,
restart/Stop-folder journal lookup, incomplete conversion, conflict review,
normal live UI updates while ASR phrases arrive, navigation/library editing,
transaction rollback, separate simultaneous recording folders, duplicate recovery,
late post-Stop writes, and native read/write command callers.

Physical acceptance remains **NOT RUN**, deferred to the final combined Mac build:
enter RU/EN Notes while phrases arrive, navigate tabs and library, pause/resume,
close/reopen the window, Stop, quit/restart, recover an interrupted recording,
retry a synthetic storage failure and verify exact Notes on both target Macs.
Measure capture/transcript continuity and UI responsiveness rather than infer it
from mock tests. There is no DMG, local install, Actions run or publication here.

LocalStorage can be unavailable/full; the error is visible and native saving can
still succeed. If both browser journaling and SQLite fail and the process dies,
unsaved memory cannot be recovered. OS power-loss guarantees are those of SQLite
and the WebView storage, not an extra independent filesystem journal. Separate
windows editing simultaneously can conflict and require review. An invalid or
two competing recovery journals are retained and explicitly reported rather than
silently discarded. Plain Markdown is not used while exact JSON reconversion is
pending. Large documents have no invented size acceptance/performance claim.

## #14 R1 integration seam: projection refresh versus Notes ownership

#14 owns removing its destructive full-page reload after transcript version
selection/correction and refreshing only the transcript/summary projection. No
#14 control or reload implementation is changed on this branch. A projection
refresh must retain the same meeting Notes target and must not key the Notes
editor by transcript version, correction revision, or summary contents.

`loadNotes(target, true)` deliberately returns the existing dirty snapshot during
Markdown conversion, debounce and native-save-in-flight phases. ASR/version/
summary updates cannot supply initial Notes or replace this document. The editor
and store remain independent of the displayed transcript and summary. New tests
exercise all three dirty phases and a shared library editor whose surrounding
transcript/version/summary projections change immediately after typing, before
its debounce. Exact Markdown/JSON remain journaled, save correctly, and reopen
unchanged. These two added regressions passed: store file 11/11 and shared-editor
entrypoint file 2/2. They test #12's seam, not #14's actual selector/corrector.

**Combined regression still required after integration:** in the normal saved
meeting UI, type distinctive RU/EN Notes (including a list and exact formatting),
then select another transcript version within **<800 ms**, without waiting for a
Notes save. Repeat using transcript correction within <800 ms. Verify the editor
and unsaved/error state remain, exact Notes Markdown/JSON reach the correct
meeting, and transcript/summary refetches do not overwrite them. Close/restart
the app and reopen that meeting from the library; verify exact Notes again.
Repeat with an in-flight Notes conversion/write and a storage failure followed
by Retry. Use #14's real entrypoints in the combined regression, not a fabricated
projection wrapper. App-restart physical execution remains the final combined
Mac build gate; existing SQLite disk close/reopen evidence is not a substitute.
