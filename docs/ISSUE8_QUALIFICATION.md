# Issue #8: duration, local ASR and subscription Codex qualification

Live issue (read with comments on 2026-11-18; no comments returned):
**[macOS 04/16] Проверка 90-минутной встречи и Codex CLI**
<https://github.com/i-s/Meetily-ActuallyFree/issues/8>.
Base: `f798c6c6d94060c4a135fdabcad41a06426784b3`, including accepted #6 software prep.

**Implemented instrumentation; physical acceptance NOT RUN.** No selected Mac
configuration is justified yet. No DMG, installation, release, or private-data
operation is implied by this work. The #6 dependency still requires physical evidence.

## Prior work and interfaces inspected

The checkout map, recording/transcription worker and pipeline, #6 probe, local
planning AGENTS/docs/agents, glossary, requirements, specification, ADR and Codex/audio
research were inspected before implementation. The fork already separates mic and
system turns; #5 retains idle-audio handling and #6 provides non-destructive source
observations. Do not reintroduce mixing, infer source from `You`, claim AEC, or
substitute models. No existing Codex provider was found in this base. The runner
here is **qualification only**, not the future in-app Codex provider/Notes feature.

## Software added

- `audio/transcription/timing.rs` and `worker.rs`: optional text-free diagnostics.
  Launch the app binary with `MEETILY_ISSUE8_TIMING=1`. Each dispatch logs
  `MEETILY_ISSUE8 {"kind":"asr_queued",...}`; each worker completion logs
  `asr_chunk` with chunk ID, independent capture source, recording-relative audio
  start/duration, monotonic dispatcher queue/service/total seconds and outcome.
  `asr_drained` records dispatcher completion counters after workers join.
  The default does not allocate a timing clock or emit these diagnostic records.
  Capture remains untouched; timing lives on the existing dispatcher/worker path.
  Early exits/panics have `aborted`; ASR errors have `failed`; empty results are
  `empty`, never claimed as successful speech recognition; event failures have
  `emit_failed`. This does not change the pre-existing completion counters, which
  count attempted work, not successful words.
- `scripts/issue8_qualification.py events LOG`: reads diagnostic lines and reports
  unfinished IDs, duplicate completions, count/drain mismatch and bad outcomes.
  One recording per log is required: chunk IDs restart between sessions. The
  existing application log may contain transcript text; keep it local and private.
  Only the new diagnostic records are text-free. Never upload the unfiltered log.
- `sample PID --duration 5400 --interval 5`: process RSS, best-effort process swap,
  and (on macOS) system swap usage/power source. Units are bytes; timestamps are
  monotonic elapsed seconds. Missing metrics are null/unavailable, never zero.
  macOS process swap is deliberately null; system swap is a host observation,
  not an attribution to Meetily. Sampling does not include every child/sidecar.
  Run a separate sampler for each relevant process, preserve PID/start metadata,
  and corroborate in Activity Monitor. The JSON is printed on completion: an
  interrupted sampler is missing evidence, not a complete run.
- `codex --timeout 60`: only the embedded owned RU/EN/mixed synthetic prompt is
  sent through installed Codex CLI stdin. No audio/transcript/Notes path accepted.
  ChatGPT login must be confirmed; API-key routing env vars are removed. The CLI
  runs ephemeral, read-only, outside the repository, ignoring user config/rules,
  with official OpenAI provider, forced ChatGPT login, shell tools and web search
  disabled. No response/stderr text is saved, only counts/hashes/timing/version.
  Timeout kills the process group. Auth/version/CLI failures are explicit; no
  paid-API or alternative-provider fallback. This is not a general privacy sandbox
  for arbitrary work text; only the fixed synthetic input is qualified here.
- `qualification JOURNAL.json`: checks the combined four-run matrix, exact model/
  corpus metadata, 90-minute duration, speech/display landmarks, resource/audio/
  queue evidence, concurrent synthetic subscription Q&A and a configuration
  decision. Outputs `blocked`, `failed` or `manual_review_required`, **never pass**.
  File references/attestations must be independently reviewed. A structurally
  complete invented journal is not physical evidence.

The worker clock begins **after VAD and the inbound pipeline queue** and ends
near native event emission, not WebView paint. `total_seconds` is not physical
speech-to-display latency; subtracting audio duration would not fix that. Enqueue
and completion IDs expose dispatcher backlog but cannot expose a stuck pipeline
before dispatch. Physical speech/display landmarks and saved-audio review remain
mandatory. Existing work queues are unbounded; this patch measures rather than
silently imposing a drop policy or claiming to solve overload.

## Reproducible final combined Mac qualification (coordinator-owned)

Execute only after separate approval for the **one final combined DMG**, backup/
rollback and physical operations in the issue #5/#6 protocol. This document does
not authorize any installation or network/power change. Use owned synthetic speech,
no private meeting, credentials, names or recordings. Evidence stays outside git.

### 1. Pin the candidate and common material

Record source SHA, final artifact SHA-256, installed app hash/version, macOS,
CPU/RAM, audio route/volume, input/output device IDs, selected diarizer, local ASR
engine and exact model filename/version/hash, language/decoding/VAD settings and
acceleration. Verify that no remote ASR provider is selected. Record CLI version
and subscription login status without reading auth files. The runner requires a
CLI supporting its explicit flags; failure means update/review separately, not
silently drop safety flags.

Use exactly the same owned reference text, synthesized/local-recorded WAV and
schedule for every candidate and Mac. Suggested six phrases, repeated at fixed
landmarks with silence and double-talk cases between them:

1. RU: «Встреча начинается в десять часов. Обсуждаем план релиза.»
2. RU: «Следующий шаг — проверить сохранность записи и очередь задач.»
3. EN: “The meeting starts at ten. We are discussing the release plan.”
4. EN: “The next step is to verify the recording and the task queue.”
5. Mixed: «Проверяем release plan и terminology в локальном transcript.»
6. Mixed: “Please check сохранность записи before the next release.”

Save reference and audio hashes, synthesis tool/voice/version/seed (if synthetic),
sample rate/channels, exact observed speech boundaries and transcript rows. Keep
RU/EN/mixed phrase quality separately: word substitutions/deletions/insertions,
terms/numbers, source-label errors. A phrase match is not a WER or retention pass.
Run selected Parakeet and/or multilingual Whisper candidates on this same material;
record unsupported-language/errors rather than presume a multilingual model works.
Native ignored model tests were not run here. Do not download/swap models mid-run.
Use a short quality screen to eliminate unusable candidates before 90-minute runs;
retain all compared candidate settings and failures, not only the winner.

### 2. Four final runs, with matched Q&A-off control windows

M4 Pro/48 GB × AC/battery and M1 Pro/32 GB × AC/battery; each at least 5400 seconds.
If battery cannot complete, record the actual duration/failure, never round up.
Use the same chosen configuration and corpus for comparison. Record background
workload, energy mode, thermal state, battery percent at start/end and any power
transition. Do not equate a Linux synthetic run to this matrix.

Start the candidate **binary** from the terminal with the timing env var and a new
private log per recording. Do not rely on `open -a` to inherit the variable. Preserve
preflight/source-track readiness from #6. Identify the real app PID; in another
terminal run (set REPO, EVIDENCE and PID explicitly):

```sh
python3 -I -B "$REPO/scripts/issue8_qualification.py" sample "$PID" \
  --duration 5400 --interval 5 > "$EVIDENCE/resources.json"
# Run at roughly 1, 15, 30, 45, 60, 75 and 89 minutes while ASR is active.
# Give every invocation a NEW output filename, e.g. qna-001.json.
python3 -I -B "$REPO/scripts/issue8_qualification.py" codex \
  --timeout 60 > "$EVIDENCE/qna-001.json"
# After Stop AND final queue drain:
python3 -I -B "$REPO/scripts/issue8_qualification.py" events \
  "$EVIDENCE/app.log" > "$EVIDENCE/asr-observations.json"
```

Use alternating matched Q&A-on/off windows; annotate their recording-relative
start/end, so audio integrity and queue/lag trends can be compared, not merely
co-occurrence asserted. Ask during real ASR work, not silence. Measure speech-end
→ first stable visible text using a shared monotonic recording clock/screen record
or timed synthetic landmarks, at least every minute and around each Q&A, including
the beginning and final minute. Annotate `speech_end_seconds` and `display_seconds`
with clock alignment/uncertainty; do not convert wall-clock HH:MM:SS strings into
recording time. Preserve maximum/p50/p95 and early/middle/late lag, queue/service
trend, RSS, system swap and final drain. Silence is not hidden backlog; missing
speech/display or missing metrics are missing evidence. A growing queue can exist
before the target is exceeded; flag sustained growth and bound final drain time.

### 3. Failures and saved-audio integrity

With owner approval, include unavailable-service/offline and cancellation/timeout
windows while recording synthetic speech. Never alter the owner's global network
without permission. Record actual failure/elapsed time, then retry after service
returns. Recording/ASR must continue without lost/corrupt source or mixed audio.
A failed CLI invocation must not be reported as a successful answer; <=60 seconds
applies to successful answers when service is available, with outage limits kept.
The CLI is a separate process here; actual in-app provider integration is not tested.

After Stop, preserve/hash the original mic/system/mixed tracks, metadata and
transcripts with the #6 hash command. Decode read-only with FFmpeg; inspect duration,
decoder errors, expected samples/landmarks, beginning/middle/end gaps/drift, each
Q&A/failure window and both sources. Run #6 source/transcript checks and listening
review (remote-only, near-only and double-talk); speaker labels are not independent
provenance. Check original files against backups/hashes. A final file hash alone
does not prove continuity or that Q&A caused no harm.

### 4. Journal and decision

Copy `docs/fixtures/issue8-qualification.example.json` outside git. It starts
`physical_acceptance: not_run`, empty runs, no decision. For each run supply:

```json
{
  "machine": "M4 Pro/48 GB", "power": "ac", "duration_seconds": 5400,
  "app_sha": "actual candidate SHA", "macos_version": "actual",
  "asr_model": "exact filename/version", "model_sha256": "actual hash",
  "model_parameters": {"language": "actual", "decoding": "actual"},
  "corpus_sha256": "actual hash", "resource_evidence": "resources.json",
  "audio_integrity_evidence": "review.json", "asr_events_evidence": "asr-observations.json",
  "audio_integrity": false, "queue_drained": false,
  "rss_peak_bytes": null, "system_swap_peak_bytes": null,
  "landmarks": [], "questions": []
}
```

Replace values with measured facts, not this example. A landmark is
`{"language":"ru","speech_end_seconds":12.3,"display_seconds":14.1}`;
questions include `synthetic_only`, `auth: chatgpt_subscription`, `during_asr`,
`status`, `elapsed_seconds` plus local evidence path and recording-relative time.
Set `physical_acceptance: measured` only after actual runs. Supply `issue6_evidence`
and `configuration_decision` with comparison results/reasons/remaining limits.
Run `qualification journal.json`; independently review all referenced originals.

Choose configuration only if speech/display target <=10 s, successful available-
service Q&A <=60 s, no sustained hidden backlog, final drain reconciles every ID,
all speech/source audio is retained and quality is acceptable in RU/EN/mixed on
both Macs and power modes. No arbitrary memory/swap pass threshold is invented:
record peaks/delta/trend, pressure/thermal issues and whether they correlate with
lag/loss. If any target fails, keep actual result, missing data, mitigation and
next test; do not close #8 or call the hardware accepted. The checker intentionally
cannot certify causality, audio quality, model identity, or physical truth.

## Checks actually run in this Linux worktree

- `python3 -I -B -m unittest discover -s tests -p 'test_issue8_qualification.py' -v`:
  10 passed; portable fake-CLI, resource/event/journal tests. These do not run ASR or macOS audio.
- Full `python3 -I -B -m unittest discover -s tests -v`: 28 passed.
- Existing #6 tests: 15 passed, deterministic synthetic PCM only.
- Real subscription Codex CLI 0.167.2, synthetic prompt hash
  `a43aeb41a9b2b752cc0adb513d87f225fd1bec7d10dd01336f650292e83460ff`:
  first smoke 5.282 s, hardened auth/version runner 4.898 s; return code 0,
  expected synthetic marker, no output text retained. Linux, no concurrent ASR.
- Native Rust timing test/full Tauri compilation: **not run**; `rustc` and `cargo`
  were not available. Coordinator review and native compilation are still required
  before including the instrumentation in a final candidate.
- Physical Macs, real-model/audio tests, 90-minute AC/battery runs, saved-audio
  integrity under Q&A and final configuration decision: **not run / pending**.
