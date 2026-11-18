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

## Combined R1: Stop revocation and routed synchronization

Review base: `f6a9525`. Both P2s are corrected locally; this does not qualify a
real client or alter capture, transcript identity, migrations, or #14.

### Native owner-specific Stop authority

`recording_ownership::Authority` retains bounded facts for only Zoom and Discord,
plus a current-take confirmed-exit flag and sticky revocation. Exact generation /
full owner / Labs enablement remain necessary, but are no longer sufficient:
Stop also requires a qualified, permitted native **End for that owner**. A matching
End arriving during the awaited capture startup is retained through lease
publication; there is no requirement that an already ended call still be Active
at commit. Start's active-call validator is not reused for Stop. Unconfirmed inactive, Unknown,
another session's End, missing owner, keep-recording and stale generations cannot
claim the take. Stable inactive may retain an already confirmed owner End; it
cannot invent one. A valid End still uses the ordinary Stop/save path.

Zoom publishes the original session removed by its tracker's confirmed End;
Discord publishes its own explicit End/session. Source permission/qualification
loss, Labs disable, a competing session, or withdrawal of an already confirmed
End revoke automatic Stop for the current take. Re-enable/requalification or a
replayed old End cannot restore it. Unknown before End does not fabricate exit;
Unknown after End withdraws that grant. A revoked take stays available to explicit
manual Stop or keep-recording; no permission toggle silently reacquires ownership.

Settings publication and observation publication use `SETTINGS -> recording
AUTHORITY` lock order. Source revocation is applied under the same short authority
mutex as lease claim, before publishing new settings. Observations compute current
permissions while holding SETTINGS, rather than republishing a scan's cached
`enabled=true`. Claim never reads adapter/settings locks while holding AUTHORITY.
The engine lifecycle lock still serializes Stop/start and all post-claim effects.
Real observations remain `qualified:false`; there is no qualification override.

### Routed Stop ownership

The launcher now persists Stop on the recorder route too, since its Controls may
not be mounted/synchronized. One valid pending slot is first-wins for storage /
event collisions: an old guarded request is never replaced with a newer lease or
manual fallback. Controls subscribe before draining, retain storage while initial
`isRecording=false`/starting/stopping prevents dispatch, and claim it only once
ready. A false mount followed by remount still leaves the exact request durable.
A per-mounted-Controls request identity suppresses identical event/storage delivery;
native generation/exit claim is the final cross-remount effect guard. Failed manual
delivery clears its deduplication claim, so explicit manual retry does not require
a remount.

Explicit manual button/launcher intent remains manual (`'1'` routed marker or
`{kind:'manual'}` event). A bare/missing event, invalid lease, or an explicitly passed
undefined expected lease is rejected, not converted to manual. Legacy valid guarded
event/routed shapes retain their exact lease. Storage failure can retain an event
in mounted Controls, but cannot promise durability across a WebView loss. No idle
snapshot substitutes another take's lease. State sync remains owned by the existing
recorder hook; no new poller or native recorder is added.

### Red -> green evidence

- Initial extended real Controls/hook/service -> production Rust bridge: **18 pass,
  12 fail**, reproducing source-only disable (Labs still on), disable/re-enable,
  Unknown, qualification loss, competing session, and both false-mount routes.
- Native correction alone: **28 pass, 2 fail** (only the two routed sync repros).
- Expanded Controls red run: **28 pass, 5 fail** (sync, remount/stale A, storage/event
  collision, missing-event/manual semantics). After Controls correction: 33 passed.
- Additional native withdrawal/replayed-End test: **13 pass, 1 fail**, then green.
- Explicit manual retry after transport failure: focused Controls **2 pass, 1 fail**
  before clearing the failed manual claim; **3 pass** afterward.
- End during admitted capture before lease publication: **39 pass, 2 fail** before
  preserving that exact End; this also verifies the hook's immediate guarded Stop
  when its start reply arrives after the call ended.
- Final expanded bridge: **41 pass, 0 fail**, **414 assertions**. Stop is queued at
  the native engine lock using a semaphore handshake, not delayed only in JS.
  Both sources cover valid End, source-only disable/re-enable, qualification loss,
  Unknown, withdrawn/replayed End, competing sessions, unrelated-source permission
  isolation, keep-recording, manual override and stale A/current B. Rejection
  preserves capture/save counts, native take, success/manual callbacks and notices.
- Portable native Stop **14**, start authority/source/settings **34**, lifecycle /
  provenance **38**, all passed. Frontend **43/43 isolated files**, **218 tests** passed; the bridge
  fixture is intentionally skipped there and run separately with its Rust binary.
  Standalone TypeScript passed. No Next/heavy desktop build was run for R1.

The bridge now compiles actual `source_automation_enabled`, settings publication
and observation-publication bodies as well as the existing production authorization
paths. Settings transport/disk I/O and qualified source snapshots are test adapters;
it does not run the whole native monitor or real Tauri/WebView. Native post-claim
hardware drain/save is still replaced by generated-file/count adapters. Separate
FFmpeg/SQLite/identity suites from the merge are not claimed as rerun R1 integration.
No new independent delegate review was run (user prohibited subagents); author
self-review and regression evidence are ready for coordinator re-review.

### Remaining gates

No full native desktop/macOS SDK compile or actual Tauri IPC, capture hardware,
model/ASR/acoustic accuracy, installed app, physical join/leave reliability or latency
bound is established here. Native Stop's post-authorization hardware/FFmpeg path is
stubbed in the combined harness; separate existing storage/recovery suites are not
misrepresented as integrated capture. Both journals still require the single final
combined Mac build qualification. No DMG, push, PR, Actions, release, issue closure,
Mac/private-data operation or subagent occurred. #14 original-session policy/version
integration remains separate. Independent integration review is the next step.

## Combined R2: admission continuity, live Settings and distinct manual Stop

This section supersedes R1's lease-publication reconstruction and first-wins Stop
slot. It does not change either source's qualification policy or import #12/#14.

### Native startup authority

`recording_commands::start_recording_with_lifecycle` creates a native
`recording_ownership::StartAdmission` after source validation/busy rejection and
before awaited model/device/capture setup. The engine lifecycle lock owns the
operation; the short recording authority mutex serializes provisional and committed
Stop authority against settings/evidence publication. Source/global disable,
qualification loss and competing sessions update the admitted owner's sticky
revocation, even if qualified Active A is republished before lease publication.
`StartAdmission::commit` transfers those flags and confirmed owner End into the
new lease instead of reconstructing trust from the latest snapshot. A valid End
during startup still authorizes the normal guarded Stop. RAII destruction on failed
or cancelled startup clears only that admission, not a later session's state.

### Settings command and monitor lifecycle

`set_meeting_detection_settings -> apply_automation_settings` still revokes source
permission before publishing settings. It now restarts the monitor only when
initializing, enabling or resuming it. A healthy monitor reads current settings on
each poll, so changing Discord-only/Zoom-only, notifications or interval settings
does not call `start_monitor -> lifecycle::suspend(true)` for an unrelated take.
An interval edit takes effect on the worker's next poll (it does not interrupt an
already sleeping interval). Genuine source disable remains sticky; global disable,
explicit monitor stop/restart and qualification loss retain their revocation paths.
Re-enabling is not a grant to the old take.

The generated bridge now compiles the production Settings command bodies, the
complete `start_monitor` function and lifecycle publication. Disk I/O, scanning and
worker scheduling are synthetic adapters; spawn/event counters establish whether
the real command path restarts/suspends, not whether a physical poll ran. Qualified
source snapshots remain test-only. The inverse-source and source/global off/on
controls verify that avoiding a spurious restart does not bypass real revocation.

### Routed manual intent and final native claim

`recording-launch.ts` stores a version-2 queue bounded to 16 pending intents and 64
completed IDs. An automated request keeps its full expected lease. A distinct
explicit manual action gets its own UUID, resolves `get_recording_state` once and
freezes that native generation before waiting for Controls state sync or directory
lookup. Resolving and ready manual states are separate; an unresolved binding lost
with the WebView is dropped, never rebound to whatever records next. Same-ID/event
and guarded-lease deliveries deduplicate; stale guarded A does not consume distinct
manual B. A new explicit action after failed transport gets a new ID.

Mounted Controls drain only when synchronized recording state is ready; queued
records remain durable through false initial state and remount. Claims are
module-owned across remount and completed after each attempt. Native
`RecordingArgs.manual_generation` is forwarded through the ordinary Stop command
and checked against the current lease under the same authority mutex as automated
Stop. It bypasses source/Labs permission, not generation matching. Supplying both
guard modes rejects. Successful A followed by a queued manual intent for A cannot
stop C, nor can a delayed manual B after directory/state reply delay stop C. Direct
manual button/tray/minibar behavior remains unchanged. Missing/malformed automated
leases never become manual Stop. Capacity or storage failure drops the request
rather than delivering an unbound Stop; unlike R1, no transient event fallback is
promised when durable queue storage fails.

### R2 verification evidence and limits

- Native startup/Settings red run: **41 pass, 6 fail**, **426 assertions**. Both
  sources reproduced source/global off/on resurrection during admitted capture and
  unrelated Settings command revocation. Intermediate corrected bridge: **57 pass,
  0 fail**, 492 assertions.
- Focused stale-A/manual-B Controls red run: **3 pass, 1 fail**; B remained recording.
  Replaying **044a43a** Controls and launcher against the native bridge also failed
  that exact mounted/remount regression (**0 pass, 1 fail**, 4 assertions; other
  tests filtered), with only one prior save instead of the expected two. These are
  runtime wrong-result repros, not import/compiler failures.
- Final expanded mounted hook/Controls/service -> production Rust authorization:
  **60 pass, 0 fail**, **513 assertions**, including startup End, both source
  permissions, native-lock ordering, stale/successful A, distinct/same manual IDs,
  explicit retry, false sync/remount and late state/directory replies versus C.
- Focused Controls: **4 pass, 0 fail**, 26 assertions. Initial full isolated suite
  found an obsolete entrypoint fixture counting queue wakeups as Stop effects;
  it now consumes bound intents and supplies native state. Its focused rerun:
  **2 pass, 0 fail**, 28 assertions. Final full isolated rerun: **43/43 files,
  219 tests passed** (the native bridge is skipped there and run separately).
  Standalone TypeScript passed. Portable native Stop **15**, start/source/settings
  **35**, lifecycle/provenance **38** passed; the R2 execution plan records commands
  and the author self-review.

Logs are local `/tmp/issue1011-r2-{native-red,manual-red,old-controls-red,combined-final,
manual-green,entrypoints-final,frontend-final,stop-final,start-final,lifecycle-final,
tsc}.log`. `test-meeting-native-integration.py` builds the small generated native
fixture and runs the bridge; `test-isolated.mjs` runs frontend files separately.
The optional `MEETILY_STOP_REVIEW_BASELINE` test input loads old Controls/launcher
copies for the manual regression without changing production files.

No heavy/Next/full desktop build, DMG, Actions, push, delegates or Mac operations
were run. No actual Tauri IPC/desktop ABI, capture hardware, model/acoustic accuracy,
installed app or physical Zoom/Discord reliability is established. Native post-claim
capture/save effects remain synthetic generated-file/count adapters; no private
recordings or model fixtures were used. Both real source classes remain
`qualified:false`; physical UNKNOWN and #6 acoustic qualification remain open.
Author self-review is not an independent review. #12/#14 at `226ff98` stay separate.
