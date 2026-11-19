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
- macOS selected-app capture explicitly supports **one app at a time**, not
  “first available app.” Preferences reject new multi-app saves; the picker
  blocks a second app and offers a clear/repair action for legacy multi-lists.
  Native stream preflight rejects unsupported counts and any unavailable target
  before opening even the microphone; app-stream open rechecks availability.
  Other multi-app platforms must resolve the entire list, never a silent subset.
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

Shared in-person microphone identity/diarization is **outside MVP**. Enhance /
Re-transcribe reads explicit `in_person` provenance and uses neutral
**Microphone** (including mixed-track fallback), never manufactured **You**.
Generic source labels are not preserved as manually named people; real manual
names can still survive overlap-based retranscription. Legacy call recordings
without the mode field keep their existing mic/system convention; malformed
metadata fails rather than guessing. No new model or room identity inference is
introduced. The mandatory MVP gate remains **Zoom through speakers**: remote
speech must not be attributed to You and local overlapping speech must survive.
Neutral relabeling or muting all mic audio does not satisfy that gate.

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
- Normal finalization and recovery share `audio/track_commit.rs`. Each track has
  a synced `.audio-commits/<track>.json` proof recording checkpoint names, byte
  lengths/SHA-256, output SHA-256, decoded duration and the staged basename.
  FFmpeg explicitly uses MP4 for the unique `.recovering` stage. Proof precedes
  exclusive hard-link promotion; retry resumes a proven stage/output rather
  than stopping merely because `audio.mp4` exists. Mixed, mic and system tracks
  are attempted independently. Legacy partial output can be adopted only when
  byte-identical to the newly validated checkpoint concatenation.
- Cleanup revalidates proof/output/inputs and removes only proven checkpoints.
  Unknown or modified output, changed inputs and uncommitted `.pending` tails
  survive; a source failure or pending tail yields a visible partial result.
  Interrupted promotion and partial cleanup are retryable without replacement.
  Existing-output conflicts require review, not destructive repair.
- `useTranscriptRecovery` → `StorageService` → `api_save_transcript` now accepts
  zero-transcript recovery only with validated final playback. Startup checks
  final audio as well as checkpoints and never clears the durable folder ref on
  validation failure. Native validation rejects empty ghost meetings. Recovery
  IDs generate deterministic SQLite IDs; the transactional folder check also
  finds an ordinary save whose response was lost. Retries never rewrite an
  already committed title/text/timestamps/speaker labels. DB/IndexedDB failure
  retains references. Partial source recovery saves playback to the library but
  retains the draft/checkpoints for retry and review.
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
python3 -I frontend/scripts/test-retranscription-sources-portable.py
python3 -I frontend/scripts/test-recovery-database-portable.py
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
- 36 shutdown/storage/recovery Rust tests passed, real FFmpeg 7.0.2 static on
  Linux. Three-track test queues 35 seconds of simultaneous 440/660 Hz signals,
  verifies a completed 30-second checkpoint plus final tail, decodes all tracks,
  and checks each source's own frequency without cross-source mixing. The
  playback mix retains both. These are synthetic tones, not speech/AEC/ASR.
- 25 continuity/idle/process-guard Rust tests passed (includes new selected-app
  exit while paused and live-but-silent process cases). Stream traits/OS liveness
  are fixture boundaries, not real Core Audio/HAL observations.
- 60 portable Python tests passed, including integrated #6–8 preparation.
- All **32 frontend test files** passed in isolated Bun 1.3.10 processes; the
  final Next production build passed, including TypeScript validation. Existing
  Browserslist/Tailwind ambiguity warnings remain. Reload-status and automatic
  short-take deletion tests were each
  observed failing before their fix, then passing. Recording-settings test
  exercises the normal persisted in-person option while preserving app choices.
- Full application Cargo test was attempted and failed before app compilation
  in `glib-sys`: this environment lacks `pkg-config` and desktop development
  prerequisites. An extracted Rust formatter was used for the new commit module
  and native caller test; this is not application compilation.
  macOS-specific application code has **not** been compiled here.
- Four room retranscription policy tests passed, including repeated Enhance,
  real manual-name preservation, neutral fallback and legacy behavior.
- Thirteen recovery command/repository tests passed with real SQLite and FFmpeg:
  actual native command bodies, empty-audio rejection, final-audio/no-checkpoint
  recovery, DB failure/retry, commit-response loss, duplicate prevention, folder
  identity mismatch and committed text/label preservation. Only desktop transport,
  device metadata and unrelated contact linking are stubbed. Portable harnesses
  do not qualify the Tauri ABI or macOS capture.
- Independent review findings were reproduced red before fixes: room mic became
  You, stale macOS multiple selection promised first-app capture, promoted mixed
  output blocked source retry, and zero-transcript/DB-response retry failed or
  duplicated library entries. Production policy/track commit, UI → storage and
  native command → repository tests now pass. The final pending-tail regression
  initially exposed a Rust iterator ownership compile error; it was corrected
  and both recovery suites rerun successfully.

## R2 follow-up: linear checkpoint cleanup (2026-11-18)

R2 confirmed the four R1 fixes, then found a cleanup I/O regression: the full
playback SHA-256 was recalculated before each checkpoint unlink. Normal awaited
Stop and recovery both reach `incremental_saver::cleanup_checkpoints` →
`track_commit::cleanup_checkpoint_track`. The follow-up changes only this track
module and this note; commit-proof format and capture/recovery interfaces stay
unchanged.

Cleanup now hashes the output once and each remaining checkpoint once, proving
**all inputs before deleting any**. Reads use open file handles, bracketed by
identity/metadata checks. A retained output handle pins its identity; checkpoint
handles are short-lived to bound open descriptors. Before each unlink, bounded
metadata checks compare regular-file type, length, modification time and stable
identity (Unix device/inode plus nanosecond ctime; Windows volume/file index and
write time, with deny-write/delete validation handles). Any mismatch, missing
file, metadata/open error or unsupported identity preserves remaining inputs.
Unknown artifacts and pending tails remain outside deletion authority. Existing
proof validation in the command adds one other full pass; there is no full-output
hash in either deletion loop.

Deterministic tests count **actual bytes fed to SHA-256**, not elapsed time or
inferred file sizes. Folder-scoped accounting crosses `spawn_blocking` and
excludes setup/encoding and other fixtures. With real FFmpeg 7.0.2 MP4 fixtures:

| Checkpoints | Output bytes O | Remaining input bytes I | Old cleanup helper | New helper (limit O+I) | Old command | New command (limit 2×(O+I)) |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 18,059 | 18,059 | 72,236 | 36,118 | 108,354 | 72,236 |
| 4 | 69,830 | 72,236 | 493,622 | 142,066 | 635,688 | 284,132 |
| 12 | 207,886 | 216,708 | 3,135,934 | 424,594 | **3,560,528** | **849,188** |
| 24 | 414,970 | 433,416 | 11,241,082 | 848,386 | 12,089,468 | 1,696,772 |

Both scaling tests fail against the original `ed96efa` cleanup routine and pass
with this patch; every green measurement equals its linear upper bound. These
are hash-I/O counts, not a macOS Stop-duration benchmark. Per-file metadata work
and stored stamps are O(checkpoint count), with bounded descriptors. Summed over
tracks, content work is linear in total output and remaining checkpoint bytes.

Controls cover changed last input before deletion (nothing deleted), absent
proof (nothing deleted/hashed), same-content output/input inode replacement
after validation, same-length input mutation with restored mtime (ctime catches
it), output mutation after the first unlink, and partial retry. The latter keeps
three inputs, restores/revalidates output and hashes exactly **124,007 bytes**
(output plus only those three inputs). Existing conflict, interrupted promotion,
pending-tail, partial-cleanup, normal Stop and recovery/database tests remain.

Affected portable production-path runs pass: **43 shutdown/storage/Stop/recovery
Rust tests**, and **13 native recovery-command/SQLite/FFmpeg tests**. No frontend,
model, device or physical qualification is implied; unrelated suites were not
rerun for this native-only patch. Evidence logs are
`/tmp/issue9-r2-command-scaling-red.log`, `/tmp/issue9-r2-cleanup-green.log` and
`/tmp/issue9-r2-recovery-green.log` in the task environment.

Concurrency limit: the application must finish checkpoint writing before
cleanup, and ordinary filesystem identity/mtime/ctime reporting must be reliable.
The final metadata check and unlink are not a portable atomic compare-and-unlink;
a hostile/external writer racing that gap, coarse/stale network metadata, or
privileged metadata manipulation is not excluded. Detected ambiguity stops
cleanup; retry always obtains fresh content proof. Windows denies writes while
validation handles are open, but checkpoint handles must close before unlink,
so no universal external-writer exclusion is claimed. Linux production-module
tests exercised the Unix guards. The macOS and Windows branches have not been
compiled or physically exercised in this environment.

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
