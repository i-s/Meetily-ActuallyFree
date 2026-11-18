# Issue #14: safe local post-meeting transcript versions

Live issue read on 2026-11-18: **[macOS 10/16] Повторная расшифровка без потери исправлений**,
https://github.com/i-s/Meetily-ActuallyFree/issues/14 (no comments at implementation start).
Software base: `4269de67c6747b2f5fce72596826c05dadbb3e1c`.

Reuse review: the prior fork's `e94132e` name-retention patch copies names by
best time overlap but still deletes live text, misses corrections to generated
speaker labels and cannot preserve manual text. Its destructive ASR commit and
name-overlay heuristic are superseded, not cherry-picked again. The existing
source-track/VAD/segment timing paths, `cdf7ec0` STT lifecycle hardening and the
selected-engine integrations documented by `6228c23` are reused. Speaker-only
reruns retain their label-only contract and now checkpoint current corrections.

## Ownership and normal entrypoints

- Meeting details → **Enhance transcript** → existing local model/language/vocabulary
  dialog starts `start_retranscription_command`. It now returns `version_id` as
  well as `meeting_id`. It does **not** replace selected text on completion.
- The post-recording handoff starts that same native command, observes durable
  job status, offers cancellation and **Keep live transcript / defer**. Leaving
  the view does not cancel native work. No browser promise owns the result.
- Meeting details → **Transcript versions** shows live, refined and corrections
  snapshots, timestamps, settings, terminal errors and running jobs. Review a
  snapshot against the displayed transcript and the existing audio player; choose
  **Save current corrections & select this version** explicitly. **Correct selected
  text** uses the native revision-checked correction command. Existing speaker
  rename/reassignment controls continue to edit the selected projection, with a
  corrections snapshot before each operation. Speaker-only reruns also checkpoint
  first; a manually chosen generated label such as `Speaker 2` is not discarded
  simply because its spelling looks machine-generated. Standalone shared-room
  identification is explicitly unavailable for in-person recordings.
- With unavailable saved audio, the version browser remains readable; the
  enhancement dialog disables Start and the native start command also rejects it.
  A missing half of a dual-track call is an error, not permission to use a mix.
- No ASR/diarization provider API is added. Only local Whisper/Parakeet are accepted.
  This handoff no longer implicitly starts the legacy configured summary provider.
  The approved subscriber Codex CLI product-text handoff belongs to #15; no paid
  API or substitute text provider is introduced here. Existing manual legacy
  summary/provider settings outside this flow are not reimplemented by #14.

## Durable model and transactions

Migration `20261118001400_transcript_versions.sql` adds:

- `transcript_versions`: immutable ready snapshots (`live`, `refined`,
  `corrections`), parent version, UTC creation time, ASR/model/language/selected
  diarizer settings, JSON segments, optional archived summary, job status/error.
- `transcript_selection`: selected baseline ID and monotonically increasing
  meeting revision. Existing `transcripts` are the selected **editable projection**.
- SQLite INSERT/UPDATE/DELETE triggers increment revision for **every** canonical
  writer, including existing speaker controls and legacy native writers.

Initial recording/recovery save creates the live baseline inside the same DB
transaction as the meeting and transcripts. Idempotent #9 recovery still returns
an existing meeting without modifying its rows, corrections or versions. Legacy
meetings are snapshotted lazily before first version operation. If a much older
application already destroyed the original text, migration cannot reconstruct it.

The native reservation rejects concurrent ASR jobs before returning. It creates
one running candidate, captures the local model and selected diarization engine,
then executes under the existing native offline-operation/STT lifecycle guards.
ASR decodes the saved tracks and saves candidate rows only to version JSON;
selected rows, original folder `transcripts.json`, `metadata.json` and checkpoint
proofs are untouched. Candidate diarization reads that JSON, updates its labels
only, and skips selected-row/contact/folder writes. Only a complete ASR + selected
engine pass becomes ready. Selected-engine errors are explicit, never a switch
of diarization engines. In-person recordings skip shared-room identification.

Cancellation is cooperative (VAD callbacks/ASR boundaries and before/after
non-preemptible diarization inference). A cancellation request is not an assertion
that native inference has already stopped. Normal UI calls scope cancellation by
meeting and, where known, version; stale job cancellation cannot cancel another
meeting's refinement. The old unscoped command remains compatible with legacy
callers. Panics of the owned native task are monitored and recorded as failed.
Native startup marks remaining running versions interrupted, never ready. A
WebView remount does not mark an active job interrupted. Progress events remain
view-session telemetry; durable running/terminal status and saved results are the
recovery authority. Percent/stage is not resumed after a process restart.

Selection takes a SQLite write transaction and compare-and-swaps the expected
revision. If any text/speaker/row changed since review, selection fails visibly.
Before replacement it saves the exact editable projection as a corrections
snapshot, including IDs, all text, speakers and timing. It then replaces canonical
rows and selected pointer atomically. A SQL failure rolls everything back. There
is no heuristic merge of a text correction into a newly segmented ASR result.
Users can restore either baseline or corrections explicitly.

Text correction similarly checks revision and line membership, checkpoints prior
text, updates only transcript text, and leaves row ID/speaker/timestamps untouched.
Pending summary generation blocks selection/correction. Existing summary result,
backup and user-edited flag are archived with the old projection before selected
summary/cache data is cleared. Archived summary text is readable in version review;
it is not silently restored or presented as a summary of new text. Meeting detail
refreshes the React transcript projection after selection/correction without a
window reload. Both rich-text conversions and serialized autosaves drain before
native selection/correction, with editors briefly read-only. Failed persistence
blocks mutation and retains the draft for retry. A native-success/view-refresh
failure has a separate refresh retry, not another destructive operation. Notes
stay mounted, while the archived selected summary is cleared in child and parent
state; pending summary edits are saved before that archive is made. Contact links
are reconciled transactionally without deleting curated contacts. Notes, task
contents/completion/curation, title, audio retention and capture/recovery files
are not rewritten; task navigation anchors alone follow the selected projection.

## Source and speaker contract

ASR processes `mic.mp4` and `system.mp4` separately when both exist. Simultaneous
speech yields independent rows, not a single winner from a mixed track. Candidate
speaker refinement restricts each row to its own source: a mic row may retain
`You` only when the shared native identity resolution permits it; otherwise it
remains neutral `Microphone`. Remote rows cannot acquire `You`, including during
overlap. Capture origin is independent of a diarizer's arbitrary speaker number. Diarization does not
split words or invent word alignment. A disappearing source track fails the
candidate rather than applying mixed attribution. Diarization uses the meeting's
actual saved folder, not a title-based search for some other recording.

Explicit `in_person: true` recordings retain **Microphone** / room-source labels.
They do not require the optional diarizer and do not run voice-profile matching or
shared-room speaker identity inference. Shared-room identification is **outside
MVP**, not an accepted model-accuracy claim. Legacy mixed-only recordings retain
anonymous candidate labels; they cannot establish local-user identity. Original
and corrected source labels remain available in saved previous versions.

This does not solve acoustic echo. Correct dual-track routing is a software
invariant, not evidence that remote speech leaking into the physical microphone
will never be transcribed as local. #9's Zoom-through-speakers remote-only and
simultaneous-speech physical gate remains mandatory on both Macs.

## Integration seam with concurrent #10 (not included here)

#10 owns flagged-session provenance and identity policy, including suppression of
polluted legacy voiceprints and classification of generated `Microphone` in the
standalone diarization path. No unaccepted #10 branch is pulled or copied here.

- Candidate settings preserve the complete native `metadata.json` object under
  `recording_metadata`, without interpreting or discarding unknown policy fields.
  ASR commits a per-row `source_map` (`microphone`, `system`, `room`, `mixed`) beside
  the words. It is not reconstructed from `You`, `Guest`, `Microphone` or names.
- Candidate diarization shares `diarize_meeting_inner` with the ordinary native
  entrypoint. #10's authoritative session-policy resolution must cover **both**
  callers there, using the original recording folder/session, not a temporary
  decoded WAV's parent. The stored metadata snapshot is also available by version;
  #10 must define which policy is authoritative if current metadata changes.
- The candidate adapter only restricts the shared native identity decision: it
  cannot restore a withheld `You`, promote mixed audio, or assign `You` to system
  rows. Physical source routing and permission to name a person remain separate.
  #10 must apply its policy **before** this adapter, not only in the Tauri wrapper.
- Candidate labels are generated (`label_origin: generated_asr`). Candidates do
  not enroll a microphone voiceprint, use legacy voice-profile names, or reinterpret
  existing labels/contact links as manual names. Actual current corrections remain
  readable snapshots rather than automatic inference hints. A `corrections` kind
  preserves a projection; it does not prove every label in it was manually named.
- Selection, correction checkpoints and restore carry settings/provenance unchanged.
  Standalone legacy label classification is deliberately left to #10. Shared-room
  personal identity remains outside MVP. This branch alone does not qualify #10's
  flagged-session policy; test that integration in the final combined build.

## Interfaces for #15 / #16

Native commands registered in `lib.rs`:

- `list_transcript_versions(meetingId)` → `{selected_id, revision, versions,
  audio_available}`. Each version includes kind/status/settings/error/summary JSON.
- `get_transcript_version(meetingId, versionId)` → immutable snapshot segments.
  Internal candidate workers can also read partial JSON; consumers must honor
  status and must not use a running/failed candidate as selected product text.
- `get_selected_transcript(meetingId)` → one transactionally consistent
  `{version_id, revision, settings_json, segments}` snapshot, including current
  corrections. It lazily initializes a legacy baseline without requiring the UI
  to list versions first. Settings preserve policy and source provenance.
- `select_transcript_version(meetingId, versionId, expectedRevision)` and
  `correct_transcript_text(meetingId, transcriptId, text, expectedRevision)`.
- `start_retranscription_command(...)` → `{meeting_id, version_id, message}`;
  existing progress/completion/error event names retained. Completion adds
  `version_id`. Cancellation accepts optional `meetingId` / `versionId`.

`database/transcript_versions.rs` is the transport-free production repository:
`ensure`, `begin`, `finish`, `finish_with_sources`, `version_sources`, `set_status`,
`segments`, `selected`, `select`, `checkpoint_current`, `correct_text`,
`interrupt_running`. `transcript_version_commands.rs` owns Tauri
transport/audio availability; `audio/retranscription.rs` owns native inference
reservation/lifetime; `diarization::diarize_version` only refines a candidate.

Product text requests should capture **meeting + selected version + revision**
and reject/mark stale a result whose revision no longer matches at publication.
#15 must not obtain refined text by assuming the last finished ASR is selected.
#15 owns queue/preemption and can schedule through the same candidate/repository
interface; #16 owns term corrections and can consume `correct_transcript_text`.
This patch is
not a durable priority queue, OS background service, resumable inference checkpoint
or battery scheduler. Starting refinement during recording is rejected and state
is rechecked before processing. Preemption of an already executing model call and
new-call scheduling belong to #15; current native offline guards are unchanged.

## R1 integration corrections

- Additive migration `20261118001401_transcript_version_references.sql` stores
  curated speaker/contact mappings in snapshots and retains each task's original
  source version, row ID and audio time. Version selection updates only task
  navigation, never task text/completion/curation. An original anchor wins on
  restore; otherwise only a selected segment containing the saved time is linked.
  Missing/gap anchors fall back to audio time or the meeting, not a deleted row.
- Contact reconciliation uses the same normalized/Unicode-aware identity helper
  as ordinary person operations, extracted without changing its policy. Explicit
  curated mappings override display-name reconstruction. Old snapshots can use
  associated correction checkpoints/current links; historical associations already
  destroyed by an older build cannot be invented.
- The post-call handoff consumes native ready results on remount and stores a
  meeting-specific ready/deferred marker only after successful callbacks. Failed
  refresh remains visible with an explicit retry which does not rerun ASR. Failed
  or interrupted jobs leave explicit later enhancement available. A changed
  meeting gets separate component state and storage; Enhance remains available
  for a deliberately requested later pass even after a handoff was consumed.
- The normal summary-language resolver deliberately does not read or write the
  old meeting-only detected-language cache. It runs cheap local detection on its
  current text instead, so switch/correction cannot reuse a prior text's hint.
  Explicit language settings take precedence; recording metadata is untouched.
- A call manifest declaring retained `mic_audio_file` or `system_audio_file`
  requires both physical source files. One missing **or both missing** fails even
  with a mixed playback file. Genuine legacy mixed-only recordings remain anonymous.

Combined #10 regression requirements remain OPEN, not replaced by these checks:
resolve/validate original-recording policy **before `ensure_wav`**; force an
external temporary WAV; exercise malformed JSON and valid nonobject manifests;
exercise generated `You` inside both `Alice + You` and unmatched `You + Guest`
through versioned candidate **then selection**. Forbidden generated identity must
not survive, while genuinely authored names/text remain in separately recoverable
corrections. #10 owns those policy/composite fixes. No unaccepted branch is pulled.

## Verification and limits

Device-free tests use **synthetic text, empty placeholder source files and real
SQLite**, not an audio accuracy fixture:

- `cargo test --manifest-path tests/transcript-versions/Cargo.toml`: includes the
  unchanged production repository with only the segment transport DTO mirrored;
  tests candidate isolation, corrections/restore, concurrent/stale revisions,
  rollback, cross-meeting rejection, summary archiving, text correction, startup
  interruption, opaque metadata/source-map retention through correction and restore,
  and read/select without audio. No GTK/Tauri/ASR engine is simulated
  as passing. The same core tests are also native unit tests.
- From `frontend`, run each `tests/transcript-versions/*.test.ts(x)` file as
  separate Bun invocations: normal review/select/correction controls, audio-missing
  state, stale error, post-call native-start entrypoint, remount and cancellation.
- `frontend/scripts/test-retranscription-sources-portable.py`: extracts production
  source/routing policy plus room/call/overlap and native-denial monotonicity
  regressions (7 tests). No model is run.
- `frontend/scripts/test-recovery-database-portable.py`: updated to include the
  production version repository and migration, while preserving the existing
  actual recovery API/SQLite/FFmpeg command tests. #9's proof/cleanup semantics are
  not replaced by version snapshots.
- Full native tests and frontend type/build checks are separate from the portable
  checks; record their actual exit/results in the handoff. Installed/physical/model
  qualification is not implied by any device-free pass.

### Baseline device-free results (head `3cd862a`, before R1 changes)

- Production SQLite version repository: **15 passed** (synthetic RU/EN text).
- Extracted production source/identity helpers: **7 passed** (empty source-file
  placeholders; no acoustic inference).
- Normal version UI: **5 passed**; post-call entrypoint/remount: **2 passed**, in
  separate Bun invocations with mocked Tauri transport.
- Existing recovery API/repository/FFmpeg suite: **13 passed**, using real SQLite
  and FFmpeg with generated nonprivate fixtures; 50 unrelated tests filtered out.
- Frontend TypeScript: **passed**, `tsc --noEmit --project frontend/tsconfig.json`.
- Next production build: **passed**, including lint/type validation and all 12
  static pages. Existing outdated Browserslist and ambiguous Tailwind-class
  warnings remain; no release packaging or publication was performed.
- Modified native audio/diarization modules parse with Rustfmt. This is a syntax
  check, not native type checking or linking.
- Full native `cargo test --lib transcript_versions --no-default-features`:
  **blocked before project compilation**, `glib-sys` cannot run missing
  `pkg-config` for `glib-2.0`. No system packages were installed to bypass it.

Snapshot retention currently has no automatic pruning; storage failures are
reported, not permission to overwrite a prior selected version. Native inference,
model accuracy, installed-app behavior and #10 policy integration are not
established by these device-free checks.

### R1 verification on 2026-11-18

Red regressions were reproduced against `3cd862a`/the pre-fix normal callers:
page reload and unflushed drafts, normalized contact loss, dangling task source,
ready-remount/hidden refresh failure, stale detected language and both declared
tracks missing. A separate legacy-snapshot red reproduced curated-link loss;
fractional task navigation reproduced `1.25` seconds being truncated to `1`.

Current targeted checks:

- Production SQLite version repository and shared normalized identity helper:
  **18 passed**, including old/new association snapshots, task restore, absent/gap
  anchors, original source-version retention and reference rollback.
- Production source/identity helper suite: **9 passed**, synthetic text and empty
  track placeholders only. No acoustic/model test is implied.
- Separate Bun invocations: version UI **6 passed**; actual PageContent +
  MeetingDocument/NotesTab + autosave + version controls **3 passed** (rich-editor
  widget and native transport mocked, including delayed conversion and injected
  storage failure); handoff **7 passed**; normal summary-language resolver **2
  passed** without a provider call; task-list navigation **1 passed** with exact
  fractional time, audio-only fallback and meeting-only fallback.
- Existing recovery API/repository suite: **13 passed**, real SQLite and FFmpeg
  generated nonprivate fixtures; **53 filtered**. Recovery semantics are unchanged.
- Standalone TypeScript passed. The production Next build also validates types
  and lint; final result is recorded in the coordinator handoff.
- Native edited modules parse with Rustfmt; full Tauri/native type/link tests
  remain unqualified here because GTK/pkg-config prerequisites are absent, as
  recorded above. No packages were installed to disguise that limit.

The page tests use an extended Bun timeout on this host; an initial harness run
hit the default 5-second timeout under I/O contention and was not a pass. The
passing runs are separate completed invocations. Injected storage errors in the
page suite are expected assertions, not a successful-write claim.

### Final combined Mac qualification (NOT RUN here)

On each target Mac (M4 Pro/48 GB, M1 Pro/32 GB), use the final integrated app only;
no packaging or Mac operations are part of this implementation batch.

1. Save one nonprivate RU/EN speech fixture with reference text and explicit
   ground-truth source intervals, including domain terms, remote-only playback
   through speakers, near-end-only and simultaneous speech. Record audio SHA-256,
   track provenance, device/routing, models, languages/settings and app source SHA.
2. Keep the original live version, correct a text term and rename/reassign a
   speaker through normal controls. Run local refinement on **the same saved
   audio**. Confirm the original and corrected version are still readable and no
   automatic selection has occurred. Compare errors/omissions/terms per source,
   not model parameter counts. A larger model alone is not an improvement.
3. Check original versus refined term accuracy and human-aligned word errors,
   missing speech, false `You` on remote-only audio, both call rows on overlap,
   and timestamp/playback alignment. Report regressions, not only averages.
   Other participants' individual identities are not guaranteed.
4. During a pass, edit selected text/speakers from another view, cancel during VAD,
   ASR and diarization, leave/reopen the view, and terminate/restart the app.
   Ready results must remain separate; stale selection must fail; interrupted
   passes must not replace selected text. Retry and restore corrections.
5. Delete saved audio under the existing retention policy. Reopen the meeting:
   refinement must be explicitly unavailable, all saved text versions readable.
   Exercise #9 audio-only and post-commit recovery retries before/after selection;
   an old recovery draft must not restore its rows over the selected version.
6. Confirm no audio or text-provider request is made by refinement. #15's approved
   subscriber CLI authorization, unavailable-network behavior and #15's priority
   queue require their own qualification, as do #16 term corrections; none is
   established by this patch.

AC on quality-before/after and physical local/remote attribution remains **open**
until that evidence exists. No real model/audio tests, Mac qualification, installer,
GitHub publication or release is claimed by this document.
