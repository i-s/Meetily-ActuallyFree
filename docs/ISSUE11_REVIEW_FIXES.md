# Issue #11 R1 review fixes and integration obligations

## Attribution and safety boundary

- Assigned accepted software base: `4269de67c6747b2f5fce72596826c05dadbb3e1c`.
- Reviewed initial implementation: `d4da3c00cfab64c65e54f6b83cdbc3a85354329a`.
- **Finding 1 / journal is separately attributable:**
  `ef7ee5dec760a53bc3a6d5849406394a60ad3b79` adds the bounded native observation
  journal, sanitized facts, normal UI/export, truth comparison and exact Mac
  protocol. This commit does not pretend to fix the later ownership/clock cases.
- This follow-up fixes findings 2–4 on top of that journal, with focused production
  core/entrypoint regressions. No #10 pending branch was cherry-picked or edited;
  its working interfaces were read and coordinated through the coordinator.
- Native real Discord `qualified` stays **false**. Nothing here authorizes
  unqualified capture, changes a native capture source, grants RPC access or adds
  an override. Mac qualification and the issue's full acceptance stay open.

## Per-finding red → green

### 1. Missing observation evidence and manual-truth comparison

At d4da3c0, a reproducible interface inventory reports MISSING for native journal
entrypoint, bounded typed journal, individual connected/disconnect fact fields
and offline manual-truth comparator. This is an **absent-interface baseline**,
not a claim that hardware observations failed or a fictional old test ran.

Journal commit ef7ee5d adds 4096-entry volatile native storage, native monotonic
scan intervals/manual truth, exact finite scenario/marker choices, per-cue facts
and specific Unknown reasons. Unrelated control labels are filtered before
snapshot retention; export contains no raw AX tree, channel/participant/message
content or audio. It distinguishes absent client / disabled observation /
unsupported reader / candidate Active / unqualified call support. See
[ISSUE11_DISCORD_OBSERVATION.md](ISSUE11_DISCORD_OBSERVATION.md) for exact
mute/leave-open/reconnect/permission/idle sequences and comparison/promotion gate.

Green: journal core **3**, offline comparator **6** (including actual CLI),
mounted observation UI **1 / 14 assertions**, normal settings **1 / 6 assertions**.
No actual AX/Mac/audio trial was performed. Architectural replacement is **not
established as required**; current public-signal adapter is simply unqualified.

### 2. Unobserved time was counted as join/leave evidence

Exact old production tracker probes:

- Active@0 then Active@3,600,000 returned **Start(1)** instead of None.
- Active@0,@3000; Inactive@4000 then @3,600,000 returned **End(1)** instead of None.
- Backwards timestamps did not start fresh evidence and prevented a valid new
  debounce in the backwards-clock probe.

All **3 red tests failed** before the fix. The production tracker now stores
`last_sample_ms`. A timestamp backwards or a gap **over 10,000 ms** resets both
candidate/grace timers before processing the new sample. It retains an existing
session; neither missing observations nor worker sleep prove leave. 10 seconds
allows the 3-second poll, bounded AX request and scheduling margin, not arbitrary
suspension. Existing positive grace tests now provide continuous 3-second samples
rather than treating two endpoints 45 seconds apart as observation.

Green: exact production evidence/tracker core **9 tests**, including the 3 probes,
continuous real grace, Unknown/reconnect, backwards time and retained session.

### 3. Manual-busy call was incorrectly queued for after Stop

Red: Discord session first observed while `is_recording=true`; then manual take
ends and same session is observed/reloaded. Old bridge dispatched **1** start
instead of **0**. This was reproduced against unchanged ef7ee5d production modules.

Fix: durable per-source/session tombstone is written and read back **before any
async admission**, even when manual capture, another owner, pending request,
in-flight startup or stopping state is busy. Busy calls are consumed, not queued.
Reload and manual Stop cannot rearm the same observed session. A new confirmed
session can be admitted. Storage failure stays fail-closed. No free-text queue,
priority or automatic retry was added.

Green: manual-busy → Stop → repeat snapshot → new controller/reload stays at 0;
only a new session increments starts. This is in the ownership regression group.

### 4. Shared pending overwrite / private Discord priority / stale authority

Red probes execute **unchanged ef7ee5d** `meeting-automation.ts` and
`discord-automation.ts` in a fresh temporary directory, changing only test import
paths (no production shims). Original six ownership probes were **0 pass / 6
fail**. Expanded final probes are **1 pass / 7 fail** on that same baseline:
manual-busy rearm, pending Zoom vs Discord dispatch, Discord then Zoom overwrite,
revoked async admission, same-process replacement while awaiting Stop, native
lease replacement/missing lease. The disable-after-await control already passed
before the fix and is retained. Red logs are diagnostic artifacts outside git.

**One common implementation** is now in `lib/meeting-automation.ts`:

- `AutomatedCall` retains app/process and adds source/sessionId, unique opaque
  token and requestedAt. Legacy active display remains readable, but tokenless
  owners have no valid reservation authority.
- `markAutomatedStart(call): boolean` synchronously rejects existing pending,
  in-flight or automation-owned take; writes a newly minted token only if storage
  is available. No source can overwrite another, and no dispatch follows false.
- `automatedStartInProgress()` reports pending/in-flight ownership.
- `takeAutomatedStart()` transfers the **same token** to in-flight, never remints
  it. Expired/future-dated requests cannot authorize capture. A recorder knows a
  request existed before taking it, so a rejected automatic request cannot be
  downgraded into an unguarded manual Start.
- `releaseAutomatedStart(expectedCall)` only releases matching source/session/
  process/token pending/in-flight state. A delayed finally from A cannot clear B.
- `beginAutomatedRecording(expectedCall): boolean` only promotes the exact live
  in-flight token and refuses an existing recording owner.
- `reservationCurrent(expectedCall, phase)` validates source/session/process/
  token and the authoritative stored 60-second pending/in-flight lifetime. Phases
  are exactly `pending`, `inflight`, `recording`.
- `endAutomatedRecording(expectedCall)` scopes an old Don't stop it toast to its
  exact recording token; it cannot revoke B. The no-argument form is reserved
  for explicit manual current-take control, not delayed automated callbacks.

Discord's private REQUEST_KEY/marker and `takeDiscordStartRequest` are **removed**.
The recorder never lets Discord metadata override a Zoom pending request. Root's
legacy offer path also checks `markAutomatedStart`'s boolean before dispatch.
Discord reserves before awaiting admission and validates its same token after
every external await, including fresh native evidence and a second recording
snapshot. The recorder checks in-flight authority after model preparation, after
Discord native preflight, before native capture and after native completion; a
revoked completion never adopts/clears a replacement owner. All automatic releases
are scoped. Explicit manual Start/Stop remains distinct.

**Stop:** the bridge requires a locally current recording token AND native
`recording_lease {generation, owner:{source,session_id,app,process}}`. It copies
the expected lease, rechecks local token/Labs/current ended signal and the exact
native lease after all awaits, then supplies that unchanged lease to the guarded
Stop dispatcher. It never upgrades A's request to B's generation. A missing lease
or missing guarded dispatcher causes **no Stop**, no owner clear, no save/UI
mutation. Duplicate accepted Stop dispatch is keyed by token + generation.

**Deliberate standalone limitation, not an integrated-native claim:** the assigned
base has no #10 native lease authority or guarded route/controls. The root Discord
bridge therefore supplies **no stop dispatcher**; the old unguarded normal-Stop
callback has been removed. #10 owns the actual native authority and guarded Stop.
The normal recorder-entrypoint test proves automatic Stop is blocked without a
lease and explicit manual `requestRecordingStop` still works. Native generation
snapshots in bridge tests are mocked; they do not prove atomic native rejection.

Green focused tests include simultaneous async Zoom/Discord claims (exactly one
winner), both pending/in-flight orderings, late A release/promotion after B,
manual-busy/reload, revoked async admission, same-process replacement during
Stop await, native generation changing while local owner is stale, disable during
final await, lease absence, model-loading revocation preserving pending Zoom and
manual controls. Final per-file counts are recorded in the coordinator handoff.

## Final focused verification

- Rust 1.85.1 standalone compilation of exact production cores: evidence/tracker
  **9 passed**, bounded journal **3 passed**, **0 failed / 0 ignored**.
- Python standard-library comparator/CLI group: **6 passed**. Synthetic finite
  truth/cue fixtures only; no Discord client, audio, models or Mac operations.
- Bun 1.3.10 files run in **separate processes**: ownership regression probes
  **8**, Discord bridge **8**, shared reservation **8**, mounted real recorder
  **1 / 20 assertions**, observation UI **1 / 14 assertions**, normal settings
  **1 / 6 assertions**. **27 frontend tests passed, 0 failed**.
- TypeScript no-emit check and `git diff --check` passed in the full focused run.
  Final incremental shared-expiry/ownership and settings checks also passed.
- No full Tauri/native command-wrapper or macOS FFI compilation ran; standalone
  Rust tests do not prove that integration. No combined native lease, actual
  guarded routed Stop/save, installed build or physical acceptance pass is claimed.

Reproducible red/green artifacts (outside git):
`/tmp/issue11-r1-journal-red.log` (missing-interface inventory),
`/tmp/issue11-r1-gap-red.log` (3 failing clock probes),
`/tmp/issue11-r1-gap-green.log` (9 passing production-core tests),
`/tmp/issue11-r1-owner-red.log` (initial 6/6 failures),
`/tmp/issue11-r1-owner-final-red.log` (expanded 1 pass / 7 failures),
`/tmp/issue11-r1-owner-green.log` (8 passing probes).
Full final focused output is in the coordinator handoff. These logs are local
verification artifacts, not a portable public environment or physical Mac data.

## Required combined-tree assertions (coordinator / #10)

These remain integration obligations; do not claim they ran on this isolated tree.

1. Resolve `meeting-automation.ts` once, using the exact common contract above.
   #10 callers must supply `expectedCall` to release (no unconditional finally),
   retain the same token through native startup and honor false mark/promotion.
   Preserve pre-await consumed-session tombstones for both sources.
2. Wire Discord `stop(expectedLease, expectedCall)` to #10's guarded
   `requestRecordingStop(navigate, expectedLease)`. Preserve the old expected
   generation verbatim through route storage/event detail, RecordingControls and
   native `stop_recording {args:{save_path,expected}}`. Missing/malformed guard
   must be discarded, never treated as a manual Stop.
3. #10's recorder lifecycle lock must atomically compare generation + native
   source/session owner and enabled automation **before** changing recorder/UI/
   save state. Manual Stop is unguarded intentionally and remains functional.
   A stale stop/keep-recording release from A after B starts must leave B's native
   recording, UI and saved take untouched. Don't stop it must release native
   ownership with A's exact expected lease, then only its matching local token;
   a late A postprocessing callback must not unconditionally clear B's local
   reservation.
4. Native start admission must validate the current source/session and produce
   its matching recording lease under the same lifecycle authority. The native
   session-owner lookup must recognize Discord only after its qualification gate
   and current active evidence succeed; do not trust arbitrary WebView owner
   strings. Normalize the call/native owner `process` consistently (our opaque
   process key is not a capture target). Current #10-only Zoom lookup is not proof
   of Discord owner support. No user-selected observation scenario can qualify it.
5. Test both simultaneous source orders, both pending/in-flight phases, manual
   busy/Stop, reload, disable/re-enable/keep-recording, late release/stop A after B
   starts, revocation during native startup, and independent Unknown/reconnect.
   Run actual native expected-lease rejection and normal routed Stop/save
   entrypoints, not just mocked callback assertions. Keep real unqualified
   Discord strictly observation-only on that combined build.

This scope did not cherry-pick pending #10 changes, introduce a second native
owner, run a Mac, package/build a DMG, push, trigger Actions, release or close an
issue. The existing app's recording architecture has not been proven inadequate;
qualification and combined ownership verification remain open.
