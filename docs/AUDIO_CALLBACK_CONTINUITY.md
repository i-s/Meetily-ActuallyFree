# Recording callback continuity — issue #40

## Report and reproduction

[@fernandog's report](https://github.com/TylerBuza/Meetily-ActuallyFree/issues/40)
identified speech-dependent broadband artifacts at 480-sample wired-microphone
and 384-sample Bluetooth capture boundaries. The report concerns the saved
microphone/mixed files, not merely the preview player.

`useMeetingAudio.ts` streams the saved file through an HTML audio element. The
native mixer previously positioned **every** block from a callback-delivery
timestamp. That clock jitters relative to the device sample clock. Sub-millisecond
timing differences therefore caused zero insertion or sample deletion inside a
continuous waveform, before both source-track persistence and live VAD.

A regression supplies continuous synthetic sine samples in the reported block
sizes with opposing ±0.3 ms microphone/system callback jitter. It failed on the
old implementation at the 480-sample microphone case. With the correction, both
tracks preserve every input sample at both block sizes.

## Timing ownership

`audio/pipeline.rs::AudioMixerRingBuffer` owns one `SourceSampleClock` per source.
Each clock tracks a next-sample index relative to the shared mixer origin and
the last callback-end timestamp in recording-relative seconds.

- First input anchors the source to recording time, preserving late-source
  startup alignment. Subsequent continuous blocks advance by their sample count.
- Only a callback-free interval greater than 100 ms reanchors a returning source
  to wall time. This matches the default two-window missing-source allowance.
  Normal jitter and gradual delivery-clock drift do not splice the waveform.
- Already-emitted silence cannot be replaced: a genuinely late block's elapsed
  prefix is still trimmed, but its sample clock advances by the original length.
- Existing bounded buffering, source zero-padding, and large-gap reset behavior
  remain. A large shared timeline reset now also clears both source clocks.
- Empty/mixed inputs do not advance source clocks. Muted source callbacks still
  supply aligned zeros. Device provenance, ASR/diarizer selection, capture-worker
  ownership, gain, and saved-file formats are unchanged.

This adds constant-size clock state; no new worker, queue, inference, or blocking
capture operation is introduced.

## Qualification and remaining limits

Fourteen mixer tests passed, including bit-for-bit jittered-block preservation,
one minute of simulated gradual clock drift, a real 200 ms source gap followed
by jittered resumption, late-source startup, equal-length tails, bounded large
gaps, mute behavior, and gain handling. The complete native CPU suite subsequently
passed **350 tests**, with **nine opt-in tests ignored**. These new continuity
tests use synthetic samples, not real microphones or speech recognition.

The UI progress change was independently exercised in browser preview: enhancing
had no dialog/backdrop, body pointer events were enabled, an actual mouse click
opened About behind the progress card, and the transcription-start call count
stayed at one. The frontend suite passed **126 tests in 22 isolated invocations**;
production build and type checking passed.

The 100 ms gap decision is a delivery-time heuristic, not hardware timestamp
recovery. Severe scheduling stalls can still look like missing capture. There is
no adaptive resampling to compensate long-run oscillator drift between devices;
this correction prioritizes preserving continuous samples over repeatedly
cutting them to fit wall time. Hardware testing on the reporter's wired/Bluetooth
devices and sustained two-source load remains separate qualification.

Existing damaged recordings are not rewritten: deleted samples cannot be
reconstructed by changing playback. The correction applies to newly captured
audio. Private recordings, diagnostic output, and fixtures stay outside Git.

## Installed release-candidate check

The final CPU/Vulkan/CUDA v0.2.18 Windows payload was built and passed
`verify-windows-release.mjs` (updater signatures, hashes, runtime files, and
bootstrapper payload). After backing up the existing install and native/WebView
data, the installed CUDA executable matched the packaged SHA-256. Startup IPC
confirmed v0.2.18, completed onboarding, selected/available Nemotron, and a ready
workspace. The app exited cleanly through native IPC and reopened without debug
flags after the check.

SQLite integrity passed. Eight meetings, 157 transcript rows, four people, seven
person-speaker links, model hashes, and preference values were preserved. This
is local installation/startup qualification, not a new microphone capture test.
The reporter's devices remain untested here. v0.2.18 is prepared as a draft;
publication is separate from this verified local installation.

## v0.2.18 macOS idle-callback candidate

The candidate branch starts at `10f11baed5e0dbc9c1e885d22534a6dec19dbb57`.
A Core Audio process tap may legitimately deliver no callbacks while the system
is idle. The recording task must await the next sample without a five-second
fatal timeout. `None` while recording and a changed sample rate remain terminal
errors; cancellation drops the pending source without requiring another callback.
`ChannelClosed` is still nonrecoverable.

`audio/stream/core_audio_processing.rs` is the handler used by native capture and
synthetic source tests. Its partial block must be submitted before returning
Pending, rather than being carried across a callback-free interval and timestamped
at resumption. It still batches up to 1024 samples. This changes no callback sample
values and adds no synthetic source data. The mixer's existing source clocks and
missing-source zero padding continue to own alignment and silence.

`audio/capture/core_audio_buffer.rs` owns the bounded ring-buffer handoff used by
`CoreAudioStream::poll_next` and the capture callback. The fast pop is followed by
waker registration and a second data/terminal check before Pending. Both delivery
and terminal transitions wake the consumer. More than ten consecutive incomplete
writes still terminate capture; success resets the counter. Once terminal, later
callbacks cannot refill the queue or postpone EOF while it drains. AtomicWaker avoids a
blocking mutex in the callback.

`trigger_system_audio_permission_command` now returns
`{ detected, conclusive, reason }`. Audible capture is verified evidence; five
seconds of silence is inconclusive, not denial. Only an explicit permission error
is conclusive denial: the native `perm` OSStatus is retained as typed
`PermissionDenied`; arbitrary error text is not treated as denial. The bounded diagnostic probe remains separate from the
unbounded recording wait. Frontend consumers use `verified | denied | unknown` and
a versioned session cache. Legacy booleans are ignored, and inconclusive results or
exceptions preserve earlier evidence. Unknown UI copy invites playback and retry.
The onboarding meter retains its unmount/run-generation guards.

The fork-only `build-macos-idle-fix.yml` workflow builds the exact PR head on
macos-15 arm64 with native production-handler tests. It validates an ad-hoc signed
DMG and its embedded app, sidecars, resources, permission strings, identifier and
minimum OS; artifacts include an app archive, checksums, patch and build metadata.
It does not publish a Release or updater. A workflow file is not evidence of a
successful build; the PR/run result must establish that separately.

### Physical qualification

CI tests inject samples and ring-buffer transitions, without a real audio device.
Even successful macOS compilation and package smoke tests do not verify Core Audio
capture on a physical Mac. Test the candidate on Apple Silicon/macOS 14.8.5:

1. Record microphone for 60 seconds with no system playback.
2. Record 10 seconds of playback, 30 seconds without it, then 10 seconds of playback.
3. Stop while system callbacks are absent.
4. Inspect `mic.mp4`, `system.mp4` and `audio.mp4`; confirm no repeated samples,
   preserved silent intervals and microphone/system synchronization within 100 ms.

Keep application data and the `com.meetily.ai` identifier. This candidate retains
version 0.2.18; installing it is separate from passing hardware qualification.

### Cloud regression evidence

The portable production-handler tests reproduced the original timeout: 16 passed
and four failed. Removing only the timeout left two failures (18 passed): the
partial chunk was missing before the gap and system markers shifted. Flushing
the tail before Pending made all 20 pass: seven stream, ten mixer (including
`a_real_source_gap_retains_silence_before_resuming_contiguously`) and three buffer
pool tests. `test-audio-idle-portable.py` includes the real processing/state files
and extracts the mixer/tests verbatim; it does not replace their audio logic.

The ring-buffer production helper reproduced four failures in six deterministic
cases before correction; all six now pass, as does compilation without its test
hooks. `test-core-audio-buffer.sh` runs that exact helper using lockfile versions.
Four device-independent permission-classification tests passed via direct source
inclusion. These harnesses do not compile the macOS wrappers or desktop/DSP paths.
The native CI filters cover those paths and all 15 pipeline tests.

Frontend permission tests passed in separate Bun processes: three cache tests,
six component/lifecycle tests and one hook test. The existing audio-level lifecycle
test also passed. The hook regression first failed against the original boolean
contract. Cargo.lock is unchanged; Tokio test-util is enabled only for development.

Cloud production `pnpm run build` and subsequent standalone `pnpm exec tsc --noEmit`
passed. The build retained existing Tailwind ambiguous-utility warnings. The full
Linux desktop Cargo attempt was stopped during dependency downloads after repeated
partial-transfer errors; no Linux desktop or macOS native pass is claimed by these
portable results. macOS verification belongs to the candidate's Actions run.

The first PR frontend job exposed the v0.2.18 workflow's documented mock leak:
`summary-language-preferences` installed a read-only global window before
`meeting-automation` assigned its own window (102 passed, one failure). The
existing `pr-checks.yml` now runs each existing lib/hooks test file in a separate
Bun process. The same isolated command passed in Cloud; no application behavior
outside the audio fix was changed for that CI correction.

A follow-up terminal-state regression failed after eleven overflows because a
later callback could refill the queue and postpone EOF indefinitely. The producer
now rejects post-terminal writes. All seven handoff tests pass, including attempts
to refill both before and after EOF; the non-test helper also compiles.
