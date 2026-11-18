# Issue #10 — Zoom lifecycle: observation, qualification and guarded recorder

Live issue read 2026-11-18: **[[macOS 06/16] Автостарт и автостоп Zoom](https://github.com/i-s/Meetily-ActuallyFree/issues/10)**, open, zero comments. Accepted software base `4269de67c6747b2f5fce72596826c05dadbb3e1c`; independent Opus R1 follow-up base `7d3ff8aeac1caddb73a8e72b8dcd0b0b66065110`.

**Real Zoom classes are observation-only. None is qualified to authorize capture.** The automatic recording implementation is exercised with explicitly synthetic qualified fixtures, not enabled by old saved Labs/detection preferences. There is no production qualification override. Manual recording remains available as manual recording, **not** as a substitute for issue #10's required verified automation. Issue acceptance, leave-open reliability, latency bounds and #6 acoustic separation remain unresolved.

## Prior evidence and the qualification gate (R1-6)

Reviewed planning AGENTS/domain/issue guidance, glossary, requirements, macOS spec, ADR 0001 and call-signals research; checkout AGENTS/map; prior MIT fork `origin/fix/codex-call-detection` at `2169cabcac80a22f5a8fea6c54d13e8b971bb261` and its `CALL_DETECTION.md`. The bounded AX code retains its attribution to YoungTeurus/meetily at `4732a3ff2331e3b33e007313c06b7748625e9304`.

The prior physical result was **UNKNOWN despite granted AX, English UI and visible toolbar**. This work does not overturn it. AX permission, executable launch, mic/camera/HAL activity, mute, silence, preview/device test or a human truth marker is not a qualified call lifecycle signal. In particular, a waiting room exposing the same controls would invalidate the candidate; software debouncing cannot resolve identical signatures.

- `meeting_detection/zoom.rs`: read-only public AppKit exact bundle `us.zoom.xos` inventory, then bounded AX control traversal on a blocking worker. No focus/control actions, AXValue/chat text, permission prompts, vendor entitlement, audio or network access. Shared 700 ms scan budget, 50 ms per target IPC timeout, 16 windows/128 children/768 nodes/depth 14 bounds. OS scheduling is not real-time.
- Positive **candidate observation**: enabled participant + leave/end controls in the same window, exact supported EN/RU command vocabulary. Menu-only Leave is insufficient. Negative candidate: complete single Home with enabled Join + New Meeting + explicitly disabled Leave menu; successful empty AppKit bundle inventory supplies process absence. Failed/nil inventory, permission loss, hidden/unsupported controls, timeouts/limits and omitted adapters are Unknown.
- `Tracker`: stable source keys; two positive samples allocate an observed session; 45,000 ms continuous explicit inactive confirms its end. Unknown/omission resets debounce/grace, retains the session; >20,000 ms sampling gaps discard unobserved debounce time. Poll 3 s, initial delay 2 s. These are **provisional software knobs**, not measured join/leave delay bounds (#7 remains pending).
- `meeting-lifecycle` / `get_meeting_lifecycle_snapshot`: `{revision,enabled,sources:[{source,app,process,presence,reason,session_id,qualified}]}`. Every real source currently has `qualified:false`, including after loading previously enabled settings. Revisions and monitor-generation checks reject obsolete scans/replies. `session_id` denotes a candidate observation session, not qualification, person identity or an actual recording.
- `validate_start` requires current monitor generation, native automation enablement, positive current source and **qualification** before setup and again immediately before capture. Production cannot satisfy the last requirement today. Only test-local synthetic fixtures set qualification; no command, environment switch or persisted setting enables it.

## Native recorder authority and Stop routing (R1-1, R1-4)

```text
qualified fixture / future evidence-qualified adapter
  -> shared token reservation -> normal launchRecording route/event
  -> useRecordingStart -> RecordingService.startRecordingWithDevices
  -> Tauri start_recording_with_devices_and_meeting(lifecycleSession)
  -> guarded native start under engine lifecycle lock
  -> freeze source/session owner across awaited setup/capture
  -> actual RecordingManager/pipeline
  -> native RecordingLease returned from THAT start

same owner session confirms end
  -> root queries native recording state (also on reload)
  -> requestRecordingStop(navigate, expectedLease)
  -> route JSON / event detail carries SAME lease
  -> RecordingControls -> stop_recording(args.expected)
  -> engine lifecycle lock + atomic authority compare/enablement claim
  -> only accepted request enters normal Stop/save/recovery/postprocessing
```

`audio/recording_ownership.rs` owns a process-lifetime opaque generation and optional `LifecycleOwner {source,session_id,app,process}`. A take has `RecordingLease {generation,owner}`. Both native start paths allocate it under the existing engine lifecycle lock. The owner is frozen **before** awaited stream startup, so a call ending while the WebView goes away cannot erase ownership. The start return value carries the original lease even if a later awaited notification overlaps another take. `get_recording_state.recording_lease` restores ownership on reload independently of a JS completion promise. The main WebView still routes automatic Stop: this is not a new OS service, and an absent/dead WebView may delay Stop until return; it no longer loses the native owner.

Every automated Stop must carry an expected lease; missing/invalid handoffs fail closed. The legacy automated path also uses the lease captured by its start, never the newest manual take's lease. Before any tray/window/event/audio/save side effect, native `claim_stop` compares the full generation **and owner** and native enablement while holding the short authority mutex under the engine lifecycle lock. Disable and “Don't stop it” release use the same authority mutex. A rejection returns `false` to the UI. Automated Controls do not invoke manual stop callbacks, set stopping/processing flags, save, navigate or announce success when rejected. Native completion events drive accepted postprocessing. Manual button/tray/minibar Stop has no expected guard and remains functional.

A delayed Stop A after manual Stop A + manual Start B therefore cannot stop B, even after navigation/await/reload. UI enablement is checked after awaited recording-state and directory queries; native enablement is checked again at the actual Stop acceptance boundary. Disabling detection also disables native automation authority. An accepted Stop linearizes before a later disable; disable does not undo shutdown already accepted. Unknown or disable is never fabricated into a lifecycle exit.

Native Zoom startup also rejects In-person mode or a per-app selection other than exactly `zoom.us`; it does not rewrite preferences or silently widen capture. Existing explicit global capture is allowed only for manual recording or a genuinely qualified future start.

## One common reservation; #11 integration contract

`meeting-automation.ts` is the one common implementation:

- `markAutomatedStart(expectedCall): boolean` synchronously rejects existing pending/in-flight reservations and assigns a unique opaque `token` on the caller's object.
- Key is `source` + `sessionId` + token. `takeAutomatedStart()` transfers that **same** token pending → in-flight. Invalid old tokenless automation cannot degrade into an unguarded manual start.
- `reservationCurrent(expectedCall,'pending'|'inflight')`, `releaseAutomatedStart(expectedCall)` and `beginAutomatedRecording(expectedCall): boolean` are scoped. Late release/begin from A cannot cancel/adopt B, including another source. Browser active-call state is only a display mirror; `reconcileAutomatedRecording` is for a just-queried native lease, not admission.
- Busy/consumed source-session tombstones are written before asynchronous admission. Simultaneous Zoom/Discord claims have no private queue or priority: one shared reservation wins; busy sessions are consumed, not queued for a surprise later handover. No automatic Zoom↔Discord capture switch or room speaker differentiation is introduced.

#10 owns native generation/owner authority and guarded Stop. #11 retains its `discord-call-state` / `get_discord_automation_status` schema (`enabled,state,reason,session_id,transition,qualified`, plus `platform_supported`, nullable `client_present`, allowlisted evidence and journal metadata), Discord-only qualification commands and adapter/journal namespace. Its enabled flag means read-only observation, real classes stay `qualified:false`, Unknown is neither active_media nor End. No Discord private REQUEST_KEY, no subscription of Zoom to Discord capture, no raw AX labels/audio in journals, and no pending-branch cherry-picks or architectural replacement. At eventual integration, #11's qualified synthetic/future start must install its owner through the same engine lock/authority and return the actual native lease; real unqualified Discord must remain unable to capture. Do not merge namespace semantics by guessing identity from client lifecycle.

Likely integration conflicts: `lib.rs` command signatures/registration, root `layout.tsx`, `RecordingControls`, `useRecordingStart`, shared reservation/launch helpers, Labs enablement and capture metadata. #14 must preserve the identity policy and shared source predicate in both direct and meeting post-call paths; #12 attaches Notes to the ordinary recording meeting, not a lifecycle/person ID.

## Scoped native microphone provenance (R1-5)

The previous blanket all-macOS label change is removed. `RecordingManager::prepare_source_provenance` freezes a native recording policy before model/capture startup:

| Context at startup | Mic identity policy |
| --- | --- |
| In-person/room | Neutral `Microphone`; no room-person differentiation |
| Identifiable Zoom automatic owner or explicitly selected `zoom.us` on macOS | Unverified, neutral `Microphone` |
| Manual global capture on macOS with Zoom present (exact public bundle inventory) | Unverified, neutral `Microphone`, even if Zoom is only idle/preview |
| Global macOS capture with unavailable inventory | Ambiguous; neutral, never proof of `You` |
| Explicit unrelated selected app, or global capture with verified Zoom absence | Existing unrelated behavior preserved |
| Windows/Linux calls | Existing convention preserved; room remains neutral |

This scopes **provenance**, not lifecycle, and does not infer a call from process presence. It covers manually recorded identifiable Zoom speaker scenarios without a special frontend flag. There is no qualified route-specific AEC/headset test, so an identified Zoom context is neutral on headsets too. Global capture with idle Zoom is intentionally conservative; an unrelated explicit app is not relabeled simply because idle Zoom is open. The frozen decision does **not** identify a Zoom app launched after recording startup, another bundle/browser meeting, private routing or an unobserved route. Those gaps need a fresh identifiable recording/context and final qualification; there is no claim that legacy unrelated mic identity is acoustically proven. No automatic successful `You` separation is claimed.

Both source tracks, text and timestamps remain. Remote activity never gates/drops microphone audio, so local overlap is not deliberately erased. #6 remote-only/double-talk speech/ASR/acoustics are still **not run**. This is conservative attribution, not AEC, voice separation or accurate local identity.

## Offline identity and source labels (R1-2, R1-3)

`MeetingMetadata.microphone_identity_unverified` persists the native decision. `diarization/identity_policy.rs` reads it (and room provenance) before the actual call:

- Flagged takes bypass the dual-track mic→You shortcut and mic enrollment. Live initialization does not seed an unqualified legacy local-user voiceprint, and mic audio is not enrolled as a known local speaker.
- Whole-file Pyannote receives `trust_legacy_user:false` **before** its centroid/voiceprint match block, so polluted legacy enrollment is not consulted. Direct file/recording entrypoints read the same metadata, not just `diarize_meeting`.
- Post-inference user-cluster assignment and transcript `You`-overlap inference are disabled for flagged takes, including remote-only system/Guest rows and a future adapter returning an erroneous user index. A stale `You` without overlapping inference becomes Unknown, not a confident identity.
- Existing enrollment files/data are **not deleted**. Historical unflagged recordings retain the prior convention. Invalid provenance cannot silently grant trust; unreadable/malformed metadata fails the operation.
- Explicit manual names survive; generated/source pieces of a flagged manual composite are not promoted to people. Text, IDs and timings remain unchanged.

`speaker_labels.rs` supplies the shared `is_generated_part`, `manual_names` and `is_manual_label` predicate for offline diarization and retranscription. It covers `Microphone`, mic, system, system audio, Guest, You, Unknown, numeric Speaker N and source composites. `Microphone` is refinable, never a human name. Real names (including names inside composites) remain manual. Enhance retains flagged mic as Microphone; whole-file anonymous speaker refinement is not source→identity proof.

## Observation-only qualification via actual app controls

Settings → Meeting Detection → **Zoom: observation only** owns a bounded local in-memory journal, separate from recorder controls:

1. On the **single final combined build**, enable read-only Meeting Detection, enter machine, actual macOS/client version, UI language, Accessibility inventory and route. No separate DMG, local build or Mac action is requested during software preparation.
2. Start the journal while idle; mark **actual observed state**, not an anticipated click: idle, preview, waiting, joined, joined-muted, silence, reconnecting, device changed, left with Zoom open, permission lost. Markers never feed the detector or authorize capture.
3. Native observation entries include Unix-ms and monotonic elapsed-ms timestamps, candidate presence, fixed allowlisted reason and opaque observed session. No raw AX labels/window titles, audio, transcript text or paths. Unknown permission loss stays Unknown, never End. Version metadata is bounded; machine/route/language/permission/truth are enums.
4. Stop and explicitly export JSON before app exit; download is local, never uploaded. Start refuses to overwrite an existing journal; clearing is explicit and UI-confirmed. A 3,600-entry cap stops collection and records `capacity_reached`; no unbounded memory or background file writer. This is not durable across app restart.
5. Compare truth/observations manually against the #7 matrix: repeated preview/waiting vs muted join, permission revoke/restore, mute/silence/reconnect, device change, leave-open and rejoin, both M4 Pro/48 GB and M1 Pro/32 GB and supported EN/RU/client/route classes. Record missed/false transitions, unknown intervals, uncertainty and measured delays. An UNKNOWN, shared waiting signature or missing scenario is **not a pass**. Leave-open remains unresolved.

Commands are Zoom-only: `start_zoom_observation(metadata)`, `mark_zoom_observation(truth)`, `stop_zoom_observation`, `export_zoom_observation`, `clear_zoom_observation`. Exports retain `physical_acceptance:not_run`/`qualified:false`; they are evidence preparation, not an automatic certificate. A reviewed, evidence-backed production qualification mapping would require a subsequent software decision; there is no UI/CLI “force qualified” switch.

## Red → green verification and limits

Runtime red evidence against `7d3ff8a`:

- Frontend disable-after-awaited-state and unqualified-positive tests both failed (`1` stop/start instead of `0`). Native preflight accepted the unqualified candidate. Source-label regression treated Microphone as a manual name.
- Actual native Stop wrappers/prefix: delayed A after manual B and native-disable while queued both returned `Completed` in the baseline. The baseline harness adds only an unused optional expected field for injection; its production guard behavior is unchanged.
- Actual `diarize_meeting` + real SQLite + production voiceprint decision block: flagged remote Guest consulted the polluted legacy user match once (`1`, expected `0`). Unrelated legacy/manual-name controls passed.

Reproducible commands (Cargo/Rust dependencies pinned to repository lock; Bun groups separate):

```sh
python3 -I frontend/scripts/test-meeting-lifecycle-portable.py
python3 -I frontend/scripts/test-guarded-recording-stop-portable.py --baseline # expected FAIL
python3 -I frontend/scripts/test-guarded-recording-stop-portable.py
python3 -I frontend/scripts/test-offline-identity-portable.py --baseline # expected FAIL
python3 -I frontend/scripts/test-offline-identity-portable.py
python3 -I frontend/scripts/test-retranscription-sources-portable.py
# from frontend, separate processes:
bun test tests/lib/meeting-automation.test.ts
bun test tests/lib/meeting-lifecycle.test.ts
bun test tests/hooks/zoom-automation.test.tsx
bun test tests/hooks/guarded-recording-stop.test.tsx
bun test tests/hooks/zoom-observation.test.tsx
bun node_modules/typescript/bin/tsc --noEmit --project tsconfig.json
```

Harness boundaries: lifecycle includes production tracker/classifier/native ownership/snapshot/journal/policy modules, stripping only Tauri command annotations; macOS FFI is cfg-excluded. Guarded-Stop harness includes actual wrappers, pre-effect inner prefix and state-query body, with transport/manager and post-accept capture/save work stubbed; it establishes the atomic authorization boundary, not hardware shutdown. Offline harness includes the actual command, naming/SQL/persistence and real centroid→voiceprint decision block, with synthetic inference/DSP and contact-link stubs; existing enrollment fixture bytes are retained, reads/writes counted. No real model/audio fixture or acoustic pass. Frontend tests exercise actual launch/event/recorder hook/service/Controls/observation components with mocked Tauri transport. Final pass counts and type-check outcome are in the completion handoff.

No full desktop native/macOS SDK compile, Next production build, installed app, physical/model/ASR accuracy, DMG/packaging, Actions trigger, push, merge/release, user-data/Mac operation, subagent or issue closure occurred. Prior R0 test counts are historical; R1 command-path regressions supersede their ownership/identity assumptions.

## Issue AC status after R1

| AC | Implemented/tested software | Not established |
| --- | --- | --- |
| Verified automatic join/leave and research-derived latency bounds | Qualified-fixture recorder path, native qualification gate and observations | **Required real automation not achieved**: all real classes capture-blocked; physical UNKNOWN/bounds pending |
| Idle/test/preview/waiting negative; mute/silence retention | Candidate negatives, observation-only gate, Unknown retention | Actual UI signature distinction, false-positive/negative rates |
| Muted join/reconnect/device change/leave-open | Synthetic sequences, scoped provenance, timestamped truth journal | Actual supported client/route behavior; leave-open unresolved |
| Manual Stop suppresses same call, next call rearms | Token reservations/tombstones, durable native owner/generation, stale-stop and disable-race tests | Installed-app/physical call qualification |
| Automatic sequence + Mac scenario; visible state/errors | Actual recorder/Controls/qualification paths and visible unqualified status | Physical half and full #6 identity/double-talk acceptance |

Coordinator owns independent Opus review and eventual combined integration. Keep #11 namespaces and one recorder authority; do not claim room speaker differentiation or successful local-user separation.
