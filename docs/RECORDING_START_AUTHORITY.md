# Native recording start authority v1

**Combined integration update (2026-11-18):** [#10/#11 integration](ISSUE10_11_INTEGRATION.md)
now wires the shared hook/service/native authority and guarded Stop. Standalone
branch limitations below are retained as historical evidence, not current wiring
claims. Real-source qualification and physical/desktop gates remain open.


Follow-up to issue #10 R1, based on fixed review snapshot `8997242`. This is a
native recorder safety interface, not qualification of Zoom/Discord capture.
All real adapter classes remain unqualified. #11 owns browser reservation expiry,
phase/timestamp semantics and transport integration; this branch does not import
its pending implementation.

## IPC and payloads

`get_recording_start_capability()` returns:

```ts
interface StartCapability {
  version: 1;
  epoch: string;          // UUID of this native process authority
  next_sequence: string; // canonical positive decimal, never a JS number
}
interface StartRequest {
  version: 1;
  epoch: string;
  sequence: string;
  source: string;
  session_id: string;
  token: string;
}
```

Capability is a read, not admission or a reservation. Freeze/persist the complete
request before dispatch. Do not rebind an old source/session/token to a freshly
queried sequence. A genuine new request uses the capability obtained only after
the old request has settled; browser consumed-session tombstones still apply.

`start_recording_with_devices_and_meeting` adds `expectedStart?: StartRequest`
(Tauri camelCase) to its existing `lifecycleSession` argument. Both must be supplied
for automated starts and their sessions must match. Native source/session must
match the qualified lifecycle owner. Manual starts provide neither. A lifecycle
start without authority fails closed; the old JS hook is not an authority client
until #11 supplies its coordinated transport. Successful return is still the
original native `RecordingLease`.

Both `resolve_automated_recording_start({expected})` and
`cancel_automated_recording_start({expected})` are **settling barriers**, returning:

```ts
interface StartResolution {
  version: 1;
  native_epoch: string; // actual running epoch, NOT the echoed expected.epoch
  expected: StartRequest;
  terminal: true;
  no_future_commit: true;
  outcome: 'committed' | 'cancelled' | 'failed' | 'rejected' | 'retired' | 'epoch_mismatch';
  committed_lease: RecordingLease | null; // retained historical result for expected
  recording_lease: RecordingLease | null; // actual take now; can be manual/another request
  is_recording: boolean;
  is_stopping: boolean;
}
```

A receipt proves **this exact frozen tuple** cannot commit again, not that the
recorder is idle or that it never captured. Compare the entire echoed tuple and
actual operating epoch, validate the busy/lease fields and retain browser native
fences until a valid idle receipt. `committed_lease` is historical; never treat it
as the current take, stop a different lease, or promote its late callback to B.
Actual lease/busy state governs admission. Epoch mismatch fences the old process
request without touching the new process take. A restart between capability and
start rejects the old ticket; request a new capability only after reconciling it.

Malformed/version-invalid and **future/out-of-order sequences return errors, not
receipts**: there is no terminal proof and no JS reservation release. A future
sequence can become next later; a prior error does not cancel it. Only the next
sequence can be admitted or cancelled. A used sequence cannot be admitted again,
including with a different token/source/session. This is process-local fencing,
not persistence across a native restart; epochs isolate those lifetimes.

## Serialization, races and bounds

`audio/recording_start_authority.rs` owns the UUID epoch, monotonic watermark,
one in-flight claim and at most **64 terminal details**. Input retained strings
are bounded (epoch 64, source 128, session/token 256 bytes); sequence is canonical
u64 decimal excluding zero/MAX. Exhaustion fails closed. Detail eviction returns
`retired` for an old tuple but never makes its sequence usable again. Retired does
NOT assert cancellation/no prior capture. No unbounded cancelled-token registry
or TTL that could reopen an old request is used. Idempotence is the permanent
fence; live recorder state and evictable diagnostic detail may evolve.

Native start claims under the existing recorder lifecycle lock before validation
or device/model setup. Early failure/future destruction settles its claim through
RAII. Successful capture binds its native lease before releasing that lock.
Resolve/cancel take the **same lock**. If cancel wins before a delayed/queued IPC,
it consumes the sequence and that IPC cannot reach setup/capture. If native start
has already been admitted, the barrier waits for it to finish and reports the
committed lease or failure. It does not interrupt/kill device setup or stop a
committed/manual recording. A hung native start can therefore leave the barrier
pending; do not time out into release or dispatch B. This is explicit safety over
liveness, not a bounded-time cancellation promise. Native disable/qualification
checks and existing exact-generation Stop guards remain in effect.

## Synthetic production-path verification

Run `python3 -I frontend/scripts/test-recording-start-authority-portable.py`.
The lock and authority/lease modules are production code; the harness compiles
the actual start authorization prefix, capture-boundary validation, lease commit
and resolve/cancel IPC bodies. Device/model startup and qualification are explicit
synthetic fixtures, not physical/model tests. It covers cancel-before-IPC, queued
start, held setup/capture, late retry after reload, duplicate request, busy manual/
automated take, failed startup, malformed/mismatched owner, missing authority,
future sequence advance/fence, epoch echo versus actual epoch and bounded history
without reopening old tickets. Existing lifecycle tests separately establish that
real observations never qualify. No full desktop/macOS compile is implied.

Combined-only assertions still require #11's shared implementation: persisted
predispatch vs native-unresolved phases; identical expiry timestamps across reload;
freeze before IPC; lost response -> terminal barrier; malformed routed intent never
manual fallback; no B while A native stage unresolved/recording; consumed-session
tombstones; and full hook/service/native transport propagation of expectedStart.
