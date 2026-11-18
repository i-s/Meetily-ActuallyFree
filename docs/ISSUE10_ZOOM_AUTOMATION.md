# Issue #10 — Zoom automatic recording software

Live issue read **2026-11-18**: **[[macOS 06/16] Автостарт и автостоп Zoom](https://github.com/i-s/Meetily-ActuallyFree/issues/10)**, open, **zero comments**. Accepted software base: `4269de67c6747b2f5fce72596826c05dadbb3e1c` (#9). This implements a candidate adapter and actual application automation, not a qualified detector or an issue-closure attestation. No Mac operation, installer/DMG, packaging, Actions dispatch, push, release or private/user-data operation belongs to this work.

## Prior work and the unresolved physical finding

Reviewed the planning workspace's AGENTS/domain/issue instructions, glossary, requirements, macOS specification, ADR 0001 and call-signals research. Reviewed the local prior fork `origin/fix/codex-call-detection` at `2169cabcac80a22f5a8fea6c54d13e8b971bb261`, its bounded AX probe, per-source tracker and `docs/CALL_DETECTION.md`, and this checkout's [issue #7 note](ISSUE7_CALL_SIGNALS.md). The fork credits YoungTeurus/meetily's MIT implementation at `4732a3ff2331e3b33e007313c06b7748625e9304`; that attribution remains in the adapter.

The prior physical Zoom inspection was **UNKNOWN despite granted Accessibility access, English UI and a visible toolbar**. Nothing here overturns that observation. Neither AX permission, synthetic controls, process launch nor HAL I/O proves a qualified participant lifecycle. The stricter candidate can still return Unknown on the target client; if it cannot distinguish participation on the mandatory physical matrix, **required automation is not achieved**. A manual notification is not a substitute.

## Native evidence and lifecycle ownership

- `meeting_detection/zoom.rs`: bounded read-only AX inspection, on a blocking worker. No focus changes, control actions, AXValue/chat text, permission prompts, external SDKs, audio inference or network calls. Shared 700 ms scan budget, 50 ms timeout per AX target, 16 windows/128 children/768 nodes/depth 14 bounds; OS scheduling is not a hard real-time guarantee. Diagnostics retain fixed reasons/counts, not labels/window titles.
- `meeting_detection/lifecycle.rs::scan_zoom`: read-only public AppKit `NSRunningApplication.runningApplicationsWithBundleIdentifier` inventory for the exact `us.zoom.xos` bundle, with a local autorelease pool and bounded PID list; helpers/keyword matches are not identities. Failed/nil/invalid inventory is Unknown. Other bundle IDs/layouts are unsupported until observed. A successful empty bundle inventory supplies `zoom_not_running`; this is process-exit evidence, **not** a process-launch start rule.
- Positive candidate: enabled participant control **and** enabled leave/end control in the **same window**, with exact supported English/Russian command labels. Menu-only Leave was deliberately rejected because it does not distinguish a waiting room. No window title substring, mic/camera activity, silence, mute, preview, test or default-device change is an input to lifecycle inference. A waiting room exposing the same pair would invalidate this candidate; physical testing must check that explicitly.
- Positive idle candidate: complete single Home window with enabled Join + New Meeting and an explicitly disabled Leave Meeting menu item. Missing/hidden toolbar, no AX access, unsupported language/layout, timeout/limit/read failure or an omitted adapter is Unknown, not exit. Already-read positive controls can survive an incomplete unrelated tree; negative evidence requires completeness.
- `meeting_detection/evidence.rs::Tracker`: per-source identity; two positive observations create one logical session. Continuous explicit inactive observations for 45,000 ms end it. Unknown/omitted source resets confirmation and exit grace without ending a live session. Gaps >20,000 ms discard debounce/grace time. Helper/device/mute changes do not allocate a new session. Settings/monitor restarts preserve native sessions but discard debounce/grace evidence.
- Native `meeting-lifecycle` snapshots and `get_meeting_lifecycle_snapshot` expose `{revision, enabled, sources:[{source,app,process,presence,reason,session_id}]}`. `session_id` is opaque and retained through Unknown and inactive grace, null after a confirmed end; the current observation and logical call ownership are different facts. Revisions reject stale event/query replies. Monitor-generation checks under the native state lock reject obsolete blocking scans and stale start authorization after monitor restart/disable. The process epoch prevents a prior native run's session token authorizing a new start.

The macOS monitor polls the candidate every **3 seconds**, separately from the configurable legacy-offer interval. Startup has the existing 2-second delay. Nominal software timing is two polls for join and at least 45 s continuously observed inactive for end, plus scan/scheduling/startup/model/device time. These are **provisional software knobs, not research-established latency bounds**. #7 measured production bounds are still TBD. No latency guarantee is claimed.

## Actual application path and manual override

```text
native monitor / blocking Zoom AX scan
  -> shared Tracker + native lifecycle snapshot
  -> root layout meeting-lifecycle listener (main window only)
  -> meeting-lifecycle.ts serialized reconciliation
  -> markAutomatedStart + launchRecording / normal recorder-route handoff
  -> useRecordingStart + existing local model/device setup
  -> RecordingService.startRecordingWithDevices(..., lifecycleSession)
  -> registered Tauri start_recording_with_devices_and_meeting
  -> native start_recording_with_lifecycle
  -> existing RecordingManager/capture/pipeline

confirmed native exit of that SAME source/session
  -> root reconciliation
  -> requestRecordingStop
  -> existing recorder Stop/save/recovery/postprocessing sequence
```

The native start validates the opaque token before initialization and again after model/device/diarizer setup immediately before opening capture. It requires a currently positive source and enabled detection, not merely a retained session through Unknown. A leave during frontend model loading cancels start. A leave racing native stream startup requests the **normal Stop/save path**, not a parallel backend teardown. Startup failures use existing visible recording errors and release the handoff reservation. Native Zoom startup rejects saved In-person mode or a per-app selection other than exactly `zoom.us`; it never changes preferences or silently captures Discord/global audio instead. Explicit existing all-system selection remains allowed.

The root listener subscribes before fetching the native snapshot; route changes and monitor restarts do not manufacture a new session. Pending and in-flight starts reserve the single recorder. Session suppression is persisted in sessionStorage as one tombstone per source. Manual Stop, failed start, a busy recorder and “Don't stop it” do not immediately retry the same session; only a subsequent confirmed call gets another opportunity. Native-call identity survives PID/device/mute changes. Manual recordings have no lifecycle owner and cannot be stopped by these events. An ownership check after asynchronous recording-state lookup prevents an old stop request from acting on a replacement manual take.

The main WebView/root listener and sessionStorage are required for automatic routing/Stop; this is not an OS service. If the WebView is unavailable, native status is retained and reconciled on its return; it is not a guaranteed background auto-stop while the WebView is dead. Closed/unavailable UI, app restart during a manually suppressed call, permission loss and failed storage need final qualification. Failed storage prevents routed automatic start; the manual recorder remains available. When automation/detection is disabled, no synthetic end is inferred and the current take remains under manual control. Re-enabling does not retry an already consumed session.

## Zoom on speakers: identity is NOT lifecycle

#6 acoustic acceptance is still **not run**. There is no new AEC, voice separation, voiceprint assumption or remote-active microphone gate. Both source tracks and their audio/timestamps/transcript turns remain; the microphone is never muted/dropped merely because remote speech is present, so this change does not erase local double-talk.

Until there is a qualified route-specific identity policy, **all new macOS microphone speech is conservatively `Microphone`, not `You`**, including manually started calls and headsets. This deliberately sacrifices confident local identity rather than let a Zoom-through-speakers remote-only utterance become `You`. Windows/Linux retain their existing convention; in-person mode remains source-only. Live microphone audio is not enrolled as a known user embedding under the unverified policy.

`MeetingMetadata.microphone_identity_unverified` persists the policy. Offline diarization disables the mic-to-You dual-track shortcut for these takes and uses the existing whole-recording path; it must not reinterpret mic provenance as identity. Enhance/retranscription also reads that flag and labels the retained mic track `Microphone` rather than `You`; invalid flag types fail provenance validation. Old metadata defaults to the historical policy and is not rewritten. Live text/timestamps and source audio are unchanged; acoustic separation, accurate local `You`, duplicate remote pickup handling and real double-talk ASR accuracy remain #6's physical gates. This is a safe attribution floor, **not** proof of separation or a full identity AC pass.

## Composable interfaces and integration conflicts

- New source adapters should supply a **single batch** of `Observation` values to `lifecycle::observe_sources(generation, observations)` per monitor poll, with a stable `key` (e.g. `zoom`/`discord`), fixed diagnostic `Probe`, and Unknown for unavailable evidence. Do not call the tracker separately for each adapter; missing observations reset debounce. Omitted snapshot sources are published Unknown with logical session retained. The session/token/snapshot API is source-neutral; the Zoom AX mechanism is not.
- The frontend reconciler already accepts multiple sources. One recorder means **first accepted session wins**. Calls seen while pending, recording or stopping are consumed with a visible conflict message, never queued, merged, source-switched or used to stop someone else's take. No automatic Zoom-to-Discord handover is promised. Choosing a different overlap policy is an explicit coordinator/product decision.
- #11 likely conflicts in `meeting_detection.rs`, `lib.rs`, root `layout.tsx`, `meeting-automation.ts`, Settings and native command registration. Integrate its source-specific adapter into the batch/snapshot path; **do not replace this with CAM/HAL/process disappearance, a second monitor-owned recorder or a manual prompt**. Legacy process offers remain separate; macOS Zoom is excluded from that fallback. Legacy meeting-ended cannot stop session-owned recordings. This commit does not claim a macOS Discord detector.
- #14 likely conflicts in `diarization/mod.rs`, worker/start interfaces and metadata policy. Preserve `microphone_identity_unverified` in every post-call source identity decision; never reintroduce source-only `You`. Keep the #9 source selection, shutdown and durable recovery semantics.
- #12 can attach Notes to the ordinary meeting created by the recorder; lifecycle status is not a meeting/person identity. This work introduces no Notes store, product-text provider or paid API. Audio/ASR stay local; only the separately approved subscriber Codex CLI may handle product text.

## Tests and acceptance accounting

Portable commands (toolchain/dependencies must be available; Bun groups run separately):

```sh
python3 -I frontend/scripts/test-meeting-lifecycle-portable.py
python3 -I frontend/scripts/test-retranscription-sources-portable.py
# from frontend, each in its own process:
bun test tests/lib/meeting-automation.test.ts
bun test tests/lib/meeting-lifecycle.test.ts
bun test tests/hooks/zoom-automation.test.tsx
bun node_modules/typescript/bin/tsc --noEmit --project tsconfig.json
```

The portable native harness includes the production AX classifier/tracker, serde lifecycle snapshots, command bodies and recording-mode policy. Only Tauri command annotations are removed in the temporary lifecycle copy. macOS FFI is cfg-excluded: this does **not** compile the macOS SDK, desktop wrappers or IPC. Synthetic frontend hook tests use the real recorder hook, root reconciler, event/route launch helpers and RecordingService; the Tauri transport/audio/model responses are mocked. They establish token propagation, manual ownership, startup cancellation, start/leave race normal Stop dispatch and error cleanup, not audio accuracy or an installed-app result. No recording/model fixtures or hardware measurements are used.

| Live issue AC | Software evidence | Remaining acceptance |
| --- | --- | --- |
| Verified join/leave detector with research-established latency bounds | Candidate AX adapter + native per-source debounce + actual start/Stop wiring | **Not accepted**: prior physical UNKNOWN unresolved; full Mac detector and latency qualification pending #7/final build |
| Running Zoom, mic test, waiting/preview do not create a meeting; mute/silence do not end | Synthetic classifier negatives; menu-only Leave rejected; audio/mute absent from policy; Unknown retains live session | **Physical pending**; reject candidate if waiting/preview shares positive controls |
| Muted join, reconnect, device changes, leave with Zoom open | Same session through Unknown/grace; muted-control fixtures; explicit Home idle can confirm leave with client alive | **Physical pending** for actual exposed controls, reconnect semantics and route changes |
| Manual Stop does not restart same call; next real call detects | Session tombstones/native IDs; deterministic root/recorder and multi-source tests | **Software tested**, installed-app/real-call check pending |
| Automatic signal sequences and physical Zoom on Mac; state/errors visible | Native/frontend deterministic sequences; real recorder entrypoint tests; Settings presence/reason and normal capture errors | **Software tested**, physical half pending; no whole-AC completion claim |

### Verification performed on Linux

- Native portable lifecycle/AX-classifier/capture-policy harness: **25 passed**, 0 failed. Production snapshot command body and stale-token validation exercised, including two independent synthetic sources and generation rejection. macOS AppKit/AX code is cfg-excluded, **not compiled** here.
- Isolated Bun groups: meeting handoff **5 passed**; lifecycle sequences/races **9 passed**; actual recorder hook + service entrypoints **3 passed**; existing Recording Settings mode **1 passed**; existing recording state/shutdown **2 passed**. The synthetic capture-error case intentionally logs `Synthetic capture unavailable` while asserting cleanup.
- One early recorder test invocation hit its configured 120-second background timeout under host contention and was stopped; rerun with adequate timeout completed successfully. No hardware failure is inferred from that timeout.
- Enhance/retranscription source-policy harness: **5 passed**, 0 failed, including retained mic/system source labels and invalid-policy rejection.
- Standalone TypeScript checking passed once during implementation; the final check is recorded with the completion report. No Next production build, full native desktop build, native macOS compile, model/ASR fixture, installed application or physical audio test was run. This Linux environment lacks `pkg-config`; it is not an authoritative macOS build environment.

Before acceptance on **both M4 Pro/48 GB and M1 Pro/32 GB**, use the final combined CI-built candidate only: compile/link macOS AX FFI; record exact source/artifact/client/macOS/language/permission/route inventory; run #7's repeated idle/test/preview/waiting/muted-join/silence/reconnect/device/leave-open/rejoin matrix and manual-stop/rearm/overlap tests; measure actual start/stop delays; exercise denied/revoked AX and unsupported controls; run #6 remote-only and local-plus-remote speakers tests with retained source audio and transcript provenance. A missing route/version or Unknown result is not a pass. No Mac qualification is delegated to the user during this software preparation.
