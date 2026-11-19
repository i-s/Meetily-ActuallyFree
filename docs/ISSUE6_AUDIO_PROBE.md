# Issue #6 audio evidence probe and acceptance protocol

Status: **portable preparation only**; physical Mac acceptance was not run. This note
is intentionally separate from the final DMG/issue batch. It records what the probe
can measure without making an unsupported echo-cancellation claim.

## Prior fork fixes checked before writing

The live issue body and comments were read on 2026-11-18 (GitHub API returned no
comments). Its exact title is **[macOS 02/16] Проверка разделения «я / коллеги» через
динамики**. The planning requirements/spec and both audio/call-signals research
notes were read separately from the checkout instructions.

- `40f438a25f9c6055f12c2afedc27c78d9801dccc`: removes the output subdevice from
  the tap aggregate; the current `audio/capture/core_audio.rs` retains tap-only
  construction. This fixes a particular software duplicate, not room echo.
- `599f052dec26f59d44e956fcfea4262300d5d387`: separates mic/system STT windows;
  current `audio/pipeline.rs` feeds independent VADs before playback mixing.
- `7521f0a`: speech onset retention/system speech sensitivity; no reference AEC.
- `aa97cdf` / `56eb2f9`: issue #5's reviewed idle-audio fix and build gates are
  present at this branch base. Their presence is not a new physical qualification.

Current `audio/pipeline.rs` applies mic high-pass and loudness normalization, then
persists aligned source windows plus mixed playback. `audio/transcription/worker.rs`
assigns mic chunks `You` unconditionally, independent of the diarizer result. No
system-reference cancellation was found on that path. A remote voice in mic could
therefore become `You`; **that is a conditional code fact, not the owner's diagnosis**.
No duplicate existing fix was reintroduced and no capture/ASR/label semantics changed.

## Implemented preparation

[`scripts/issue6_audio_probe.py`](../scripts/issue6_audio_probe.py) is a read-only,
bounded probe with three commands:

- `tracks` reads a selected interval from two mono PCM16 WAV source tracks, reports
  RMS/peak/clipping, and scans a bounded lag window for coarse waveform similarity.
  It preserves the files and reports the observed offset; it does not suppress,
  relabel, align destructively, or call ASR.
- `transcript` checks a manually adapted JSON evidence record. Every row must retain
  `capture_source` (`microphone`, `system`, or `unknown`); speaker text is never
  treated as proof of provenance. It flags remote-only rows labeled with an explicit
  local-user label and checks expected near-speech phrase presence in microphone
  rows for near-only/double-talk cases and expected remote-speech phrase presence
  in system rows for remote-only/double-talk cases. These keyword observations only
  support listening and transcript comparison; they cannot establish retention.
- `hash` records SHA-256 for original evidence files without uploading them.

The probe returns `manual_review_required` for every case. A null result, missing
phrase, or high similarity is an observation, not a pass: ASR errors, missing
speech, boundary-spanning chunks, clocks, route selection, and ducking require
human review against source audio. Word-level attribution cannot be proven from
segment-level timestamps.

The example fixture in [`docs/fixtures/issue6-evidence.example.json`](fixtures/issue6-evidence.example.json)
has no recordings or transcript and explicitly states `physical_acceptance:
not_run`. It is a schema/template, not evidence.

## Portable test command and scope

From the repository root:

```text
python3 -I -B -m unittest discover -s tests -p 'test_issue6_audio_probe.py' -v
```

The test creates deterministic synthetic PCM and verifies bounded reads, explicit
offset handling, silence/null behavior, clipping, double-talk non-suppression,
source preservation, Unicode phrase matching, invalid input rejection, and the
example schema. It cannot establish macOS Core Audio behavior, AEC, route leakage,
ASR quality, Zoom behavior, or a physical acceptance result.

## Runnable local evidence commands (future physical batch)

These are instructions, not commands executed on a Mac in this task. Store outputs
outside the checkout, in a private evidence directory; audio/text artifacts must
not be committed or uploaded. Python 3 and FFmpeg must be available explicitly;
the probe does not install either or execute a shell/decoder for you.

For example, set `MEETING` to the owned test meeting directory, `EVIDENCE` to a new
empty private directory, and `REPO` to this checkout. Preserve the original tracks
and `metadata.json`/`transcripts.json` first. Then:

```sh
python3 -I -B "$REPO/scripts/issue6_audio_probe.py" hash \
  "$MEETING/mic.mp4" "$MEETING/system.mp4" "$MEETING/audio.mp4" \
  "$MEETING/metadata.json" "$MEETING/transcripts.json" > "$EVIDENCE/original-hashes.json"
ffmpeg -nostdin -n -i "$MEETING/mic.mp4" -map 0:a:0 -ac 1 -ar 16000 \
  -c:a pcm_s16le "$EVIDENCE/mic.wav"
ffmpeg -nostdin -n -i "$MEETING/system.mp4" -map 0:a:0 -ac 1 -ar 16000 \
  -c:a pcm_s16le "$EVIDENCE/system.wav"
python3 -I -B "$REPO/scripts/issue6_audio_probe.py" tracks \
  --mic "$EVIDENCE/mic.wav" --system "$EVIDENCE/system.wav" \
  --start 10 --duration 3 --mic-offset 0 --max-lag-ms 250 \
  > "$EVIDENCE/remote-ru-tracks.json"
python3 -I -B "$REPO/scripts/issue6_audio_probe.py" transcript \
  "$EVIDENCE/evidence.json" > "$EVIDENCE/transcript-observations.json"
```

Copy the example JSON to `evidence.json`, fill metadata, and adjust each case's
`start`/`end` to **recording-relative seconds actually observed**, not the example
schedule. Annotate transcript rows as
`{"start":12,"end":15,"text":"...","label":"You","capture_source":"unknown"}`.
Saved `transcripts.json` uses `audio_start_time`, `audio_end_time`, `text`, and
`speaker`; copy these values into `start`, `end`, `text`, and `label` without editing
words, labels, or timing. Preserve original row IDs/sequence IDs as extra fields
for traceability. Do not convert wall-clock `display_time` into recording time.
If a recording-relative boundary is absent, record that gap outside the adapter
rather than inventing a timestamp. Add all local-user display-name/manual-label
substitutions to `you_labels`.

**Current interface limit:** `TranscriptSegment` and DB transcript rows persist a
speaker label, not an independent immutable capture source. Use `unknown` unless
you have independent event/source evidence, and describe that evidence per row in
extra fields. The current event's `source` is also a speaker string, not durable
provenance. The probe still flags false local-user labels on unknown-source rows,
but will not count their phrases as confirmed mic/system observations. Compare
source audio by listening. Source-specific replays can supplement that review;
never treat a speaker label alone as proof of input identity.

The WAVs are decoded **processed saved sources**, not raw ADC samples: mic high-pass
and loudness normalization have already occurred. No trim, silence removal, tempo
change, or timestamp reset should be applied during decoding. Record FFmpeg version
and any container start offset/padding; a decoder invocation does not establish
sample alignment. Measure source offsets near the beginning, middle, and end using
test landmarks; if the corresponding mic landmark occurs 0.08 seconds later, use
`--mic-offset 0.08`. The lag report then represents residual mic delay after that
explicit offset. A stable near-zero lag does not establish a common capture clock.

Correlation uses block-averaged waveform at at most 1 kHz, a roughly 5 ms lag grid,
and no more than a 10 s window / 500 ms lag search. It is deliberately coarse:
periodic signals, room response, normalization, sample-clock drift, and double-talk
can mislead it. It does not identify a speaker, separate waveforms, quantify word
retention, or prove acoustic versus software routing. Scan several known phrase
windows, inspect the audio, and retain the measured offset rather than automatically
suppressing or shifting samples. Silence has null dBFS/correlation, not a successful
separation score. Use headphones to listen to individual saved tracks; mixed
playback has its own ducking and is not a reliable Zoom output-level measurement.

## Batched physical protocol after all tickets

This is a **bounded, owner-participating physical stage**, arranged with the owner
for the final batch, not authorization for unattended Mac operations. No Mac
operations are performed now. The owner confirms the test scope, private local
destinations, and owned/synthetic material before that stage starts.

Use one final DMG and record its SHA-256 and the installed binary/source commit.
Do not mix artifacts from different commits. On each target Mac, record macOS build,
Meetily commit, client versions, input/output routes, Mic Mode, volume/gain, selected
system capture process, ASR/diarization versions, and permission state. Use only
owned or synthetic phrases; keep audio and ASR local and disable automatic LLM
transmission. Never add a virtual loopback or replay a system tap merely to create
an AEC reference.

### Required recording preflight before the duration matrix

During that owner-participating stage, on each target Mac:

1. Enable **audio auto-save** and verify the active recording setting, not just
   transcript saving. Confirm a writable, private local recording destination
   outside the checkout and cloud-synced/shared folders, with sufficient free space;
   record the setting and destination in the local evidence. Audio auto-save being
   off discards audio chunks and cannot supply the required source evidence.
2. Make a short owned/synthetic smoke take with both selected sources active, long
   enough to meet the installed build's minimum save duration and include recognizable
   local and remote phrases. Stop it normally and wait for saving/finalization to
   complete; do not assume a brief start/stop or live transcript proves audio saved.
3. In the completed meeting directory, verify that **all three distinct files**
   `mic.mp4`, `system.mp4`, and mixed `audio.mp4` exist, are nonempty, and each decodes
   successfully. Listen to the expected test material in the separate sources and
   mixed playback; retain the save/decode results and hashes locally. A mixed-only
   recording, filename presence alone, or a transcript is not an adequate preflight.

If auto-save, the destination, finalization, any required file, or decoding is
missing, failed, or unresolved, **stop before the 5/30/90-minute matrix** and report
the blocker. Do not spend the duration run without verified source evidence. Repeat
the preflight when recording settings or source selection change.

### Controlled cases and duration matrix

For each route (built-in speakers, then the same microphone with wired headphones),
run both Zoom and Discord only when their actual call is connected. Capture a short
script with 10 seconds of silence before each case:

1. remote-only: unique RU phrase, then unique EN phrase while the local speaker is
   silent;
2. near-only: unique RU phrase, then unique EN phrase with remote silent;
3. double-talk: unique RU and EN phrases spoken simultaneously and then alternating.

Repeat at 5, 30, and 90 minutes for the load/lifecycle batch where requested.
Preserve `mic`, `system`, and mixed tracks before any retranscription. Hash originals,
inspect/annotate start-middle-end clock offset and drift, and compare live text,
source label, transcript interval, and post-call output. Record whether the remote
phrase is audible in mic, whether it is duplicated within system, and whether Zoom
output ducks or returns around local speech. Do not use “mute mic while remote talks”
as a candidate fix: it invalidates double-talk acceptance.

### Timing, controls, ducking, and candidate gate

Use the same mic and fixed device volumes when comparing speakers/headphones;
Bluetooth may switch input/profile and needs its own recorded configuration. Keep
each controlled run separate, with the same RU/EN script. If routing is ambiguous,
a short diagnostic-only remote-only trial with physical speaker volume zero can
help, **only if the system track still contains the remote phrase**. A separate
Meetily-mic-mute control may establish input routing; Zoom mute does not mute
Meetily's independent input. Neither control is a passing double-talk run. Start
with wired headphones to minimize accidental feedback on the controlled second
device. Record any client/OS setting change and revert it before comparisons.

For Zoom ducking, keep a fixed remote playback phrase/level and output volume;
compare client output audibility/physical output level before, during and after
local speech and recording start/stop. Use a fixed-position independent measurement
if available, and record the method/settings; otherwise mark this **unmeasured**,
not “no ducking.” Neither saved system-tap level nor mixed playback level proves
physical speaker level. Record delay to volume recovery and any discontinuities.

No AEC candidate is enabled by this preparation. After the physical diagnosis:

1. If the issue is software duplication/routing or a label/save bug, fix that
   demonstrated mechanism rather than add unnecessary voice processing.
2. If acoustic leakage is established, identify a supported reference path for a
   local AEC candidate. Apple VoiceProcessing availability does not prove access to
   Zoom's render reference. Do not replay a captured tap, rely on Zoom's own AEC for
   Meetily, or claim that voice isolation/noise suppression provides separation.
3. Qualify candidate versus baseline on identical owned phrases and routes through
   capture → source persistence → local ASR → displayed/saved labels. Manually verify
   expected remote speech retention in remote-only **and** double-talk, expected
   local speech retention in near-only **and** double-talk, and zero remote speech
   falsely labeled `You`, as required by the acceptance gate below. Preserve
   text/timing, original sources, and comparison artifacts. Check source timing at
   start/middle/end, device contention, latency,
   route/volume changes and Zoom ducking; include the full duration matrix on both
   Macs. Keep candidate/reference configuration and failures in evidence.
4. If candidate fails a required case, stop qualification and report the failed
   interval and remaining work to the parent. Do not silently substitute an engine,
   upgrade a model as “proof,” gate all mic audio, or package/rerun a DMG here.

A confirmed need for capture/label changes after the final batch is a genuine
remaining blocker to mandatory speaker support, not something this probe resolves.
The one-final-DMG policy is owned by the parent; this task must not create an
intermediate installer just to attempt an unvalidated candidate.

### Decision table

- Remote phrase in mic falls with speakers but remains in system: consistent with
  acoustic leakage; still requires validated capture→ASR→label behavior.
- Remote phrase remains in mic with output physically absent: inspect selected input,
  virtual routes, and software loopback before attributing it to acoustics.
- One copy in each source but two in mixed: likely mixing/route leakage; distinguish
  from two capture callbacks by timestamps and source hashes.
- Clean mic/system source tracks but `You` remains on system text: investigate
  persisted/manual labels, event source, build identity, and timestamp alignment.

### Required manual acceptance gate

Pass only when manual listening to the saved source audio and comparison with the
expected test speech and live/saved/post-call transcripts establish **all** of:

- expected remote speech retained in every **remote-only and double-talk** case;
- expected local speech retained in every **near-only and double-talk** case;
- **zero remote speech falsely labeled `You`** (including local-user display-name
  or manual-label substitutions), in remote-only and double-talk cases alike.

Record the per-case listening and transcript comparison, word errors or losses,
and label findings. Keyword/phrase presence from the probe only supports that
review; it does not prove full speech retention or correct attribution. Zero
false-`You` rows is not a pass if required speech was lost or never captured.
**Missing, failed, or unresolved required evidence blocks acceptance**, including
recording preflight, retention, labels, alignment, and Zoom ducking. Do not convert
`unknown`, `not_run`, an empty transcript, or a boundary ambiguity into a pass.
A failed or unresolved case must name the remaining mechanism unknown and whether
the current source-separated foundation remains suitable. Do not call waveform
similarity AEC, model improvement, or an API capability evidence.

## Acceptance criteria and honest handoff

| Live issue acceptance criterion | Preparation delivered | Acceptance state |
| --- | --- | --- |
| Compare system/mic and text; distinguish acoustic echo, software duplication, attribution | Read-only track/lag/level probe, transcript observations, hash collection, diagnostic controls | Pending real owned Mac tracks and diagnosis |
| Remote-only, near-only, double-talk on speakers/headphones with reproducible configuration | Required auto-save/private-destination/completed smoke-take preflight with all three tracks verified to decode; explicit metadata template, unique RU/EN script, both Macs/clients/routes protocol | Pending owner-participating physical preflight and matrix |
| No false `You` remotely; preserve near speech in overlap; no mic gating | Required manual remote retention in remote-only/double-talk, local retention in near-only/double-talk, and zero remote falsely labeled `You`; keyword observations support review only | Pending actual local ASR/audio/text comparison; missing or unresolved evidence blocks acceptance, not proved by unit tests |
| Candidate in capture→ASR→labels with source timing and Zoom ducking | Candidate gate, clock/route controls, independent-output ducking protocol | Blocked on diagnosis/reference selection and final physical run; no AEC candidate implemented |
| If mandatory case fails, remaining work and foundation suitability | Explicit limits and remainder below; no model/API claim | Preparation documented, final suitability decision still pending |

**Remainder/suitability:** source-separated capture is a useful diagnostic base and
avoids destructive double-talk mixing, but mandatory speaker-mode separation is
**not qualified**. A known unconditional mic→`You` mapping means contaminated mic
speech is a real risk. Obtain baseline source tracks, establish the actual cause,
then select/implement a supported local fix if needed and pass the full physical
matrix. This commit does not solve acoustic AEC or claim all of #6 complete.
Do not close the issue. No install, native Mac build, DMG build/dispatch/rerun,
release, push, merge, external audio/ASR call, or physical experiment was performed.

## Current known dependency and limits

The current pipeline already emits separate mic/system ASR windows and persists
three tracks; mic chunks are intentionally labeled `You`, while system chunks are
kept remote-labelled. The prior Core Audio fix removes a known software duplicate
capture path. Neither fact proves acoustic AEC or physical route behavior. Issue #5
build availability and the final batched physical Mac run remain dependencies.
