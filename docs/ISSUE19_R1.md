# Issue #19 R1 — lock ordering, playback revalidation and reader lifetime

Review target: standalone `15f8ca70288367482e89f95fd75c8715a614ad85`.
These narrow fixes preserve that history and do not merge the #18 bridge yet.
All test files/audio/databases below are generated disposable fixtures.

## P1: native Stop must not wait for offline admission under lifecycle

Actual native Stop takes `acquire_engine_lifecycle_lock`, then calls the recording
manager → `RecordingSaver::stop_and_save`. R1's original saver called the externally
admitted `cleanup_checkpoints`, which waits for offline admission. Refinement has
the opposite order: offline admission → STT batch lifecycle. The deterministic
interleave reproduced the cycle using the **actual lock helper bodies and actual
saver/FFmpeg/finalization/cleanup path**, with adapted capture/Tauri transport:

1. Stop owns lifecycle.
2. A competing task obtains offline admission and signals readiness, then waits
   for lifecycle, as refinement does before STT.
3. Stop saves generated audio and reaches checkpoint cleanup.
4. Before the fix, the test's 3-second bound expires (44 passed, 1 failed).
   This is the same inversion, without spending the reported 300-second wait.

Native finalization now calls the internal, unregistered
`cleanup_finalized_checkpoints`; it does not acquire offline. It is only used after
this lifecycle-owned recording has committed its final tracks and metadata.
Independent IPC `cleanup_checkpoints` still acquires offline and transfers its
owned guard into the blocking worker. Both call the **same unchanged per-track
commit-proof validation/unlink implementation**. Recovery admission is unchanged.
No lock is simply removed from the external command, and no recursive/file-scope
expansion is introduced.

Current ordering:

| Caller | Admission through file operation |
| --- | --- |
| Native Stop/finalize | lifecycle → own finalization cleanup; never waits for offline |
| Manual/queued refinement | offline → lifecycle as before |
| Independent checkpoint cleanup/recovery | offline → blocking file worker |
| Audio-only library cleanup | try offline → try lifecycle → SQLite immediate reservation |

The Stop regression asserts successful final audio, completed metadata, retained
transcript backup and removal of the committed generated checkpoint, followed by
idempotent external cleanup calls. A separate deterministic poll proves the
external command still waits while an offline reader owns admission. One initial
green-run fixture assertion assumed the entire `.checkpoints` root disappears;
silent source tracks can leave empty directories, so the assertion was corrected
to the actual committed checkpoint and metadata invariants. No production cleanup
scope was broadened to satisfy that assertion.

## P2: observing audio must not recreate unchanged playback

`useMeetingAudio` now separates navigation disposal from file observations. Focus
and relevant native cleanup notifications read current audio without pausing,
clearing or rebuilding an unchanged player. The source comparison uses path plus
`playbackRevision` returned by the normal `api_get_meeting_audio` command. Its new
lightweight native helper observes length/modified time and, on Unix, device,
inode and ctime; it is not an audio content hash or a retention lease.

A changed identity, changed path or missing playback track disposes the old player
and updates availability. New players receive the currently selected rate before
loading, and a revision query distinguishes same-path replacements from cached
old media. Every async check is generation-fenced; callbacks and play-promise
errors check element ownership. A late response/callback from an earlier check or
another meeting cannot replace the current player. A failed observation is not
proof that a still-playing track disappeared.

`audio-cleanup-finished` now carries the deduplicated affected meeting IDs from
native outcomes. Meeting-specific hooks ignore unrelated successful cleanup.
Null denotes an execution error without exact outcomes and conservatively requests
revalidation; it still cannot reset an unchanged source. Library-wide observers
continue to observe all cleanup events. If only a non-playback source file changes,
unchanged mixed playback is retained by its own identity check.

Mounted regressions first reproduced an extra element on unchanged focus and an
actual rate of 1 while UI rate was 1.5. They now cover playing at **23 seconds /
1.5×** across unchanged focus, no recheck for an unrelated meeting's cleanup,
same-path replacement, deletion, selected rate on replacement/navigation, late
same-meeting observations and stale responses/callbacks after navigation. Native
fixtures invoke the actual playback API body against generated files to verify
stable identity, same-path replacement identity and absent-track output; the
normal cleanup command's emitted affected-ID payload is also checked.

## Diarization abandonment: demonstrated gap vs unqualified Tauri behavior

The original public `diarize_meeting` held offline admission in its awaiting Rust
future while its inner path awaited `spawn_blocking`. Aborting that Rust waiter
released admission while the blocking reader remained alive. A deterministic test
reproduced this using the **actual public command body** with SQLite, production
admission helpers and a barrier-controlled inner blocking-reader adapter. Model
inference, identity-policy/checkpoint work and projection publication are adapted
boundaries in that fixture; it is not a model/diarization quality test.

This demonstrates a Rust lifetime gap. It **does not demonstrate that a Tauri
WebView close/reload actually cancels that command future**. No physical or native
Tauri cancellation qualification was run.

The public command now clones its database owner into a native Tokio task. That
task owns admission, awaits the existing shared reader and publication path, and
is not cancelled when its IPC/Rust waiter is abandoned. The abandonment regression
now proves admission remains unavailable until the controlled blocking reader
finishes. Selected-engine behavior, native identity policy and candidate/shared
inner path remain unchanged. Explicit application shutdown/runtime destruction,
real inference completion timing and physical Tauri behavior remain separate
qualification limits. This change does not forcibly cancel a model thread.

## Executed checks — 2026-11-19

- Complete shutdown harness: **45 passed, 0 failed, 0 ignored**, including generated
  FFmpeg audio, normal saver/finalization, exact commit proofs and the forced Stop
  interleave. Red was the expected bounded lock-inversion failure.
- Complete recovery harness (explicit empty test filter): **83 passed, 0 failed,
  0 ignored, 0 filtered**. The initial default-filter invocation also passed
  **16/16**, with 67 filtered; the full rerun removes that qualification gap.
  Covers actual recovery commands, generated FFmpeg checkpoints, native save,
  temporary SQLite failure/retry/idempotency, Notes and versions/corrections.
- Narrow cached-rustc cleanup/API/ownership suite: **14 passed, 0 failed, 0 ignored**.
  Native playback contract was red before the new response field; diarization
  abandonment was red before native task ownership.
- Mounted playback group: **3 passed**, 29 assertions; initial red reproduced the
  focus/rate failures. Full isolated frontend runner: **54/54 files passed**.
- `git diff --check`: passed. No whole-project TypeScript or desktop/Next build.

Both requested Cargo/FFmpeg harnesses ran offline, one Cargo job, using the existing
portable cache. The first invocation recompiled some cached dependency feature
variants and the small fixture crate; there were no dependency downloads, model
or full application builds. Logs remain temporary diagnostic artifacts, not
committed recordings/build payloads. No real/private data, Mac, agents, packaging,
Actions, push or external deletion was used.

## Integration gate

The coordinator supplied accepted canonical bridge #5–18 head
`2d388cf15adb95f2d37ab1e9ff17b82df723e328`. This R1 fix is a separately attributable
standalone commit returned for recheck **before** merging that dependency. Once
P1/P2 are accepted, integrate that exact head in the existing #19 branch preserving
both histories, then qualify the combined Stop/offline/queue/cleanup, retained
canonical search and both exports, text-only summary retry and playback paths.
No combined-bridge acceptance or heavy-build result is claimed here.

Real HTMLMediaElement decoding, Tauri asset-protocol query/cache behavior and
physical focus/playback on Mac remain unqualified: mounted media elements and IPC
are fixture adapters. These tests establish lifecycle/state behavior, not a real
installed-player or transport-cancellation result.
