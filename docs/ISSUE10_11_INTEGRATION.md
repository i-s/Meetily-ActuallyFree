# Combined #10 / #11 recorder integration

Integration date: 2026-11-18. Local feature merge, not a default-branch merge,
release, installed application or physical qualification.

- #10 parent: `0819f08f29646e443496d92468e5531728785c15` (includes fixed R2
  review snapshot `8997242` and separately attributable native v1 `1c964ec`).
- #11 parent: `edc29e3276bd2150a082bed74e565b0733fdda58`.
- Common #9 base: `4269de67c6747b2f5fce72596826c05dadbb3e1c`.
- #14 is deliberately separate. No version/candidate/transcript-schema work is
  imported. The final merge SHA/tree and verification logs are in the handoff.

## Intentional conflict resolutions

Eight files conflicted: native `meeting_detection.rs`, root `layout.tsx`,
`MeetingDetectionSettings`, `useRecordingStart`, shared `meeting-automation`,
`recording-launch`, `recordingService`, and the common reservation tests.

The one common reservation/parser is #11's implementation, integrated with #10's
native ticket/lease authority and guarded normal Stop route. Both source workers,
settings/diagnostics and journals remain independent. No private Discord request
or claim key, Zoom tombstone store, priority, second recorder, or translation of
Discord observations into Zoom observations remains. Generic process/media offers
cannot automate an unsupported source. Independent source controllers use the same
source/session/token reservation before asynchronous busy admission.

The Discord and Zoom journal interfaces remain unchanged:

- Zoom: start/mark/stop/export/clear observation commands and bounded typed metadata;
- Discord: get status, start/mark/stop/get/clear observation commands, typed facts,
  platform/client diagnostics and offline comparator.

Historical implementation/review notes are retained, with this note superseding
standalone-branch descriptions of missing transport or native Stop wiring.

## Native qualification and source routing

`recording_start_sources.rs` dispatches **by expected.source**, not through Zoom's
session table for every client. Before setup and again at the capture boundary:

- Zoom requires native automation enabled, master detection, explicit
  `zoom_automation` opt-in (serde default **false** for old settings), current
  qualified Zoom session and matching frozen owner.
- Discord requires native automation enabled, master + Discord opt-in and its own
  current qualified status/session. Its existing process stamp remains
  `discord-macos:<native session>`; it is not a Zoom process or a speaker identity.
- Unknown sources fail closed. Automatic room mode and another selected app are
  rejected; no silent global/source fallback. Explicit global capture remains an
  explicit configuration, not a fallback chosen by automation.

**Every real Zoom and Discord class remains unqualified.** Production has no
qualification override or synthetic setter. The new opt-in never grants it.
Old enabled preferences, journals/truth markers, process presence, mute/silence,
waiting/preview or permission cannot unlock capture. Failed/unqualified/unknown
observations are not fabricated End transitions. Both physical UNKNOWN/leave-open
and #6 acoustic acceptance remain unresolved.

Manual source selection, recording metadata/voice identity policy, offline
sanitization and database migrations are unchanged by this merge. Discord's future
lifecycle qualification would not establish acoustic identity or room separation.

## One start authority across dispatch, reload and late callbacks

The normal root/launcher/hook/service path carries explicit intent:

```text
source-specific qualified synthetic/future observation
  -> common source/session reservation + opaque token + requestedAt
  -> launchRecording(push,{automated: SAME call})
  -> explicit event/routed parser -> takeAutomatedStartResult(expectedCall)
  -> preparing (bounded expiry; safe to cancel before any dispatch)
  -> native capability + fresh selected-source qualification check
  -> freeze/persist exact v1 epoch/sequence/source/session/token BEFORE IPC
  -> real service expectedStart + lifecycleSession
  -> native engine lifecycle lock + source validation + actual lease commit
```

No await separates persisted native stage from dispatch. Model awaits check scoped
ownership. Absent/corrupt/expired/missing automated intent never becomes manual.
Explicit manual buttons, typed route/tray intent and guarded manual Stop remain
ordinary user operations. A manual intent revokes only the captured pending/
preparing request or promotion; it cannot erase a possibly dispatched fence.

`recording-start-recovery.ts` deduplicates the exact persisted barrier operation.
Root startup/native start/stop events and recorder remount use it. Resolve/cancel
wait for the same native engine lock. Cancel-before-arrival fences delayed IPC;
admitted setup may win and return committed/busy. A plain idle snapshot, promise
error, TTL or late `finally` is not a release proof. Only a validated full-tuple
terminal **idle** receipt clears the native stage. Future/out-of-order/error
responses retain it. Receipts use actual `native_epoch`, not the echoed epoch.

Committed response loss is reconciled using the retained exact tuple, native
terminal result and actual current lease; no token is invented from a raw snapshot.
If browser authority is completely missing, source-owned native Stop can still
use the actual native lease, but no browser token authority is fabricated.
Revoked/expired callbacks cannot adopt B, and old A release/promotion/resolution
cannot modify B. Committed/manual/stopping takes remain busy even after promotion
or keep-recording. Historical `committed_lease` is never treated as the current take.
Native start rejects recording **or stopping** under lifecycle serialization.

### Bounded and unresolved states

There is one shared pending/inflight/active protocol and a non-evicting **256**
consumed-session cap. Exhaustion or malformed authority is visibly unavailable,
not a reset/qualification override. Busy sessions are consumed, not queued for
handover. Native terminal detail is bounded at **64**; monotonic sequence fencing
survives eviction (`retired` is not proof a request never recorded). Invalid legacy
unstaged inflight data with no exact native tuple remains quarantined; manual
controls remain available. Do not clear live fences to retry. A hung admitted
native start can keep its barrier pending: safety is not a bounded-time cancellation
promise. Restart/epoch mismatch can settle old exact requests without touching a
new process's actual take. See [v1 contract](RECORDING_START_AUTHORITY.md).

## Guarded normal Stop and keep-recording

Both source controllers independently validate their own enabled/qualified current
exit, read actual native owner/generation and route **that exact lease** through
`requestRecordingStop` -> event/route -> actual `RecordingControls` -> native Stop.
Fresh checks after awaits reject returned calls, permission/qualification loss,
manual/other owners and stale end events. Unknown is neither inactivity nor exit.

Before any capture/save/tray/event effect the native Stop compares full lease and
enablement atomically. Rejected stale/disabled Stop has no save, success analytics,
processing/reset/manual callbacks or success toast. Accepted Stop stays on the
existing capture-drain/save/recovery/postprocessing path. Keep-recording captures
its original lease and calls native owner release; only successful matching release
revokes that browser call. A delayed A toast action cannot release B.

Frontend postprocessing no longer clears a global automation owner. Native
settlement owns release, so a delayed A save callback cannot revoke current B.
No new completion/saving engine or database writer is introduced.

## Verification: what each layer establishes

`python3 -I frontend/scripts/test-meeting-native-integration.py` builds a temporary
native fixture crate and mounts the actual frontend recorder hook and Controls.
The real launch parser, shared reservation/recovery modules, recording service and
wire validators communicate over JSON-lines with **production Rust start
prefix/capture-boundary/commit, source dispatch, Zoom validation, Discord
qualified_owner, resolve/cancel, Stop authorization prefix and state-query bodies**.
Only transport, desktop/device/model setup, opt-in/source fixture setters and
post-accept drain/save effects are adapters. Qualified fixture injection exists
only in the temporary generated test crate, never production. The save adapter
writes generated bytes to an isolated temporary folder and counts accepted saves.

Fourteen combined cases cover both source orders, one recorder/no private keys,
manual busy, wrong-source exit, cancel-before-delayed IPC, held/committed/uncommitted
A, idle-snapshot non-authority, lost reply, reload, delayed A callback after B,
stale routed Stop and old keep action, disable at native Stop boundary, actual
busy/stopping, future error -> advance -> barrier, actual-vs-echoed epoch,
failed startup, corrupted/missing/routed intent, explicit manual Start/Stop/save,
and unqualified/opted-out source rejection. Async assertions await native promises,
not guessed delays. This is cross-language command-path coverage, **not actual
Tauri/WebView ABI or a full mounted root/physical app end-to-end test**.

`test-recording-start-wire.py` is separately only pinned-serializer/fixture
compatibility. Portable native suites separately test production trackers/journals,
atomic authority, source-selection rules, SQLite naming/provenance and generated
FFmpeg recovery fixtures. Full frontend tests run per file because Bun module mocks
must not share a process. Standalone TypeScript and the bounded Next production
export run sequentially; the latter is not a native desktop build or installer.
Final counts, exact tree, commands and logs are reported with the merge commit.

### Remaining gates

No full native desktop/macOS SDK compile or actual Tauri IPC, capture hardware,
model/ASR/acoustic accuracy, installed app, physical join/leave reliability or latency
bound is established here. Native Stop's post-authorization hardware/FFmpeg path is
stubbed in the combined harness; separate existing storage/recovery suites are not
misrepresented as integrated capture. Both journals still require the single final
combined Mac build qualification. No DMG, push, PR, Actions, release, issue closure,
Mac/private-data operation or subagent occurred. #14 original-session policy/version
integration remains separate. Independent integration review is the next step.
