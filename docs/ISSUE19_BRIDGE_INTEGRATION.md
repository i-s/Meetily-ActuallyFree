# Issue #19 — accepted bridge integration, 2026-11-19

This history-preserving integration joins accepted standalone R1
`9cd59f429a50506cd38da45a7c55792a10c734eb` and the exact accepted #5–#18 bridge
`2d388cf15adb95f2d37ab1e9ff17b82df723e328`. It does not replace either history,
reimplement the canonical projection, publish a release, or qualify a Mac build.

## Shared boundaries

- Manual `diarize_meeting` retains R1's native task and offline admission through
  blocking inference and publication. That task calls the bridge's shared inner
  operation, which validates identity against the original recording folder
  before checkpoint/mutation. The old weaker outer precheck is not restored.
  Candidate cancellation, immutable original-folder policy, candidate source
  routing and post-call capture priority remain bridge-owned.
- Transcript versions retain both audio-availability re-observation and the
  bridge's Remember term/vocabulary suggestions. Library materials subscribe to
  both canonical publication and scoped audio changes, with stale loads fenced.
- Stop's lifecycle-owned finalization still avoids waiting for offline admission;
  independent checkpoint cleanup retains its offline reader exclusion and the
  same exact-file proof. No deletion scope is broadened.
- Canonical summaries/tasks still use the bridge's immutable artifact projection,
  selected transcript/Notes tuple and original person/transcript/time anchors.
  Cleanup does not write these rows or queue artifacts.

## Integration defect found and fixed: unchanged Notes readiness

The mounted normal navigation test initially lost the current canonical summary:
leaving/reopening the detail page advanced Notes revisions even with identical
rich JSON and Markdown. The editor's readiness conversion called `editNotes`,
which unconditionally dirtied clean, unblocked Notes. Unmount then saved that
fabricated edit and invalidated the bridge's summary tuple. This was not audio
cleanup deleting a summary and was not fixed by weakening tuple checks.

`notes-store.ts` now dirties on changed JSON or on changed converted Markdown.
Already-dirty/recovered/in-flight drafts remain dirty; genuine JSON changes are
still journaled before conversion, and failed conversion/CAS/quota handling is
unchanged. Clean identical readiness creates neither a journal nor a native save.
Markdown-only legacy documents may still require a real rich representation save;
the combined fixture commits that representation before publishing its canonical
summary, just as an editor preparation barrier does.

RED: the new store regression observed 19 writes where 18 were expected.
GREEN: unchanged readiness keeps revision 7/no draft, a changed conversion advances
it to 8, and formatting-only JSON advances it to 9. The mounted navigation test
then retains the canonical summary through reviewed cleanup and both exports.

## Fresh scoped verification

All files, SQLite databases, audio markers and encoded audio are generated,
disposable fixtures. None are private recordings or installed user data.

| Check | Result | What it establishes / adapter boundary |
| --- | --- | --- |
| `test-audio-cleanup-portable.py` | 43 passed, 0 failed/ignored | Actual cleanup/command/locks, merged migrations, real queue store/runner/projection/priority, versions, library search/export, playback identity; adapted Tauri/capture flags/inference |
| `scripts/test-isolated.mjs` | 73/73 frontend files | Real mounted storage consent, unchanged-focus/replacement player, canonical page, Notes CAS/recovery, search and both export pipelines; mocked native transport/editor engine/picker |
| `test-recording-shutdown-portable.py` | 48 passed, 0 failed/ignored | Actual savers, deterministic Stop/offline lock interleave, real FFmpeg generated audio, committed checkpoint cleanup; no device capture |
| `test-recovery-database-portable.py` | 17 passed, 68 filtered, 0 failed/ignored | Targeted recovery commands, SQLite transactions, generated FFmpeg fixtures; not a full recovery-crate run |
| `test-post-call-native.py` | 4 passed, 50 filtered, 0 failed/ignored | Actual queued refinement dispatch, cancellation before admission, original-folder identity, fake CLI retry/revision fencing |
| `test-post-call-portable.py` | 28 passed, 0 failed/ignored | Queue epochs, capture priority, artifact/projection CAS, immutable vocabulary/revision behavior |
| `test-guarded-recording-stop-portable.py` | 15 passed, 0 failed/ignored | Exact generation/owner Stop authority, adapted post-accept device work |
| `test-recording-start-authority-portable.py` | 39 passed, 0 failed/ignored | Start tickets/owner fencing, priority drain/revocation, adapted capture/model setup |

The combined native cleanup regression reviews before queue enqueue/claim,
observes blocked deletion, cancels the claim but retains a draining offline
reader, and refuses normal confirmation until the reader releases admission.
It then publishes a summary with the real text-only runner, confirms cleanup
through normal command bodies, compares **every database table's rows** before
and after, and reads corrected transcript, Notes, canonical summary and tasks via
ordinary library search/export. A curated task's person ID, done state, transcript
ID and audio-time anchor remain intact. After removal, a new Notes revision and
explicit summary refresh invoke only text generation; the ASR closure would panic
if called. Separate normal-command tests cover active capture, stopping, lifecycle
contention and reader drain; review/confirm tests cover queue/version/file changes,
partial unlink failure, one-shot consent and fresh retry.

The mounted combined fixture exercises unchecked selection, review, explicit
confirmation, detail-page Markdown export, library search and bulk Markdown
export after removal. Playback/re-ASR unavailability uses the saved-audio
notification, not destruction of transcript versions. The dedicated mounted
player fixture still covers unchanged focus at 23s/1.5x, unrelated cleanup,
replacement/deletion and stale navigation responses.

## Limits and handoff

The warm runners compile only portable adapters/production modules against cached
dependencies. No full desktop/Next build, standalone project TypeScript check,
model inference/acoustic fixture, actual Tauri asset protocol/WebView cancellation,
codec playback, macOS M4/M1 capture/storage, physical disk accounting, deletion
latency, installation, packaging, Actions, push or issue closure ran. Aborting a
Rust diarization waiter establishes native reader lifetime ownership, not that
closing a real Tauri WebView cancels commands. External filesystem actors remain
outside the application's lock/SQLite reservation. Independent review remains
coordinator-owned under the explicit no-delegates instruction.
