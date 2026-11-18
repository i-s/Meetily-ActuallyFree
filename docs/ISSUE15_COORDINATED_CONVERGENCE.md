# #15 coordinated software convergence

## Attributable local histories

Standalone #15 software was accepted at
`bd89425d974e5a9334ecfb61f460fce0679efbd7`, including independently rechecked
SQLite release contention/priority fixes. This convergence preserves that history.

1. Merge `a0832aeb2045a82e0723a9805b0b3c0cc7c1e89f` has parents
   `bd89425d974e5a9334ecfb61f460fce0679efbd7` and accepted
   `3d36ddfeb146367cba1578f2d26a485db4201c87` (#10/#11/#12/#14).
2. The following local merge imports accepted
   `21048259e1dfe99e8ebcea7bd37d77f50db86c45` (#16/#17), preserving its
   exact-source suggestion confirmation and Unicode mark handling. It shares
   the already integrated #16 approval history, not a copied replacement.

Neither merge is a default-branch integration, push, release, installation or
physical acceptance. #18 and the canonical-summary bridge are deliberately absent.

## Resolved shared ownership

- Both manual recorder routes and automated `start_recording_with_lifecycle`
  reserve #15 priority **before** the shared engine lifecycle lock. Under that
  lock, the existing native v1 authority still claims the exact frozen ticket,
  validates source/session qualification, and binds the native lease. Resolve,
  cancel and guarded Stop retain their existing ownership. Failed setup or a
  dropped priority-acquisition future releases its RAII reservation. No second
  engine lock was retained from the textual merge conflict.
- The candidate diarization blocking scope keeps #15's thread-local cancellation
  checks but uses the accepted resolver and **original session folder**, not the
  decoded WAV parent. There is no duplicate identity mechanism. The same ordinary
  `diarize_version` path enforces source routing/identity before decode, with
  `finish_with_identity` guarding its candidate commit.
- Queued reservation now calls `meeting_identity_policy` and saves its effective
  snapshot beside untouched opaque recording metadata, as manual reservation
  already does. The model-availability gate uses that same effective room policy.
  Retry retains settings/checkpoints. A metadata edit cannot weaken frozen
  restrictions between reservation and ASR preflight.
- Automatic candidates remain unselected, with independent row/source maps.
  Manual corrections and original live snapshots are not projected onto generated
  text or overwritten by queue completion. Only existing explicit revision-CAS
  selection changes the projection; Notes retain their own CAS and ownership.
- Cancellation drains owned blocking work before resource admission is released.
  Exact-epoch release reconciliation still runs without model admission; SQLite
  contention cannot replay local/CLI work, overwrite defer/cancel/new ownership,
  or permanently strand the next job after the writer becomes available.
- Approved vocabulary remains an immutable attempt-start spelling snapshot,
  separate from speech and Notes. Artifact/cache identity includes its revision.
  Suggestions are read-only until exact explicit confirmation checks the selected
  meeting/version/revision/row and whole token under the writer transaction.
- Zoom and Discord real adapters both remain `qualified:false`. Test-only source
  setters do not exist in the application. No real capture/service authority is
  inferred from synthetic passing tests.

## Focused combined verification

All checks use existing warm lightweight targets, synthetic nonprivate fixtures,
real SQLite and production command/repository bodies. No desktop/model build.

| Command / group | Result | Adapter boundaries |
| --- | --- | --- |
| `python3 frontend/scripts/test-post-call-native.py` | 4 passed | Production queue worker, awaited refinement, source-policy helpers, ASR preflight, original-folder resolver and isolated CLI adapter; Tauri transport, model/source decoding/inference and settings transport synthetic; fake CLI only. |
| `python3 frontend/scripts/test-offline-identity-portable.py` | 46 passed | Actual ordinary/candidate commands, identity resolver, source filtering, shared inner persistence and version selection; deterministic DSP/inference/voiceprint boundary, real SQLite and migrations. |
| `python3 frontend/scripts/test-recording-start-authority-portable.py` | 36 passed | Actual lifecycle start prefix, authority/source dispatch, commit, barriers, guarded Stop prefix and #15 Priority; setup/capture/drain/save effects synthetic. |
| `python3 frontend/scripts/test-meeting-native-integration.py` | 60 passed | Mounted hook/Controls, production Rust authorization through JSON-lines instead of Tauri; synthetic source qualification/device/save effects. Expected injected device-start failures log errors; no failed tests. |
| `python3 frontend/scripts/test-post-call-portable.py` | 28 passed | Production queue/runner/store/evidence/priority; includes all three accepted SQLite contention/priority regressions. |
| `python3 frontend/scripts/test-recovery-database-portable.py --offline` | 85 passed | Actual save/recovery/Notes/version repositories with SQLite and generated FFmpeg fixture; no audio recognition. |
| `cargo test --offline --locked --manifest-path tests/vocabulary/Cargo.toml` | 55 passed | Actual approval/suggestion/version repository and native API adapters; read-only/clipped context, stale/exact confirmation, empty/invalidation and Unicode marks. |
| `bun frontend/scripts/test-isolated.mjs` | 62/62 files passed | Frontend groups run in separate processes, with their documented transport/component mocks. |
| frontend `tsc --noEmit` | exit 0 | Standalone types only, not a Next/native build. |

Regression evidence: removing the queued effective-policy snapshot fails the
new native reservation assertion; restoring it passes. Reversing the lifecycle
priority/engine order makes the new combined start test time out before background
cancellation; restoring the correct order passes. That test exercises manual and
automated starts, pending-acquisition abort and failed device setup. Existing
identity tests exercise simultaneous mic/system rows, source-neutral generated
candidates, ordinary manual names/corrections, restrictive restore and explicit
selection. Native/UI authority tests exercise exact v1 leases, lost callbacks,
stale ownership, cancellation/settling and guarded manual/automatic Stop.

The portable adapters now include the **real** priority/cancellation modules and
#17 suggestion-proof module, rather than replacing their mechanisms with mocks.
The production ASR preflight is extracted unchanged before a deliberately synthetic
decode boundary; this does not establish acoustic quality or full-model timing.

## Open gates

No delegates, physical Mac, private recordings, credentials, real subscription or
network calls, Actions, DMG/packaging, push or release. No #18 unaccepted history or
canonical-summary bridge. Full native/desktop and Next verification require a
separately allocated exclusive heavy-build slot; none was run in this convergence.
Physical 5–90 minute RU/EN, overlap/leakage quality, real source qualification,
battery/thermal behavior, maximum inference yield and recording readiness remain
open as documented in the owning feature notes.
