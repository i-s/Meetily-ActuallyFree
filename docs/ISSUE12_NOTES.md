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

Each edit is also journaled synchronously as an immutable snapshot under
`af-notes-v2:draft:<encoded-target>:<randomUUID>` in localStorage, with exact JSON,
last complete Markdown, base revision, conversion-pending flag and explicit
supersession references. No two writers update the same journal key. Updating a
local snapshot creates a new key before retiring the previous immutable key.
A crash between these steps leaves linked snapshots, not a missing draft. The
native `api_notes_recording_target` also discovers the recording target's journals
after Stop, reload, app restart or recovery. A recovered incomplete conversion is
redone before text is used for AI/export.

Native acknowledgement only retires the writer's own immutable keys and the
explicit snapshots consumed by recovery. It cannot erase another writer's newer
same-target or recording-alias key, even when writes interleave with cleanup.
Unrelated surviving journals are never picked by timestamp: the UI shows their
exact contents and requires an explicit recovery choice, retaining the others.

Existing mutable `af-notes-v1:<target>` journals remain recoverable. They are
**never removed** by new code: localStorage has no atomic compare-and-delete, so
even comparing raw values before removal would race an older WebView writer.
After committing a legacy snapshot, an immutable exact-value receipt suppresses
replay of only that key/raw pair. A concurrent different raw value remains
recoverable. Legacy raw data and these receipts are intentionally retained;
new v2 keys and consumed immutable snapshots are cleaned up after saving.

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
- `notesForContext(target)` in `lib/notes-store.ts` → committed snapshot
  `{source: "user_notes", markdown, json, revision}` after flush. It rejects if
  loading, conversion, persistence or conflict failed. It captures target/entry,
  generation and revision before the native read, rechecks them and dirty/error
  state afterwards, then returns a detached clone of the committed native
  document, not mutable editor JSON. An edit during that await is an explicit
  pending/stale refusal, never uncommitted or mixed Markdown/JSON. Long-running AI jobs should retain this immutable revisioned
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


## Issue #12 R1 — snapshot and cross-WebView cleanup races

Both review findings were reproduced as actual failing tests before fixing them,
not merely accepted from static inspection. With only a store factory extraction
(no behavior fix), `tests/notes/r1-races.test.ts` failed all four initial cases:
normal context read with a concurrent edit (conversion complete/pending), and
independent A/B stores sharing Storage with a held A save (meeting/recording
keys). A's acknowledgement removed B's only recovery journal. The red run is a
local diagnostic, not a committed private artifact.

The fix uses the committed-only context boundary and immutable journal protocol
above. `createNotesStore({invoke, storage})` makes the actual store instantiable
for the harness; production still has one default instance per JS realm. The
shared-storage tests use independent stores, a real shared Storage-shaped map,
held native responses and fresh instances simulating loss of all JS state.
Additional checks interleave a new writer *inside* removeItem, replace a legacy
journal while its receipt is written, test both same-target and recording-alias
migration, retain independent recovery choices across restart, and verify a
returned context snapshot cannot change with later editor mutations. The normal
shared editor also renders those recovery choices and never silently selects an
empty or arbitrary document.

### R1 verification and pending export

- Red: the initial four delayed-read/shared-storage repros all failed before
  behavior fixes. Green: the extended race file passes all **9 tests**, including
  immediately pending conversion, own/alias recovery, storage-call interleavings,
  independent recovery choices and detached committed context snapshots.
- Final full isolated frontend suite: **36/36 files passed**, including 11 store,
  9 race, 3 shared-editor entrypoint and 1 production-editor Notes tests.
- Final standalone TypeScript validation and `git diff --check`: **passed**.
- Serialized portable native verification: **6 Notes tests and 15 recovery tests
  passed**, including real on-disk close/reopen. Selections overlap; no new native
  product change is made for R1. Fixtures are synthetic, not private audio.
- Production export: **pending**, not a product-code failure. The R1 Next attempt
  was stopped during compilation when host memory pressure became severe and a
  separate #14 production build was found running. No successful production
  export is claimed. The coordinator confirmed #14 owns the current build slot
  and #12 follows; the next attempt is a single retry at the committed R1 SHA,
  after explicit resumption, with `NODE_OPTIONS=--max-old-space-size=2048` and
  `CIRCLE_NODE_TOTAL=2`. No competing heavy build or worker polling is permitted.
- Mac WebView behavior, physical recording/ASR continuity and the combined #14
  <800 ms switch/correction plus app-restart regression remain unrun gates. No
  packaging, DMG, Actions, push, install or Mac/private-data operation occurred.

### Storage and lifetime assumptions

- Each WebView has its own JS store. The app currently uses a main window and a
  separate minibar WebView; code does not assume only one WebView can ever edit.
  If WebViews share same-origin persistent localStorage, they may interleave at
  every storage call. Individual setItem/getItem/removeItem operations are
  treated as synchronous/atomic; **their sequences are not atomic**. No storage
  mutex, storage-event ordering, Web Locks support or multi-call CAS is assumed.
- All new writers create unique immutable keys with `crypto.randomUUID()`.
  Another writer/recovery instance must copy a selected record to a fresh key,
  never overwrite it. Retiring an immutable source therefore cannot delete a
  later edit. This is a protocol invariant, not a hoped-for scheduling property.
- Different origins/profiles may not share browser journals; native committed
  Notes are still common. A journal can only be recovered from the profile that
  persisted it. Cross-window behavior on target macOS WebViews remains a physical
  gate; the synthetic shared Storage tests prove the interleaving protocol, not
  macOS storage durability or OS power-loss semantics.
- Legacy mutable data has no writer identity. Exact-value receipts preserve raw
  records without unsafe deletion, but cannot infer the intention of an old
  writer rewriting identical bytes. Receipts cover only already committed exact
  snapshots. Legacy data/receipts may consume storage until a separately approved
  cleanup/migration policy; no automatic destructive cleanup is introduced here.
- Independent unsaved writers may still conflict at SQLite's revision check.
  No automatic merge or last-writer-wins is claimed. Recovery retains the other
  drafts and offers explicit choices; stale conversion/text never enters context.

## Issue #12 R2 — readable recovery journal with no allocation capacity

The independent review's P2 was reproduced on `57db11b`: a valid single retained
v2 snapshot was readable, but every `setItem` threw `QuotaExceededError` while the
native store was healthy. The actual `MeetingNotesEditor` + production
`NotesEditor` mounted test failed because the editor stayed hidden. Separately,
the production-store own/alias cases were run against the unmodified store from
that reviewed SHA in temporary test modules: both failed at `loaded === true`.
Those temporary modules were removed; only the regression tests are committed.
These are executed red repros, not static-only findings.

### Recovery without allocating another browser record

When copying the selected snapshot fails, the store now publishes a **loaded,
read-only** recovery state, retains the original immutable source(s), and exposes
the storage error/warning rather than hiding the editor. The ordinary Notes UI
mounts the actual rich editor read-only and shows selectable/copyable exact rich
JSON and Markdown. Pending conversion labels the Markdown as last-complete/stale;
the production editor reconverts its actual rich JSON before native save is
allowed. It does not substitute stale Markdown or flatten formatting into text.
If editor normalization changes the JSON values, the lock rejects that change
rather than silently claiming those different blocks were recovered.

An explicit **Save retained Notes to meeting** bypasses new browser allocation,
not native validation: the same revision-checked SQLite command must commit,
and a subsequent native read must match its revision, exact Markdown and rich
JSON values/array order before immutable sources are retired. JSON object-key
order is immaterial (native serde_json can reorder properties). Read/write errors,
conversion failure or a conflicting native revision keep the original source.
Conflict review still requires explicit replacement of a reviewed revision;
other writers' immutable journals are never consumed by that replacement.

**Cancel recovery (keep original)** cancels the local conversion generation and
pauses automatic/unmount flushing without deleting a source. **Resume recovery**
remounts read-only for actual JSON reconversion. Native writes already in progress
cannot be cancelled by that control: it is disabled and the store also rejects
such cancellation honestly. Retrying after native failure works with no new
browser capacity. A fresh store can reopen the original after failure or the
verified native document after success.

The quota warning/lock persists after successful native recovery. Further
unjournaled edits are not accepted or presented as saved. **Retry recovery
backup** must actually create a durable browser snapshot before unlocking normal
editing; a failed retry retains the lock and source. The normal autosave path
also enters this honest locked mode if a later edit cannot be journaled. The
UI retains selectable JSON/text if clipboard access is unavailable or denied.

A v2 source can reference mutable legacy ancestry. With zero capacity, safe
legacy acknowledgement receipts cannot be allocated: after verified native
commit, the store retains the immutable supersession record and its legacy
sources, reports native success plus deferred cleanup, and does not expose the
older superseded draft or delete mutable data unsafely. Reopening still recovers
the correct rich snapshot; it may require explicit reconversion/save confirmation
again until browser storage is restored. Ordinary v2-only sources require no
receipt allocation and are retired after successful readback.

### R2 verification and limits

- `quota-mounted.test.tsx`: **5 passed**, through the production meeting Notes
  component and production `NotesEditor`/Markdown conversion helper. BlockNote's
  view/editor engine and Tauri transport are mocked; the exact rich blocks passed
  to conversion and read-only state are asserted. Cases cover finished/pending
  conversion, copying, successful/failed reopen, cancel/resume, multiple choices,
  native CAS conflict and reviewed retry under permanently throwing `setItem`.
- `quota-store.test.ts`: **9 passed** using independent production-store instances
  with Storage-shaped backends, held native write/read responses, and fresh
  instances for restart. Covers own/alias sources, verification before retirement,
  write/read errors, paused generations, locked edits, restored backup capacity,
  competing drafts/CAS and JSON property reordering/legacy ancestry.
- Full isolated frontend suite: **38/38 test files passed**, including all
  **38 Notes tests** across six isolated files. Final standalone TypeScript
  `tsc --noEmit` and `git diff --check` passed. No native schema, native command,
  recording, provider or #14 code changes are made by this fix. Native tests were
  not rerun for this frontend-only change.
- R1's previously pending production export subsequently **passed at exact
  `57db11bce0597857d8a53765d3fc39cceb89e49b`**, exit 0, including lint/types and
  12/12 static pages, using the exclusive bounded-memory slot. That result is not
  automatically a production export verification of the later R2 commit.
- R2 does not start an uncoordinated heavy build. Physical Mac WebView/storage
  durability, recording/ASR continuity, and the combined #14 <800 ms change plus
  restart test remain deferred. The tests establish software recovery behavior,
  not an actual Mac restart, real BlockNote/WebView rendering, or power-loss proof.
