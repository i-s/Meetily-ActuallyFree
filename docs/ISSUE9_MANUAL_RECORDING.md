# Issue #9: manual recording reliability and explicit source control

**Live issue:** [#9 — [macOS 05/16] Надёжная запись с ручным управлением](https://github.com/i-s/Meetily-ActuallyFree/issues/9).
Read 2026-11-18, including the complete body and comments (zero comments).
This is application software work, not a physical acceptance result or release.

## Base decision and integration

- Assigned base: `6dd23572ab4d60b1be088a25104c97d4e89d977c`.
- First integrated accepted #7 `1b7fec1aa9a636773c7c5549299127b8e09f54bd`
  with `cherry-pick -x`: **`a5c884bc3fcef4346a61dea1e585eb8196e3b0a3`**.
  The development-map change was additive, preserving #6/#8. The independent
  #7 note, Swift probe, Python probe and tests match the accepted tree exactly.
- Read checkout AGENTS/development map, #5–8 qualification notes, and the
  separate planning checkout's AGENTS, glossary, agent guidance, requirements,
  macOS specification, ADR and research. Live #5–8 are open with no comments.
- Reconsidered the foundation against stages 1–4: #5 has accepted idle-capture
  software, #6 has evidence tooling but **no qualified acoustic separation**,
  #7 explicitly has **no selected/qualified automatic join/leave mechanism**,
  and #8 has instrumentation but **no two-Mac 90-minute/model/latency pass**.
  Retain Meetily **provisionally for feasible manual-recording software**, not
  as a proven solution to the mandatory speaker/automation requirements. The
  final foundation decision remains gated by those measurements; this ticket
  does not relax the requirements or substitute manual start for automation.

## Prior fork work reused, not imported wholesale

Checked `origin/fix/macos-system-audio-idle`, `origin/local-fix-base-0.2.18`
(at `d9cb04f4`) and `origin/fix/codex-call-detection` (`2169cab`). #5 already
restored the relevant idle/overflow correction. #7 records why the AX/call
candidate is not a qualified macOS Zoom/Discord automatic adapter; none of that
large branch was imported or enabled here.

The still-applicable **FFmpeg checkpoint/save deadlock** in
`d9cb04f4ff9bc2560be30853863e3c89dd713102` was missing from the assigned base:
writing all PCM before draining diagnostic pipes can stop both processes at a
30-second checkpoint. Ported only the encoder helper, frozen capture duration,
retained stop-state accounting, shutdown-stage display and focused regressions.
No Codex/provider, workflow, publication, packaging or unrelated UI changes from
that commit were imported. Prior physical observations are historical evidence,
not a physical pass for this tree.

## Application entrypoints and ownership

- **Settings → Recording → In-person meeting** persists `in_person` in native
  recording preferences (old stores default to false). Both native start paths
  configure `RecordingManager`, which validates sources before capture and
  suppresses both system-device and per-app capture in this explicit mode.
  Ordinary Start, Pause, Resume and Stop remain the app/tray/minibar commands.
  The live card displays microphone-only mode from the native state snapshot.
- Session mode is frozen by native start in `audio/recording_mode.rs`; editing
  preferences does not relabel a running session. Room-mic text uses
  **Microphone**, not an assertion that everyone in the room is the user. It
  does not train the online diarizer as the user's voice. `metadata.json` records
  the mode, and offline diarization skips the mic→You dual-track shortcut for
  these meetings. Source labels are not person names.
- Chosen-app capture resolves a Unix PID to the pinned cidre HAL `Process`
  object before constructing `TapDesc`. Unknown process objects fail startup.
  A tap failure never falls back to a global tap. Empty chosen-app selection
  fails before opening streams. Invalid/unreadable saved source preferences
  fail closed instead of substituting global-audio defaults.
- A selected-app process-lifetime guard shares the Core Audio task lifetime.
  It checks the **same PID** once per second off the capture/Tokio worker,
  stops visibly on process disappearance/check failure, and never retargets to
  another helper/global audio. Silence, muted calls and paused recording do not
  themselves mean process exit. Normal Stop drops/cancels the guard with its
  owning task. Helper migration is not automatically followed.
- Requested system-stream creation failure now fails Start visibly rather than
  claiming successful mic-only recording. A missing output is an error unless
  microphone-only mode was explicitly chosen. Startup failure stops any already
  opened mic and flushes/stops its pipeline. Existing stream/device EOF/error
  callbacks still drive the native fatal-error/save path.
- Capture still submits independent mic/system audio. No remote-triggered mic
  mute, echo-text deduplication, correlation suppression or VAD deletion was
  introduced. Mixed playback and both source tracks keep their prior clocks.
  **You in ordinary call mode remains a source convention, not verified
  identity. Speaker playback leaking into a mic is not solved by this change.**

## Save, failure and recovery behavior

The pipeline's audio saver remains independent of ASR completion. One saver
owner serially awaits blocking checkpoint work off Tokio; FFmpeg's borrowed-PCM
writer and diagnostic drain progress concurrently and reap the child. No
recording-sized PCM copy or detached input writer was added by the pipe fix.

- First checkpoint or incremental-transcript storage failure records its error, reports a terminal
  `StorageFailed` to native recording state and emits the existing visible
  recording-error/save-completion flow. Disk/permission errors are no longer
  just logged while the UI continues claiming a good recording.
- A checkpoint is encoded to `.pending`, synced and promoted to `.mp4` only
  after success. Recovery ignores pending outputs. Failed encoding keeps the
  in-memory source buffer; committed checkpoints remain on disk.
- Stop freezes the capture end time and accounts for an outstanding pause.
  Pipeline cleanup no longer erases timing/errors before final metadata save.
  Saver drain precedes final track locks. Every populated track is attempted
  even if another fails; source-track failure is no longer silent/best-effort.
- Per-track finalize keeps checkpoints. Only after **all populated tracks,
  transcripts and metadata** succeed does meeting-level save clean them up.
  Failure preserves available checkpoint sets and reports an audio-save error.
- Recovery explicitly specifies the MP4 container for `.mp4.recovering`; FFmpeg
  cannot infer it from that suffix. It syncs recovered output before promotion,
  retains source tails rather than truncating them to mixed checkpoint count,
  and refuses to overwrite existing audio. An existing-output conflict is
  intentionally reported for manual review; it is not destructive repair.
- Manual Stop no longer auto-deletes takes shorter than ten seconds. An empty
  live transcript is not proof that a recording is worthless. ASR UI timeout
  still saves available text/audio instead of skipping meeting save. Failed
  database save retains folder/recovery references for retry.
- Main-window Close remains the native hide-to-tray handler; it does not close
  capture. Native tasks and native transcript listener own recording, not the
  visible recorder component. The existing save-to-library/detail path remains.
- WebView mount installs listeners before its native snapshot. Native recording
  restores lifecycle status **and polling**, including pause state. Late polls
  cannot overwrite newer lifecycle events, late listener registrations clean up,
  and shutdown progress does not masquerade as completion. Detached-manager
  snapshots preserve the last duration and report native `is_stopping`.
  All native stop entrypoints publish Stopping to the tray; a menu refresh during
  drain consults the native stop guard rather than reverting to Recording.

## Acceptance criteria — evidence, not checkbox inflation

| Live AC | Software/evidence status |
| --- | --- |
| 1. Review results 1–4, reconsider foundation if mandatory audio/automation unmet | Reviewed and provisional decision above. No physical pass or final foundation approval inferred from prep. |
| 2. Qualified approach in normal meetings; remote-only not You; preserve local double-talk | Independent tracks preserved; synthetic simultaneous-source tail regression passes. **Blocked:** #6 physical acoustic/ASR/attribution acceptance. Source routing is not acoustic separation. |
| 3. RU/EN text with time/source, ≤10s target, audio despite ASR delay | Existing local ASR/time/source path preserved; saver tested without an ASR consumer. **Pending:** actual RU/EN/mixed models, speech-to-WebView ≤10s, concurrent backlog/resource measurements from #8. |
| 4. Manual start/pause/resume/stop, in-person, actual menu-bar state | Existing app/tray/minibar controls retained; explicit in-person capture, frozen pause/stop clocks, native stop snapshot and reload restoration implemented/tested. **Pending:** real macOS menu/window/control/device exercise. |
| 5. Visible permissions/device/stream/storage failures; no silent source widening | Fail-closed source/tap/preferences, startup rollback, selected-PID guard and storage failure path implemented. Portable injected/stream tests pass. **Pending:** macOS compile, real permission denials, unplug/default-route/helper transitions and real low-space exercise. |
| 6. Continue after window close, reopen saved meeting | Existing native hide-to-tray and saved-library flow preserved, reload/state/short-take retention hardened. **Pending:** installed Mac close/reopen/restart and meeting reopen/DB/data-integrity check. |

## Verification

Run from the task checkout, with Python 3.11+, Rust/Cargo, real FFmpeg and
frontend prerequisites on PATH:

```sh
python3 -I frontend/scripts/test-recording-shutdown-portable.py
python3 -I frontend/scripts/test-audio-idle-portable.py
python3 -I -m unittest discover -s tests -v
# From frontend/, one process per test file:
pnpm dlx bun@1.3.10 scripts/test-isolated.mjs
pnpm run build
# Separately, never during Next build:
cargo test -p meetily --lib --no-default-features recording_mode --locked
```

Portable shutdown harness includes production encoder, state, incremental saver,
recording saver and mode policy. It removes only Tauri command annotations from
a temporary copy, stubs desktop event/device/default-folder boundaries and
extracts folder-creation functions verbatim. Repository-lock verification rejects
new dependency versions. It is **not** a native Tauri/macOS build.

Actual 2026-11-18 checks:
- 22 shutdown/storage/recovery Rust tests passed, real FFmpeg 7.0.2 static on
  Linux. Three-track test queues 35 seconds of simultaneous 440/660 Hz signals,
  verifies a completed 30-second checkpoint plus final tail, decodes all tracks,
  and checks each source's own frequency without cross-source mixing. The
  playback mix retains both. These are synthetic tones, not speech/AEC/ASR.
- 25 continuity/idle/process-guard Rust tests passed (includes new selected-app
  exit while paused and live-but-silent process cases). Stream traits/OS liveness
  are fixture boundaries, not real Core Audio/HAL observations.
- 60 portable Python tests passed, including integrated #6–8 preparation.
- All **29 frontend test files** passed in isolated Bun 1.3.10 processes; the
  final Next production build passed, including TypeScript validation. Existing
  Browserslist/Tailwind ambiguity warnings remain. Reload-status and automatic
  short-take deletion tests were each
  observed failing before their fix, then passing. Recording-settings test
  exercises the normal persisted in-person option while preserving app choices.
- Full application Cargo test was attempted and failed before app compilation
  in `glib-sys`: this environment lacks `pkg-config` and desktop development
  prerequisites. The available minimal Rust toolchain also lacks `rustfmt`.
  macOS-specific application code has **not** been compiled here.

## Remaining limits and final combined gate

- No physical audio, real speech/model, two-Mac latency, acoustic separation,
  automatic call detection, local install or release is qualified here.
- Taps returning silence cannot be reliably distinguished from permission
  denial/quiet output by the legacy permission-support boolean. Existing audible
  permission probe and system-level advice remain the honest diagnostic path;
  do not label ordinary silence a confirmed denial or call exit.
- Chosen PID can differ from the actual client audio helper. No helper graph,
  migration or PID identity inference is invented. Verify client versions and
  intended app-only coverage with unrelated sound playing on each target Mac.
- Existing ASR and saver channels are unbounded; this work does not claim a
  qualified 90-minute backlog/memory bound. #8 must establish service headroom.
  A checkpoint/OS encoder hang other than the fixed pipe deadlock can still
  reach the existing stop timeout. Checkpoint crash loss can include the current
  uncommitted 30-second buffer; synced checkpoints are the recovery boundary.
- The existing-output recovery guard deliberately requires review rather than
  overwriting a potentially good recording. No private/user files were touched
  by these tests; they use isolated temporary synthetic directories.

Coordinator's independent review and one final combined macOS build/physical gate
must cover both M4 Pro/48 GB and M1 Pro/32 GB (or explicitly record the missing
machine), headphones/speakers, remote-only/near-only/double-talk, RU/EN/mixed,
paused/resumed clocks, selected-app exit/helper changes, denied permissions,
disconnect/EOF, low storage, window hide/reopen, saved meeting reopen and restart
recovery. Retain #6's source/text/timestamp evidence and #8's latency/resource
journals. No DMG, packaging, Actions trigger, push, release, installation/data
operation, merge to main, issue closure or subagent is part of this work.
