# Issue #10 R2 follow-up: local fixes and integration boundary

**Combined integration update (2026-11-18):** [#10/#11 integration](ISSUE10_11_INTEGRATION.md)
now wires the shared hook/service/native authority and guarded Stop. Standalone
branch limitations below are retained as historical evidence, not current wiring
claims. Real-source qualification and physical/desktop gates remain open.


Review base remains fixed at `8997242`. Native start-authority follow-up is the
separate local commit `1c964ec`; its API, serialization and limits are in
[RECORDING_START_AUTHORITY.md](RECORDING_START_AUTHORITY.md). This note covers the
subsequent R2 consumer/identity fixes, not acceptance of a combined #10/#11 build.

## R2-1: actual recorder consumer must require intent

`launchRecording(push,{automated:call})` carries the identical
`{kind:'automated',call}` object in the event or routed `AUTO_START_KEY` JSON.
Manual launchers carry `{kind:'manual'}`. Native tray launch writes explicit manual
JSON too. Old bare `true`, missing intent, invalid JSON, empty objects and malformed
call bindings are no action; they never fall back to manual capture. The home
record button is independently explicit manual intent and never consumes another
call's reservation.

The existing hook validates source/session/token and the expected pending token
**before** calling the existing transfer helper. A delayed A event cannot take B.
A valid route can survive recorder unmount/remount; missing expected reservation
is rejected. This is a local consumer correction, not a second expiry or native
stage authority. No new reservation keys, phase/timestamp conventions or priority
are added. Existing reservation helpers are deliberately unchanged.

Red evidence on `1c964ec`: new actual hook + real service + mocked Tauri transport
regressions dispatched an unguarded native start for malformed/missing automation;
explicit typed manual route was not recognized. Green: 10 hook tests, including
corrupt JSON/empty object/missing process, absent expected reservation, malformed
event/route after reload, manual home/event/route with corrupt automation storage,
stale A versus B and valid typed routed intent. Mocked positive starts do **not**
prove the hook supplies native `expectedStart`; native correctly rejects lifecycle
starts without it until the coordinated integration.

### Exact remaining #11 dependency (not implemented here)

At integration replace the local consumer-only `startIntent` parser with #11's
`readRecordingLaunchIntent(payload)` / `takeRecordingLaunchIntent()` discriminator
(absent/manual/automated/rejected). Use:

- `takeAutomatedStartResult(expectedCall?)`, not nullable parse as intent;
- `automatedStartState()` pending/preparing/native_unresolved/rejected/absent/recording;
- `freezeAutomatedStart(call,capability)` before dispatch;
- service `getRecordingStartCapability`, `resolveAutomatedRecordingStart(expected)`,
  `cancelAutomatedRecordingStart(expected)` and fifth `expectedStart` argument to
  `startRecordingWithDevices(mic,system,title,lifecycleSession?,expectedStart?)`;
- `beginAutomatedRecording(call,lease)` and
  `applyAutomatedStartResolution(call,expected,resolution)` with full tuple/actual
  epoch/busy validation. A native fence survives finally/reload/manual release and
  promotion until a valid idle barrier; an error is not a receipt.

No unaccepted #11 branch/shared code was imported and no fake exact shared exports
were added. **Combined hook, reload/native settlement, common timestamp semantics,
A-recording prevents B, consumed-session suppression and cross-source races remain
pending actual production frontend/native integration and regression runs.** #11
owns expiry/phase state; a simple idle snapshot cannot settle a queued start.

## R2-2 / R2-3: generated user components are not manual identity

Enhance freezes `IdentityPolicy::for_folder(original_session_folder)` before
source selection or temporary decode and passes it through the manual-range merge.
On unverified identity, only actual non-generated names from a manual composite
are retained (`Alice + You` / `Alice+You` -> `Alice`). Generated hints never restore
You. Trusted legacy hints/composites retain their prior behavior. Existing schema
has no explicit per-row label-origin proof for a deliberate rename to a reserved
source word; no such generated component is promoted to acoustic identity. #14's
provenance/edit/version integration remains separate.

The shared label helper recognizes spaced and compact composites consistently.
Unmatched offline rows are sanitized even with no inferred turn: `You + Guest`
becomes `Guest`, `You+Speaker 2` becomes `Speaker 2`, pure `You` becomes `Unknown`;
manually evidenced names remain. Text, row IDs, timestamps, source audio and
existing enrollment bytes are not deleted or rewritten by that policy.

Red: production Enhance helper returned `Alice + You`, and actual
`diarize_meeting` with real SQLite retained unmatched `You + Guest`. Green:
production Enhance/source-policy harness 11 passed; real command/SQL/production
voiceprint-decision harness 6 passed (11 transcript rows per fixture, both selected
engine adapters synthetic). These do not execute model/ASR/audio inference or the
full Enhance desktop job.

## R2-4: malformed metadata is not documented legacy

One authoritative folder loader now serves Enhance and offline diarization.
`metadata.json` must be an object with boolean provenance fields when present.
Null, arrays, scalars, malformed JSON, non-boolean provenance and read failures
abort preflight. Missing file or missing fields in a valid object retain the
historical policy; malformed data cannot silently opt into it. The folder loader
runs before decode/ASR or transcript replacement; it does not repair/delete user
metadata. The in-memory `from_metadata` policy remains fail-closed for malformed
values. Tests distinguish null/array/bool/number/string from legacy missing fields.
#14 must use this ORIGINAL session policy in both ordinary and version-candidate
inner paths, not the temporary WAV parent.

## Verification limits

All fixtures are generated non-private metadata/text or synthetic transport/model
results. Native lifecycle/scoped-provenance suite also passes (36). Native start
and Stop suites are separately attributable to `1c964ec` (13 and 12). Frontend
mock-heavy groups run separately; final counts/typecheck are in the handoff.
No full desktop/macOS compile, physical/model/ASR/acoustic test, installer/build,
branch integration, push, Actions, subagent, release or issue closure. Real Zoom
and Discord capture remains unqualified and blocked. No room speaker separation
or successful local-user acoustic attribution is claimed.
