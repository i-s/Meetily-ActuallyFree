# Issue #15: durable automatic post-call processing

Live issue: **[macOS 11/16] Автоматическая обработка после встречи**
(https://github.com/i-s/Meetily-ActuallyFree/issues/15). Live body and comments
read on 2026-11-18; there were no comments.

## Integration provenance

- Runtime base: `226ff989997e49a404ab8b684e6168f453f31c4f` (#12 Notes + #14 versions).
- Reviewed #13 product source: `7f017db`; integrated tip
  `e40f6298424ad706e7e91ad434e7d69be6aba8e2` includes its docs-only qualification update.
- Separate local integration merge:
  `a203a83809d4d319e0784597fb620065761aa779`.
- The merge preserves both histories. `lib.rs` command registration retains the
  original Notes/selected-version APIs; recorder text-context helpers are additive.
  #15 implementation follows that integration commit, not inside it.
- No PR, push, release, packaging, Actions, installation, real user data or service
  request belongs to this qualification. Independent Opus review is coordinator-owned.

## Native ownership and data flow

```text
normal Stop -> existing transcript drain/save
recovery -> existing idempotent native save
  -> TranscriptsRepository save transaction:
       meeting + live snapshot + Notes association + post_call_jobs enqueue
  -> one process-owned native scheduler (not React)
       local stage -> awaited #14 ASR + selected-engine speaker refinement
       summary stage -> committed selected transcript + committed Notes snapshot
                     -> #13 isolated subscription Codex CLI
                     -> validated JSON + revision-fenced transaction
  -> post_call_results (immutable, separate from editable summary and Notes)
```

`post_call/store.rs` owns claims, fencing, stage transitions and publication.
Migration `20261118001500_post_call_queue.sql` provides one automatic job per saved
meeting and at most one `running` job. Both branches of normal/recovery save
(including already-committed folder/recovery identities) enqueue in the same
transaction. A failed queue write rolls back that save; a lost IPC response can
be retried without a duplicate meeting/job. Recordings without an owned saved
folder are not enqueued; this does not bulk-process the old library on upgrade.

`post_call/worker.rs` starts after native database activation, once per process.
A WebView reload never recovers/restarts a live claim. Native startup recovers
only `running` jobs to `queued` and increments their epoch; deferred, cancelled
and waiting jobs retain the user's choice. The existing #14 startup recovery
marks incomplete candidates interrupted. No battery/AC check gates scheduling.

`post_call/runner.rs` is the same orchestrator in the app and portable tests. It
awaits every owned local/model/CLI operation; invalidating a claim requests scoped
cancellation but does not abandon a blocking model thread. Successful local work
advances the durable stage before CLI work. CLI/network/quota/validation failures
become `waiting`, with an explicit Retry action, not a retry storm or provider
fallback. Cancel/Defer invalidate the epoch before requesting process-local
cancellation; an old result cannot undo that choice.

### Recording priority and retained progress

Both native recording-start entrypoints reserve priority **before** acquiring
the engine lifecycle lock. This fences new automatic work, cancels the current
owned background token, and waits for its resource admission guard to drain before
live initialization. A cancelled automatic pass waiting for the offline operation
lock returns without waiting for an unrelated manual operation. Startup failures
release their priority reservation. Active/stopping recording checks also run
before claim and after admission.

The shared #14 saved-meeting worker is awaited by `run_queued_refinement`, not
started via a fake live session or a JS event listener. One deterministic candidate
ID (`post-call-refined:<meeting>`) closes the crash window between candidate
creation and the queue checkpoint. Retry reuses this identity, original settings,
recording metadata and source map. A committed ASR checkpoint is reused when
speaker refinement was interrupted; a ready candidate is immutable and skipped.
Incomplete ASR is replayed from the stage boundary, not duplicated into selected
rows. A corrupt checkpoint fails visibly and receives terminal failed status.

Offline candidate diarization installs a thread-local cancellation scope. Selected
Pyannote segmentation windows/embedding turns and Nemotron feature windows check
it between inference units. Live inference and unrelated threads have no scope.
No model is killed in the middle of an ONNX call and no alternate engine is chosen.
Decoding/resampling/feature preparation, a current ASR/ONNX inference unit and
clustering may still need to finish before yield. **Maximum wall-clock yield delay
and recording readiness are unqualified until the physical Mac run.** Completed
stages/versions survive; this is not a per-sample/per-ASR-chunk resume system.

## Selection, correction and Notes invariants

- Automatic local refinement never selects a candidate and never overwrites
  canonical live text, user corrections, contact mappings or Notes.
- The automatic summary deliberately uses the **currently selected** transcript,
  not the new unreviewed ASR candidate. Its UI states this. Reviewing/selecting a
  candidate remains the original #14 revision-checked operation; use
  “Summarize current selection” afterwards for an updated summary.
- `worker::snapshot` reads selected version ID, current transcript rows/revision
  and committed Notes/revision in one SQLite transaction. Browser drafts not yet
  committed are not silently treated as durable Notes. A later committed recovery
  draft increments the independent Notes revision and makes an old result stale.
- Publication checks meeting + selected version + transcript revision + Notes
  revision + claimed job epoch **inside its write transaction**. No stale result
  overwrites a newer selection, correction, Notes document or unrelated meeting.
- Result and `complete` job state commit atomically. The artifact primary key is
  the meeting/version/transcript-revision/Notes-revision tuple. An explicit refresh
  of an already-published tuple reuses its artifact without another CLI request.
- Previously published artifacts remain readable through `post_call_results`;
  `post_call_list` marks the latest artifact current/outdated. These artifacts do
  not overwrite the existing summary editor or `transcript_versions.summary_json`.
- A process crash after sending to Codex but before local publication can require
  repeating the external request. Exactly-once external service execution is not
  claimed; local jobs/candidates/publications remain idempotent and bounded.

## Saved-meeting subscription Codex operation

The native worker calls the actual #13 `codex_cli::cli::generate`, using the same
configured executable and `isolated_auth_home` (`codex-live-auth`). This is a new
saved-meeting text operation, not a fabricated active recording or live-Q&A DTO.
It retains #13's qualified CLI-version checks, dedicated ChatGPT subscription login,
environment/config/rules isolation, no-tool policy, ephemeral working directory,
bounded output/timeout and process-group cancellation. No generic summary resolver,
paid API, alternative CLI or audio path is used. Auth credentials remain CLI-owned;
no credentials were read or used during this work.

The bounded JSON prompt contains selected transcript rows and a separately labeled
`user_notes` source. It asks for summary, decisions, tasks, open questions and
uncertainty in the meeting's RU/EN language. `summary.rs` validates all four sections,
exact meeting-row quotes and explicit verbatim owner/deadline evidence. Notes may
appear only as explicitly prefixed personal context in summary/open questions;
they cannot establish participant decisions/tasks/owners/deadlines. Speaker numbers
are not identities. Validation proves that evidence exists, **not** that an LLM's
semantic interpretation or real-world attribution is correct; physical/content
qualification and user review remain required.

The current serialized context limit is 480,000 UTF-8 bytes; output is bounded to
512,000 bytes with bounded claim/evidence counts. Oversized context fails visibly,
without truncation, preserving local results and previous summaries. A synthetic 1,200-turn, 90-minute RU/EN text fixture fits intact
(first and last turns verified), using compact rows without redundant timestamp/
duration fields. Chunked summarization is not implemented; texts above this bound
require future long-context work. No model request is sent on context-bound failure.

## Visible interface and composable commands

`PostCallQueue` mounts above normal app navigation. It observes native state,
shows local/CLI stages and meeting-scoped local progress, links to meeting/version
review, exposes Defer/Resume/Cancel/Retry, displays stale-result provenance and
retained summary history with speech quotes. Model output is rendered as escaped
text, never executable HTML/remote Markdown. Unmount does not cancel work.

The normal meeting-details post-recording route uses `PostCallQueueNotice`, not
its former WebView-owned `PostCallProcessingDialog` trigger. The legacy manual
component remains available unchanged; it is not mounted by the automatic route.
No competing automatic local refinement or legacy API-summary request is started.

Additive IPC:
- `post_call_list()` -> durable job state, candidate, latest result tuple/freshness.
- `post_call_control(meetingId, action)` -> defer/resume/cancel/retry/refresh.
- `post_call_results(meetingId)` -> all immutable result artifacts for that meeting.

### #10 policy integration

Opaque recording metadata and per-row source provenance survive candidate retry
and checkpoint reuse. Explicit in-person recordings retain neutral room microphone
labels; no model-channel identity or voice-profile assertion is introduced. #10's
accepted session-policy resolver must apply in the **shared native diarization
inner path**, before the candidate-only restrictive adapter. This branch does not
import unfinished #10/#11 work or duplicate that policy implementation. The added
cancellation scope wraps work, not identity decisions; preserve it when merging
that shared `spawn_blocking` entrypoint and both recorder-start gates.

### #16 approved-vocabulary seam (not integrated here)

The updated contract names:
`VocabularyRepository::approved_global(pool)` -> `ApprovedGlobalVocabulary {
terms: String, revision: i64 }`, read with one SELECT at summary-job execution;
`codex_cli::vocabulary::ApprovedVocabulary::global(&terms, revision)?` then
`.augment_prompt(&prompt)?` before this worker's normal isolated `cli::generate`.
Save the helper's actual `.revision()` and `vocabulary:global:N` snapshot identity
in result/job provenance; include that snapshot in cache identity so an approved
vocabulary change does not reuse a differently-guided result. The source is
`approved_vocabulary`, scope global: spelling guidance only, not speech, Notes,
decisions, or evidence. Never use legacy `get_global`/`get_effective` for summaries.
Existing `get_effective` here remains only the preexisting local ASR hint interface.

No unfinished files/branch/helper were copied. The integration point is immediately
before `generate` in `worker.rs`; accepted #16 integration must add its actual
snapshot to artifact provenance/cache key and run combined tests: legacy-only
terms excluded, the in-flight snapshot immutable across edits, 1,000 Unicode
characters/512 KiB final-prompt overflow explicit with no truncation/publication.
Those combined tests have **not run** without the accepted helper. #16 owns it.

### #18 summary-owner generation seam (not integrated here)

This queue publishes immutable `post_call_results`, not `summary_processes`, and
never runs task extraction or same-row summary UPSERT. Existing editable summary
and extraction ownership APIs remain untouched. Coordinator contract for #18 R1 (parent `fc096430`, not merged here):
`20261118001800_action_sync_ownership.sql` adds trigger-owned
`summary_processes.action_sync_token`. Writers must never copy/set/persist that
token; ordinary status/result INSERT/UPDATE/UPSERT writes participate without an
extra JS promise or job hook. A later bridge into #18 must obey that ownership: a newly published
or replaced canonical summary invalidates the previous extraction even if an
UPSERT keeps the same row ID and identical result text. Carry the automatic
artifact tuple together with that canonical owner generation; keep meeting,
selected-version, transcript-revision and Notes-revision publication guards.
Do not use content equality or row ID alone as an extraction freshness proof.
#18 owns the migration/triggers/extraction tests; no unfinished code or duplicate
trigger is imported here. If extraction is added, use `syncMeetingActionItems` and
its checked-context/atomic commands, never meeting-only `syncAiActionItems` plus
`api_save_meeting_summary(userEdit:false)`. Combined identical-text replacement and
stale-extraction qualification awaits the final accepted #18 SHA.

## Portable verification and remaining gates

These tests use synthetic local text, SQLite and fake CLI executables only:

- `python3 frontend/scripts/test-post-call-portable.py`: production queue storage,
  runner, evidence validation, recording-priority admission and scoped cancellation;
  **16 passed**; includes disk reopen, epoch fencing, defer/resume, two calls, interruption, CLI
  failure/retry, immutable history and all revision-tuple stale guards.
- `python3 frontend/scripts/test-post-call-native.py`: production scheduler,
  actual awaited refinement/start bodies, native command bodies, real migrations,
  SQLite and the #13 subprocess adapter. **3 passed**. Only desktop transport, model/source IO,
  model locks and preference transport are synthetic. Fake subscriber CLI verifies
  two saved calls, no work during recording, local candidates retained after network
  failure, explicit retry without rerunning local ASR, selected-only summary input,
  idempotent refresh, cancellable admission and terminal checkpoint-read failures.
- `python3 frontend/scripts/test-recovery-database-portable.py --offline`: **85
  passed** across real repository/save/recovery/Notes/version/SQLite/FFmpeg tests.
  Includes actual normal/recovery queue enqueue/rollback and deterministic candidate
  checkpoint preservation. Generated audio is synthetic; this is not an ASR test.
- Isolated frontend runner: final **47/47** files passed. The initial run was **46/47**; the
  page-editor mock lacked the newly imported Tauri event transport. After fixing
  that fixture, all **10** page-editor tests and the entire isolated runner pass.
- Queue UI **4 passed**, including scoped progress, persisted result/history,
  retry/defer, observation failures, stale cached-history tuple handling and no
  competing job/Notes/selection writes.
- Standalone TypeScript check completed with exit **0**. No Next build ran.

Use the existing installed Rust/Bun/FFmpeg prerequisites; the scripts pin dependency
versions from the checkout lockfile and run native portable builds offline with
one cargo job. Do not treat local helper paths as a public portable toolchain.

**Not run / not claimed:** full desktop/native/model build (no heavy-build allocation),
Next production export, DMG/packaging/installation/Actions/release, real signed-in
Codex or network requests, real microphone/system/Zoom/Discord/in-person audio,
RU/EN same-audio ASR quality, power/thermal/resource/yield measurements, real Mac
restart/two-call/CLI-failure/cancellation acceptance on M4 Pro/48 GB or M1 Pro/32 GB.
The real Mac batch must cover normal Stop and recovery with Notes, two consecutive
5–90 minute calls, selected/corrected text preserved, battery/defer/resume, recording
preemption during ASR and each speaker engine, killed-app restart, offline/quota,
unsupported/signed-out isolated CLI and a real authorized successful summary.
Portable tests do not close those gates or the live issue.
