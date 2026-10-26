# Per-recording Meeting notes, Summary and Ask AI

Implements [issue #3](https://github.com/i-s/Meetily-ActuallyFree/issues/3)
on top of the open [PR #2](https://github.com/i-s/Meetily-ActuallyFree/pull/2)
head `55b4dd776010e42331b173d8539b61cbb22624ec`. PR #2 was not merged;
its Codex CLI, call-detection and packaging changes remain the base. No release
or version/identity change is part of this work.

## Ownership and interfaces

- `audio/recording_commands.rs` allocates the SQLite meeting before capture and
  exposes `meeting_id` in state/events. Pause retains it, Stop/recovery reuse it,
  and another Start allocates another ID, including silent and short recordings.
- `audio/recording_persistence.rs` owns a bounded 256-message transcript writer.
  Capture uses nonblocking sends. Errors are visible and prevent incomplete AI
  snapshots; Stop retries the complete buffered transcript. Speaker display edits
  are serialized with writes; capture-source provenance remains separate.
- `flush_recording_transcripts` is the barrier used by chat and summary inputs.
  It rejects during Stop, including the interval after detaching the writer.
- `meeting-notes-store.ts` owns per-ID edits beyond component lifetime. Exact
  BlockNote JSON is synchronously saved to a per-ID local recovery draft before
  Markdown conversion; serialized SQLite saves follow after 400 ms.
  `flushMeetingNotes` waits for conversion and writes before Stop finalization,
  Generate/Regenerate and Send. Failed reads/saves stay visible; they never become
  empty successful content. Deletion tombstones reject late editor callbacks.
- `meeting_chat.rs` persists each question and its context before inference;
  native tasks persist completion independently of the view. Startup marks
  unfinished requests interrupted. Deletion cascades and late completions cannot
  recreate a meeting. Commands are exposed through existing `live_assistant`.
- `summary/input_snapshot.rs` captures all transcript rows and full notes in a
  transaction. The worker receives this immutable snapshot. Only successful
  completion promotes it to the result's provenance; failure/cancel preserves
  the previous summary and its snapshot. Chat is never an input to summary.

See [RECORDING_SHUTDOWN.md](RECORDING_SHUTDOWN.md) for lifecycle failure handling
and [ASK_AI_LANGUAGE.md](ASK_AI_LANGUAGE.md) for chat/provider contracts.

## UI and context semantics

The right panel has Meeting notes / Summary / Ask AI; the live panel also retains
Speakers. Tasks live inside Summary. Editing notes only marks an existing summary
as stale; generation is explicit. Notes and transcript are separate prompt
sources, with instructions to label note-derived claims and contradictions.

Ask AI uses the last 10,000 Unicode scalar characters of transcript by default;
Settings accepts another positive N or the whole transcript. Speaker/timestamp
wrappers do not consume N. Full notes and the existing summary/tasks are separate
inputs. All chat turns remain stored; the prompt includes the latest 20 completed
turns, with that history limit reported alongside each answer's actual range.

The transcript remains complete. The first included row is split for display
when the context begins inside it, keeping the original row ID for edits and
honest turn-level timing. No word timestamp is invented. If that row is outside
the loaded page, the header explains this rather than drawing a false boundary.
Each saved answer retains its own range even after settings/transcript change.

No legacy content migration or new search/tool integration is added. Additive SQL
migrations create chat storage and snapshot columns; existing providers are reused.

## Acceptance evidence (2026-10-26)

| Scenario | Automated evidence |
| --- | --- |
| Same ID through Pause/Stop/recovery; different next Start | Recording identity hooks and native repository/writer tests |
| Silent recording and Stop under 10 seconds retain notes | Three-second silent Stop/recovery hook fixture; notes flush tests |
| Immediate typing, navigation, pending conversion/save | Notes editor/store tests; per-ID draft, failed-save retry and delete guards |
| Restart/crash boundaries and persistent chat | Real SQLite close/reopen, interrupted-request recovery, detached Codex subprocess fixture, recovery hooks |
| Full immutable summary input, no chat, manual regeneration | 130-row transcript and long Unicode notes SQL fixture; concurrent edits; failed/cancelled regeneration; generation hooks and panel tests |
| Latest N/default 10,000/all, full notes and saved ranges | Native Unicode/settings/range/history tests; settings/hook/boundary UI tests |
| Speakers, separate tabs, tasks and generated live summary | Panel, live-summary and existing chat tests; serialized native speaker edits |
| Delete recording and prevent late resurrection | Native cascade with foreign keys disabled, frontend recovery deletion and draft tombstone tests |

Validation runs use separate Bun processes per file because module mocks are
global. Native Linux validation uses genuine GTK/WebKit/ALSA/ONNX dependencies and
built sidecars. The final full library suite passed 402 tests (0 failed, 9 ignored). Final native
`cargo check -p meetily --lib --tests` passed. Frontend: 203 tests in 47 isolated files passed, plus five NSIS payload tests.
The production Next static build and standalone `tsc --noEmit` passed.
CI includes the new frontend and native regressions on macOS and Windows.

Real Chromium/BlockNote Preview checks cover typing then immediately switching
notes → Speakers → Ask AI, flushing on Send, silent Stop and retained notes, plus
live tab geometry at a 340px panel width. This caught a readiness transition that
remounted BlockNote and removed save subscriptions; the subscription now
reattaches after editable/readiness changes. Run against an already running
`next dev` (never concurrently with `next build`):

```sh
PLAYWRIGHT_MODULE=/path/to/playwright-core CHROMIUM_PATH=/path/to/chromium \
  node frontend/scripts/test-meeting-notes-browser.cjs --base-url http://localhost:3118
```

Saved-view Preview checks also exercised separate Summary/tasks, a partial-turn
context boundary with N=40, and live chat → Pause/Resume → silent Stop → the same
saved chat → notes-only summary generation. These use fixtures, not native
persistence.

## Qualification limits

- Browser Preview uses explicit fixtures, not a database or model; it qualifies
  layout and user flow only. SQL close/reopen and hook recovery do not substitute
  for killing a running packaged application during physical audio capture.
- Actual microphone/system capture, macOS WebView recovery after process kill,
  and a logged-in Codex/Claude session require installed-app qualification.
  Hardware/model/auth-dependent ignored tests were not run.
- A recovered JSON draft whose Markdown conversion was interrupted is converted
  when Meeting notes mounts. An exceptionally early Stop/Generate/Send can report
  a retryable save error until editor initialization finishes; stale Markdown is
  never submitted instead.
- Notes are never silently truncated. A provider whose context window cannot fit
  the complete notes/input reports an error; the saved material remains intact.
