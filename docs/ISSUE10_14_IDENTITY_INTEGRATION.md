# Combined #10/#11 + #12/#14: session identity authority

## Scope and preserved owners

This feature-branch integration joins the accepted #10/#11 scaffold at
`2796db23258de6baaa5e70b74f7e08f3786dec80` with accepted #12/#14 at
`226ff989997e49a404ab8b684e6168f453f31c4f`. Both histories are retained. It is
not a default-branch merge, installed application, release or physical acceptance.

Both real source classes remain `qualified:false`. The source qualification gates,
shared native start/Stop authority, bounded reservations, guarded manual controls,
Zoom/Discord observation journals and recorder lifecycle are not relaxed. See
[ISSUE10_11_INTEGRATION.md](ISSUE10_11_INTEGRATION.md). Notes still use their own
stable target/revision and native attachment; see
[ISSUE12_14_INTEGRATION.md](ISSUE12_14_INTEGRATION.md).

## One policy before decode

`IdentityPolicy::metadata_for_folder` validates the original session metadata:
missing file/fields remain documented legacy, but malformed JSON, null, arrays,
scalars and nonboolean provenance fields abort. The complete object, including
unknown keys, remains available for `recording_metadata` snapshots.

`diarization::meeting_identity_policy` intersects that file policy with stored
`recording_metadata` and effective `identity_policy` from all ready meeting
snapshots and the current running candidate. A stored restriction cannot be
weakened by changing the file or selecting an older snapshot. Only snapshots from
the same meeting participate; failed/cancelled unrelated candidates do not grant
or revoke identity. Explicit invalid stored provenance fails visibly, not as
legacy. Trust can only decrease; in-person restrictions combine with OR.

- `start_retranscription_command` freezes effective authority separately from the
  opaque metadata object when reserving a candidate.
- `run_retranscription` resolves original-file plus frozen authority again before
  decoding any retained source. Flagged mic hints become neutral `Microphone`.
- Ordinary `diarize_meeting` and candidate `diarize_version` use the same inner
  resolver before `ensure_wav` or any inference. An external/temporary WAV never
  changes the provenance folder when the meeting owns an original folder.
- The resolved authority reaches engine adapters, user-range matching, source-hint
  restoration and fallback labels. A denied legacy user is never recreated from
  an old generated `You`, a polluted voiceprint, or a composite source label.

Candidates retain separate mic/system inference and source filtering during
simultaneous speech even when identity is unverified. Their mic remains neutral;
remote rows cannot acquire `You`. Candidate inference never reads/enrolls legacy
user identity or matches named voice profiles. Ordinary unverified reruns retain
#10's mixed clustering: forcibly collapsing an unverified mic could spread a
manual remote name over leaked voices. Real manual names/composites retain their
existing ordinary-rerun behavior, including stripping only generated `You` from
`Alice + You`; generated `Microphone` is not a manual person.

Room policy rejects ordinary speaker identification. Candidates keep source
rows/text but neutralize any stale generated user hints without decode, models or
shared-room personal identity. Shared-room identity remains outside MVP.

## Snapshots and commit boundaries

`transcript_versions::finish_with_identity` atomically saves candidate labels and
effective authority, guarded by running status. It does not rewrite original
metadata or `source_map`. Failed inference or SQL cannot replace selected rows;
a failing label/authority SQL update leaves both candidate fields unchanged.
Only explicit version selection changes the editable transcript, using the
existing revision CAS and corrections checkpoint transaction.

Manual text, names, row IDs, timing and historical live/corrections snapshots stay
separate from generated candidates. There is no time-overlap copy of old manual
text/names into ASR candidates and no destructive legacy Enhance overwrite.
Historical snapshots remain exact data, including old displayed source labels;
explicitly restoring one is not acoustic evidence or permission for a later rerun
to restore a withheld identity. The existing schema still cannot prove a manual
rename to a reserved generated source word as acoustic identity. Ordinary reruns
retain the native corrections checkpoint before changing labels.

## Verification and remaining limits

`frontend/scripts/test-offline-identity-portable.py` compiles the production
ordinary/candidate commands, shared inner labeling/persistence, identity policy,
voiceprint decision, version/Notes repositories and full checked-in migrations.
It also extracts the actual ASR producer preflight before its decode boundary.
Tauri state/transport, inference/DSP and unrelated contact operations are test
adapters. Synthetic embeddings exercise the production voiceprint decision;
spy counters verify forbidden legacy reads/enrollment and preflight-before-decode.
These do not load models or establish acoustic identity.

`frontend/tests/native/identity-versions.rs` covers external temporary WAV,
malformed file/stored JSON and nonobject values, polluted enrollment decisions,
manual/composite versus generated labels, overlapping capture rows through
candidate -> explicit selection -> text correction/checkpoint/restore, file versus
stored restrictions, an older selected snapshot, room exclusion, source-map/opaque
metadata preservation and injected engine/SQL rollback. Existing legacy controls
remain in `offline-identity.rs`. The full recovery harness separately covers
Notes/version CAS, compatibility first-save revision, normal manual save and
idempotent recovery. Mock-heavy frontend groups run in separate Bun invocations.

No heavy production build or native desktop link is established by this pass.
No private recording, real model/ASR inference, physical Mac/WebView restart,
Zoom-through-speakers remote-only/double-talk qualification, room separation,
installer, packaging, Actions or release is performed. Both source qualification
and #6 physical acoustic gates stay open. Independent review remains separate
from author verification.
