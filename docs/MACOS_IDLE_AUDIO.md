# macOS idle system audio: issue #5 correction

## Diagnosis and prior fix

The owner reports that the Actions DMG on M4 Pro / 48 GB, macOS 26.2
(25C56), stops after roughly ten seconds without system playback:
`Audio channel was closed unexpectedly`, followed by the under-ten-second
recording cancellation message. This is user-reported physical evidence,
not an agent-observed capture trace.

Affected artifact: Actions run `44123648263`, attempt 2; DMG SHA-256
`093db4b9b9dacc26eb2d5e9ba23a4d9f725364d2c84c9aeb34fd00659ff9c681`.
Its source is `589f538793cf1c648cfce2e79807a23b7793c466`, tree
`ea192e5a5c6928f8b25890b57d21263c90e3fe6b` (identical to original local
source `bd782bfedabb2f1d5b8057cb89baa3ded1656b93`).

[Prior fork PR #1](https://github.com/i-s/Meetily-ActuallyFree/pull/1)
contains the existing fix:

- `b713871e147c844e32db0f03002fdf322b24c765`: await a Pending tap without
  a silence timeout; flush partial blocks before Pending; register/recheck
  callback wakeups; retain real EOF and sample-rate errors.
- `e24cb12decc5e38ade06b3ee04623877f7fd829a`: make terminal overflow final,
  preventing later callbacks from refilling the buffer after termination.
- PR #1's recorded tested candidate is
  `d9cb04f4ff9bc2560be30853863e3c89dd713102`. Its description records native
  Actions tests and an owner physical preview test. That historical evidence
  does not qualify this new candidate.

Neither audio-fix commit is an ancestor of the affected pinned source. Its
`stream.rs` and `capture/core_audio.rs` match the prior fix's parent at these
paths. **This build omitted the fix; this is not evidence that it regressed.**
PR #1 merged into `local-fix-base-0.2.18`, not the main-based issue #5 source.

The old task uses `timeout(Duration::from_secs(5), stream.next())`. No callbacks
from a silent tap cause `AudioError::ChannelClosed`, which RecordingState treats
as fatal. Recording orchestration emits the error and stops; the frontend's
existing under-ten-second discard explains the second message. Five seconds
is the code timeout, not a claim that the screenshot measures exact timing.
A native callback log from the current Mac is unavailable; physical validation
must still distinguish this reproduced path from another genuine stream failure.

## Scope and test seam

Restore only the prior stream processor, callback handoff and overflow follow-up,
plus their tests. The shared handler serves both global and per-app Core Audio.
CPAL, permission probes/UI, models, VAD, gain, source labels, storage, bundle ID
and migrations are unchanged. Real EOF/rate mismatch remain fatal. Stop aborts
and drops a Pending task without waiting for a new callback. Partial blocks are
sent before idle, not timestamped as new audio after a long gap.

The portable runner includes the production processor, RecordingState and buffer
pool directly, and extracts the real mixer and applicable tests verbatim. Only
unused device metadata is stubbed; this is not a second audio implementation.
The buffer runner includes the exact production callback handoff module.

```bash
python3 -I frontend/scripts/test-audio-idle-portable.py
bash frontend/scripts/test-core-audio-buffer.sh
```

Requires Python 3.11+ and Cargo/Rust; tests use repository-locked dependency
versions in temporary crates. Linux verification used isolated Rust/Cargo 1.97.1.
No desktop application, native Mac API or speech model runs in these tests.

Before applying the fix, the original production loop was extracted into the
shared handler with only dependency-name substitutions. An assertion verified
that its body matched the original; the same portable command failed twice:
16 passed / 4 failed. The sharpened assertion reported exactly:
`silent source reported: Some("Audio channel was closed unexpectedly")`.
Initial 60-second silence, 30-second resumption, partial-tail flushing and the
continuous-microphone/idle-system mixer case failed. After restoring the prior
helper the same 20 tests passed. Seven buffer tests also passed, including
lost-wakeup interleavings, terminal notification/drain and post-terminal refill.
The buffer helper also passed non-test compilation.

The macOS-only AudioStream stop wrapper and complete native groups must run in
Actions, not on the owner's Mac build environment:

```bash
cargo test --locked -p meetily --lib --target aarch64-apple-darwin audio::stream
cargo test --locked -p meetily --lib --target aarch64-apple-darwin core_audio
cargo test --locked -p meetily --lib --target aarch64-apple-darwin audio::pipeline
```

These commands are pending, not claimed passes. Frontend behavior was not changed.

## Build handoff: local workflow candidate, remote import blocked

The approved original local source is `91bd839947b60f339b8f835a1a22ba10fd17f37c`,
with tree `8a75052991eaebdaa82f6ccf69f64cbcdffec9ab`. The local workflow candidate
uses that source metadata, but **it is not yet a remotely available checkout**.
On 2026-11-17 the remote task branch and PR #21 remained at
`d9d186c1b2141b53b252c6690552c28d68ac45ba`; a source-only push failed because
Git could not read GitHub credentials. The GitHub App object-import tools were
not exposed in this implementation context. No source upload is claimed.

Before any workflow upload, use the supported App import path, recheck the
remote head without overwriting concurrent updates, verify its imported tree
against the approved tree, and replace SOURCE_SHA with the actual remote commit.
Keep ORIGINAL_LOCAL_SHA at the approved local commit even if import metadata
changes its SHA. Record workflow SHA and source SHA separately. Do not reuse the
previous DMG/hash as fixed evidence.

The local workflow installs locked frontend dependencies, builds frontend assets,
builds/copies the ARM64 Metal llama-helper sidecar, then runs the three native
filters above. The app build script supplies FFmpeg during Cargo's build phase.
Only after those tests pass does the existing sidecar/DMG packaging script run;
it may repeat cached frontend/helper builds. Tests use synthetic in-process
fixtures, not models or real capture. All native execution remains pending in
Actions. Failure artifacts now retain test/build logs as well as text diagnostics.

This workflow change must receive parent Sol review before it is pushed or a new
native run starts. No release, merge, installation or Mac data operation is
permitted. Issue #5 remains blocked; #6/#7 do not start.

Local preparation checks (2026-11-17): actionlint passed; all six multiline shell
blocks passed `bash -n`; `git diff --check` and approved source/tree assertion
passed. Using the isolated Rust 1.97.1 toolchain, the portable suite passed
20/20 and the buffer suite passed 7/7 plus non-test compilation. Initial attempts
without that toolchain on PATH failed because Cargo was unavailable; these were
rerun successfully, not counted as passes. Native macOS filters, packaging,
remote tree verification, and post-upload path-filter behavior remain untested.
PR #21 was open/unmerged with `auto_merge: null`; public rulesets returned `[]`,
while branch protection details required authentication. No merge-queue workflow
trigger or automatic merge command was found in the checked-in workflows.


## Physical regression protocol (owner-executed, not yet performed)

Use the existing issue #5 same-account backup/approval boundary in
[ISSUE5_MACOS_BUILD.md](ISSUE5_MACOS_BUILD.md). This protocol grants no additional
installation, permission-reset or data-operation authority. Do not build locally
on either Mac; use the newly pinned Actions artifact after review and approval.

1. Verify artifact provenance, new source SHA/tree and locally recomputed DMG
   hash. Record app version, Mac/model/RAM, macOS/build, mic/output devices,
   headphones/speakers, local ASR/diarizer names and weight hashes, auto-save and
   actual recording folder. Keep summary/Q&A/cloud providers and automation off.
   Enable both mic and system capture; use synthetic/self-authored phrases only.
2. **Initial idle:** with all system playback stopped, manually start recording.
   Speak a mic marker around seconds 2 and 15. Keep system playback silent for
   at least 60 seconds. Verify the same session remains active past 5/10/30/60
   seconds, with advancing time and no channel-closed/cancellation error. Stop
   normally; it must finish, not hang. Check saved mic/mixed audio and transcript;
   the intentionally silent system track must not contain fabricated speech.
3. **Resume:** start another take with headphones. Play a short, self-authored
   system marker, stop playback for 30 seconds while speaking distinct mic
   markers, then play a different system marker. Keep recording through 60
   seconds and stop. Check one continuous meeting, both system markers exactly
   once at the correct times, uninterrupted mic markers, and silence in the
   system gap. Listen to mic.mp4, system.mp4 and audio.mp4 separately. Do not
   infer identity from speaker numbers or hide acoustic leakage by relabeling.
4. **Stop while idle:** start with no system playback, wait at least 15 seconds,
   stop while the tap is idle, then start another take. Verify responsive stop,
   no stale recording or delayed fatal callback, and working capture on restart.
   If per-app capture is used, repeat steps 2-4 with the selected app open but
   silent, then playing; do not treat a genuinely exited app as guaranteed idle.
5. For every take record timestamps, meeting ID/title, errors, file durations,
   hashes, listening results and transcript text/time/source. Decode all three
   tracks with the bundled FFmpeg as in the existing runbook. Wait for final
   save, export Markdown, Cmd-Q and reopen the exact candidate, then verify the
   same audio and transcript are accessible. Retain audio and logs locally;
   share only sanitized evidence. Never upload meeting audio/text to a provider.
6. Record M4 results separately from M1 Pro / 32 GB; M1 remains unqualified until
   actually tested. No Linux test or previous PR candidate substitutes for these
   checks. If failure remains, capture sanitized native error lines with relative
   timestamps, capture backend/mode, callback/EOF/overflow/rate-change evidence,
   model setting and stop outcome; do not speculate from the toast alone.

A separate save/checkpoint deadlock fix exists in PR #1 commit
`d9cb04f4ff9bc2560be30853863e3c89dd713102`. It is not ported here: it also contains
unrelated Codex/UI work, and it is not the reproduced idle-stop error. The current
encoder still writes PCM before draining FFmpeg stderr. The 60-second save/restart
checks above are therefore important: if they expose that independent failure,
report it separately and port its isolated, testable fix before claiming the
issue #5 gate passed. This correction alone is not a claim of complete physical
recording/persistence qualification.
