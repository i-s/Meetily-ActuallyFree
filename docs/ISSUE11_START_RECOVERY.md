# Issue #11 R2: expirable preparation, durable native uncertainty

## Scope and attribution

Based on review head `22af298c74d4e79092b9500bde1b13bc828e69a8`.
The earlier R1 findings remain fixed; this follow-up replaces R1's incomplete
60-second admission/storage semantics, not the signal adapter. Production
Discord `qualified=false` remains unchanged. No observation scenario, Labs
switch, sequence, receipt or user override qualifies capture.

#10 owns native authority v1, committed separately at
`1c964ec0b71288ebd7e37430579ddcb78b8792df` (parent `8997242`). This isolated
branch reads its contract and serializers, but **does not integrate/cherry-pick
that branch**. There is no new native authority or second native owner here.
Absent that native capability, automatic capture fails closed before dispatch;
explicit manual Start still uses its original unguarded/manual IPC fields.

## Data flow and states

`lib/meeting-automation.ts` is the single common main-WebView implementation.
It preserves `source/sessionId/app/process/token/requestedAt` across transfer.
It never remints a token after model loading, navigation or reload.

| Stored state | Expiry/release | Admission of a genuinely fresh B |
| --- | --- | --- |
| Pending, no native dispatch | Retire after 60,000 ms (also backwards clock); retain consumed session | Allowed if otherwise idle |
| Inflight with explicit `startStage: preparing` | Same local retirement; stale async A cannot freeze/dispatch | Allowed if otherwise idle |
| Inflight with `startStage: native`, persisted exact `expectedStart` | TTL/manual release/finally/reload **cannot erase** it | Blocked until verified native settling barrier reports idle |
| Old/unstaged or malformed inflight | Cannot prove no dispatch; quarantine, do not assume preparing | Blocked; no fabricated native request |
| Active automation owner | Not an expirable preparation; exact local token and native lease required | Blocked |
| Native/manual/stopping take returned by barrier | Historical A is fenced, current recorder is still busy | Blocked; cancel never stops that take |

`automatedStartInProgress()` reports only **live** local work, not expired A.
Its null return is **not permission to start**. `automatedStartState()` exposes
`absent`, `pending`, `preparing`, `recording`, `native_unresolved` and `rejected`.
The native-unresolved state carries the frozen request, or null for a legacy
record whose native identity cannot safely be reconstructed. `markAutomatedStart`
uses the full state, not raw-key presence or that nullable getter.

The stored timestamp, not a supplied callback timestamp, determines local TTL.
`takeAutomatedStartResult(expectedCall?)` distinguishes **absent**, **rejected**
and **accepted**; only accepted transfers the same pending token to preparing.
The optional expected call must match the original routed token. The old nullable
`takeAutomatedStart()` remains compatibility-only, not a recorder intent parser.

A just-completed native reply promotes with `beginAutomatedRecording(call,lease)`
only for the exact live native-stage call and matching native owned lease. The
native-stage record remains even after local promotion/release: releasing JS
automation ownership does not establish that native capture stopped. Late A's
reply, finally, idle receipt or Don't stop it cannot clear/promote/stop B. No
start-recovery helper issues Stop or adopts a historical lease as the current take.

## Native v1 boundary: minimal required integration contract

Types/validators: `lib/recording-start-authority.ts`. Transport:
`services/recordingService.ts`. Matching wire fixture:
`tests/fixtures/recording-start-v1.json` (paths in this section relative to frontend).

1. `getRecordingStartCapability()` invokes `get_recording_start_capability` and
   validates `{version:1,epoch,next_sequence}`. This is a read, **not admission**.
2. `freezeAutomatedStart(call,capability)` synchronously validates current preparing
   ownership, persists **the complete exact** native tuple in native-stage storage,
   reads it back, then returns a frozen copy. No `invoke` may precede this write.
   Tuple: `{version:1,epoch,sequence,source,session_id,token}`. Sequences stay
   canonical decimal u64 **strings**, excluding zero/MAX; byte bounds are epoch64,
   source128, session/token256. No JS-number rounding or fresh-ticket rebinding.
3. `startRecordingWithDevices(mic,system,title,lifecycleSession,expectedStart)`
   sends both camelCase IPC guards to `start_recording_with_devices_and_meeting`.
   Session must match the tuple. Supplying only one guard or a malformed guard
   throws, never drops the guard and sends a manual start. A successful native
   call returns the original native `RecordingLease`.
4. #10's lifecycle lock atomically admits/claims the request before setup,
   revalidates qualified current source/session at the capture boundary and
   commits that request's lease. The high watermark permanently rejects an old
   sequence, including after bounded terminal-detail eviction. Local storage
   checks alone do not implement this guarantee.
5. `resolveAutomatedRecordingStart(expected)` and
   `cancelAutomatedRecordingStart(expected)` invoke #10's settling barriers.
   They wait for admitted native startup, or permanently fence an unseen eligible
   request. No timeout/IPC error/missing capability/idle snapshot is proof.
   A held native startup may make a barrier wait indefinitely. That is unresolved
   safety-over-liveness, **not cancellation success**. Cancel never stops a
   committed/manual take.
6. Validate a receipt's version, **all six expected tuple fields**, `terminal:true`,
   `no_future_commit:true`, native lease shapes, actual busy flags and
   `native_epoch`. `epoch_mismatch` must name a different actual operating epoch;
   normal outcomes must name the expected epoch. `committed_lease` is historical;
   `recording_lease/is_recording/is_stopping` describe the actual current recorder.
   A `retired` receipt is not proof that A never committed.
7. `applyAutomatedStartResolution(call,expected,receipt)` returns `unresolved`,
   `settled_busy` or `settled_idle`. Only the last clears the matching native
   fence (and matching local A owner). Busy retains it, including manual ownership
   and stopping. An old callback after B's reservation cannot apply to B.
   Fresh B obtains a **new capability after settlement**, never a cached A ticket.
8. Future/out-of-order sequence errors contain **no terminal receipt**. They cannot
   clear the fence, even if sequence advancement later makes that request eligible.
   Only a later real barrier can prove permanent fencing. A native restart between
   calls is reconciled by the old request's epoch-mismatch receipt and a freshly
   fetched capability; native still checks that new epoch at dispatch.

## Explicit automated intent through events, navigation and reload

`lib/recording-launch.ts` owns one shared event/route format:
`{kind:'manual'}` or `{kind:'automated',call:originalExpectedCall}`. Discord has no
private request key or priority. Its bridge passes the accepted call to the
ordinary launcher; layout forwards the same call. Manual buttons/tray/toasts send
manual intent explicitly.

`readRecordingLaunchIntent(payload)` / `takeRecordingLaunchIntent()` preserve
`absent`, `manual`, `automated` and `rejected`. `{}`, empty/broken JSON, legacy
untyped `true`, missing automated call, absent reservation, stale token and expiry
are **rejected**, not manual. The producer preserves property presence even if an
invalid caller passes `automated:undefined`. The consumer never repairs A's route
by fetching B. A delayed routed event cannot consume B's pending token.

The isolated old recorder hook is minimally wired to these APIs and the guarded
service transport. #10 owns the definitive hook regression/reload reconciliation
and native routed Stop integration. A combined hook must resolve a persisted
`native_unresolved.expected` on mount/state recovery using the exact tuple, apply
its result, and only then admit a genuinely fresh session. Missing/legacy identity
stays unavailable; never synthesize a replacement ticket from an idle snapshot.
This branch's tests of helper recovery/IPC are not a claim that the complete
combined React/native lifecycle or native guarded Stop/save has run here.

## Consumed sessions and the 256-entry limit

The shared `labsAutoStartConsumed` ledger consumes a source/session **before busy
admission**, independently of the Discord observation tombstone. Expiry, manual
Stop, release, reload and Labs toggles never erase it. No old A or busy-observed B
is retried; only a genuinely new confirmed session may claim later.

Storage is bounded at **256 distinct consumed source/session pairs**. The 256th
can be admitted; subsequent calls fail closed without evicting any old tombstone.
The public state is explicitly `rejected / consumed_capacity_exhausted`, not idle
or a successful automatic start. `automationUnavailableMessage()` and normal
Meeting detection settings show the full-history/unavailable explanation and
safe-recovery instructions. Tests cover all 256 entries, old A/new session refusal,
unchanged history, manual revocation and the mounted settings message.

This is an explicit availability limit, **not silent permanent unavailability**:
manual controls remain usable. Finish/stop/save recordings, end every call, fully
quit Meetily (not merely reload/navigate/toggle Labs), then reopen with a clean
WebView session and join a new call. A complete process exit removes the possibility
of a delayed start from the old process; the new process has its own native epoch.
If the platform restores old WebView/session data and the warning remains, leave
automation unavailable and use manual controls until a clean session is provided;
**do not delete live keys/force idle to bypass the warning**. No in-process reset or
unproven pruning API is supplied. Future safe pruning needs authoritative ended
session/epoch evidence, not just passage of time or a recorder-idle snapshot.

## Verification and remaining gates

Focused groups run as separate Bun processes:

```sh
bun test frontend/tests/lib/meeting-automation.test.ts
bun test frontend/tests/lib/recording-launch-intent.test.ts
bun test frontend/tests/lib/recording-ipc.test.ts
bun test frontend/tests/lib/discord-automation.test.ts
bun test frontend/tests/lib/discord-ownership-regressions.test.ts
bun test --timeout 120000 frontend/tests/hooks/discord-recording-entrypoints.test.tsx
bun test --timeout 120000 frontend/tests/hooks/discord-settings.test.tsx
bun test --timeout 120000 frontend/tests/hooks/discord-observation.test.tsx
bun frontend/node_modules/typescript/bin/tsc --project frontend/tsconfig.json --noEmit --incremental false
python3 -I frontend/scripts/test-recording-start-wire.py
```

The wire script reads the pinned #10 commit's actual production Rust modules
with `git show`, removes desktop command macros for portable compilation and
compares actual serde serializations with the shared JSON fixtures (capability,
request, committed busy/manual busy/retired/epoch-mismatch/stopping receipts).
Requires Cargo/Rust and cached lockfile dependencies for offline compilation.
It never checks out/edits #10 or runs capture. No `.build-tools` helper is a
portable prerequisite. Local timings/toolchain limitations belong in the handoff.

Deterministic JS tests use an injected clock, preserved storage plus fresh module
imports for reload, held native promises and explicitly synthetic native lock/
watermark fixtures. They cover both Zoom/Discord orders, predispatch expiry,
manual/active exclusion, held-admitted start, cancel-before-delayed-IPC, late native
reply, future sequence error/advance, missing authority, malformed intent, full
history and stale A effects after B. The synthetic producer does not establish
atomic native fencing: that is #10's production-core portable harness and the
combined entrypoint gate, not a claim about these mocked callbacks.

Baseline red probes execute unchanged `22af298` in a temporary directory: expired
pending A still reports as current work, and fresh B is rejected after 60 seconds.
Logs are outside git at `/tmp/issue11-r2-expiry-red.log`. Final focused output,
TypeScript result and native serializer result are reported separately to the
coordinator, including any timed-out/uncompleted checks.

Completed local focused results (Bun 1.3.10, separate processes): shared reservation
**19**, launch intent **3**, IPC/validators **5**, Discord bridge **8**, ownership
regressions **8**, mounted recorder **2 / 28 assertions**, settings **2 / 10
assertions**, observation UI **1 / 14 assertions**: **48 passed, 0 failed**. A final
shared-helper re-run after tightening missing-intent rejection also passed all19.
The unchanged-base red probes are **0 passed / 2 failed**, as expected. Exact
production native serializers compiled with Rust1.97.1 and matched capability,
request and all **5** resolution fixtures. The initial serializer build hit its
10-minute limit during dependency compilation; the cached retry completed in
7m16s. Neither run exercised devices/capture. Logs: `/tmp/issue11-r2-focused-final.log`
and `/tmp/issue11-r2-native-wire-retry.log`; environment-local logs are not shipped
as portable prerequisites. Final TypeScript no-emit and `git diff --check` both
passed after the last shared-helper change; this is separate from Bun transpilation.

Still required on the combined tree: definitive #10 hook reload/malformed `{}`
regression; real service-to-native start/settle and routed guarded Stop/save;
future→advance→barrier semantics against production authority; both source orderings
through the combined lifecycle; physical Discord guild/DM/GDM signal qualification.
No Mac/user-data changes, real audio/model trials, full desktop/native build,
installer/DMG, Actions, push, release or issue closure are claimed.
