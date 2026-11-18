# Issue #16: explicit confirmed vocabulary

Live issue read 2026-11-18: **[macOS 12/16] «Исправить и запомнить» термин**,
https://github.com/i-s/Meetily-ActuallyFree/issues/16. Live comments: `[]`.

## Integration and scope

- Runtime base: `226ff989997e49a404ab8b684e6168f453f31c4f` (accepted #12/#14).
- Local #13 tip: `e40f6298424ad706e7e91ad434e7d69be6aba8e2`, reviewed product
  `7f017dbbf75fccb0400a08ff3e48e1a1cb1447d0`, documentation-only follow-up.
- Separate two-parent integration commit: `7d2f6b4cfd42b56c84aba4d4f0712b8c6e6c196a`.
  Both histories retained. Shared `lib.rs` registration and development-map merge
  reviewed; native Notes ownership and selected-transcript/version APIs unchanged.
- Prior fork `42b0607` already supplied vocabulary storage, editing and local
  Whisper hint consumers. Reused rather than introducing another dictionary.
- No #17 suggestions, #15 queue, provider fallback, model training, packaging,
  release, user recordings/data, sign-in or real-service operation is included.

## Normal user flow and ownership

Meeting details → Transcript versions → Correct selected text → Save text
correction uses the original `correct_transcript_text` revision CAS. That native
transaction checkpoints history and preserves row identity, timing, labels and
contact associations. Notes/summary editor preparation and projection refresh
remain the accepted #12/#14 flow.

**Only after native success**, `RememberTerm` offers an empty, user-entered
RU/EN term/name/abbreviation field and explicit **Запомнить / Не запоминать**.
No term is inferred, no model suggestion is requested and no draft is learned.
Confirmation is a separate native operation; decline, confirmation failure,
readback failure or navigation cannot undo the saved correction. Retry invokes
only confirmation, not correction again. In-flight duplicate clicks are bounded;
the native append is independently idempotent. A meeting-keyed component boundary
prevents a previous meeting's confirmation/draft from appearing on another page.
The confirmation is transient UI, not a durable pending-learning queue. After
navigation/restart a term can be entered explicitly in Personal vocabulary.

Settings → Transcription → **Personal vocabulary** remains editable with Parakeet
selected. Edit/delete lines and explicitly Save; a loaded value **and revision**
are required. Stale writes fail visibly and preserve the draft. Reload is clearly
labelled as discarding the draft. Initial read failure blocks saving. A committed
save with failed readback retains success and disables further saving until a
revision is reloaded, rather than guessing ownership.

## Persistence, approval provenance and concurrency

The existing `transcript_settings.whisperVocabulary` remains the global local-ASR
store. `meeting_whisper_vocabulary` and its per-meeting semantics are unchanged.
`VocabularyRepository::get_effective` still merges meeting terms first, then
case-insensitive unique global terms. Unicode lowercase determines duplicate
identity; first approved spelling wins. Before any approval, explicit confirmation
may correct matching legacy casing. Canonically equivalent Unicode sequences
are not additionally normalized. Commas/newlines remain bulk-editor separators;
single-term Remember rejects separators/control characters and empty values.
Each stored scope is limited to 1000 Unicode scalar characters. Oversized updates
roll back completely, never erase the dictionary to fit.

Migration `20261118001600_vocabulary_approval.sql` adds one durable global approval
snapshot: `vocabulary_approval(id=1, revision, approved_terms)`. **Pre-existing
entries have no inferred approval proof.** They retain their previous local-ASR
behavior but are excluded from Codex until reviewed and explicitly saved, or
individually confirmed. Remember approves only its submitted term; it does not
approve the rest of a legacy list. Settings Save explicitly reviews the entire
submitted list, with this meaning visible beside the editor. The existing
retranscription dialog's explicitly typed global-add action approves those typed
terms only. Meeting-local hints are not implicitly imported into Codex context.

Global writes use `BEGIN IMMEDIATE` before reading, plus the existing native mutex.
Append merges against the current value. Replacement compares loaded value and
approval revision inside the same transaction. Failed validation/CAS/SQL leaves
both full vocabulary and approval unchanged. A successful real store/proof change
increments the durable revision; duplicate confirmation does not. Triggers clear
proof and increment revision if legacy/raw SQL changes or deletes the global
store. Thus an older/unconfirmed writer cannot promote arbitrary stored text to
approved provider input. Ordinary transcript/label/Notes writes do not touch this
state. No meeting/contact/Notes retrieval is used to manufacture terms.

## Consumer interfaces (stable seam for #15)

Production repository `database/repositories/vocabulary.rs`:

```rust
VocabularyRepository::approved_global(pool: &SqlitePool)
    -> Result<ApprovedGlobalVocabulary, sqlx::Error>
// DTO: { pub revision: i64, pub terms: String }
VocabularyRepository::global_state(pool: &SqlitePool)
    -> Result<GlobalVocabularyState, sqlx::Error>
// DTO: { pub global: String, pub approved_global: String, pub revision: i64 }
VocabularyRepository::confirm_global(pool: &SqlitePool, term: &str)
    -> Result<Option<String>, String>
VocabularyRepository::replace_global_at_revision(
    pool: &SqlitePool, expected: &str, revision: i64, raw: &str
) -> Result<Option<String>, String>
```

Each snapshot read is one SQL SELECT, not a separate term/read-revision pair.
Tauri keeps existing names, with additive read metadata and mandatory CAS on Save:

- `api_get_whisper_vocabulary(meetingId?)` → `{global, meeting, approvedGlobal,
  vocabularyRevision}`; original global/meeting fields retained.
- `api_save_global_whisper_vocabulary(vocabulary, expectedVocabulary,
  expectedRevision)` → normalized string. All in-tree callers updated.
- `api_confirm_vocabulary_term(term)` → normalized current global string.
- Original meeting vocabulary and selected/Notes APIs unchanged.

**Provider input must use `approved_global`, never `get_global`.** Shared pure
`codex_cli::vocabulary::ApprovedVocabulary`:

```rust
ApprovedVocabulary::global(terms: &str, revision: i64) -> Result<Self, String>
snapshot.revision() -> i64
snapshot.augment_prompt(prompt: &str) -> Result<String, String>
```

Serialized source: `approved_vocabulary`, scope `global`, durable `revision`,
`snapshot_id = vocabulary:global:<revision>`, and a distinct `terms` array.
Maximum 1000 Unicode characters; invalid/control/oversized snapshots fail visibly.
The combined augmented prompt has a 512 KiB byte limit. It is sent in full or
rejected; no prefix truncation or silent empty source for Codex. The existing
subscription-only isolated `cli::generate` and auth-home resolver are unchanged.
Terms are untrusted spelling guidance, **not speech, Notes, citations, attendance,
decisions or agreement**. The prompt explains that a term alone cannot support an
answer. Existing native citation/uncertainty validation rejects vocabulary-source
IDs and confident answers with no speech evidence. It cannot prove that every
claim with a valid citation is semantically entailed; real-model qualification
remains required. Client-supplied vocabulary fields are rejected by the live
request's existing deny-unknown-fields boundary.

`codex_live_ask` loads the approved snapshot natively at dispatch and calls
`service::ask_with_vocabulary(request, canonical, committed_notes, &snapshot,
configured, auth_home, token, is_current)`. The original `service::ask` remains
compatible. `LiveAnswer` adds `vocabularyRevision`; the normal live UI labels its
revision separately from speech/Notes. An in-flight operation retains its snapshot
even if the user edits/deletes terms later; the next request reads the new revision.

**#15 integration requirement:** load `approved_global` at normal saved-summary
job start, construct the shared snapshot, augment the existing speech/Notes prompt
before isolated `cli::generate`, and retain vocabulary revision in job/result
provenance. This branch supplies and tests that shared seam, **not #15's normal
summary job or queue**. Integration into that independently running ticket and
verification of its normal caller are still required; no automatic summary or
legacy paid-provider path is introduced here.

## Local ASR consumers and limits

Existing live `worker.rs` snapshots `get_effective(pool, None)` at recording start.
Existing repeated ASR reads `get_effective(pool, Some(meeting_id))` at job start
and stores its prompt in candidate settings; selecting a candidate still uses
#14's explicit version CAS. Import/serial/parallel Whisper paths retain their
existing vocabulary consumers. Updating vocabulary does not mutate an active
recording's snapshot, previously selected rows, labels or historical versions.

Whisper's existing tokenizer uses **the first 224 tokens**, not 1000 characters.
Later terms can be truncated, especially mixed RU/EN; meeting terms take priority.
The full stored dictionary remains editable. Settings, confirmation and existing
model/retranscription choices explain the limit and that **Parakeet has no hints**.
No runtime claim of exact token coverage, recognition guarantee, model improvement
or changed model weights is made. A single long term may itself be cut at the
budget. Put important terms first. Tokenization failure is not model learning.

## Portable verification and remaining gates

No heavy Next/native desktop build, physical Mac/audio/device operation or real
signed-in service call ran. No installer was built, installed or published.

- `cargo test --offline --manifest-path tests/vocabulary/Cargo.toml`: production
  repository + all original transcript-version/speaker persistence bodies +
  production vocabulary API bodies with Tauri state transport substituted.
  Real temporary SQLite tests cover legacy proof isolation, approval-only revision
  CAS, concurrent append, duplicate retry, full-store rollback, Unicode persistence
  after close/reopen, edit/delete and independent correction/links/Notes/history.
- The same crate compiles unchanged production live-ASR dispatch and Whisper
  token-budget function bodies via `build.rs`. Fixed generated non-silent 16 kHz
  alternating PCM is passed unchanged without/with global/meeting hints. A **fake
  model and character tokenizer** observe delivery, priority and nonempty 224-token
  prefix. This does not run Whisper/Parakeet models, the actual repeat-job audio
  decoder/orchestrator, real tokenizer coverage or speech recognition quality.
- `python3 -I frontend/scripts/test-codex-portable.py`: production isolated CLI,
  context, service and native command bodies; fake subprocess verifies mixed
  RU/EN spelling response and separate source/revision. Shared summary prompt
  helper is exercised through the same adapter, not #15's normal summary caller.
  Recorder, vocabulary DB transport and Notes transport are substituted here;
  real SQLite proof behavior is established separately above. Three optional
  actual-CLI localhost tests remain ignored; no real CLI/service qualification
  was repeated in this issue.
- Frontend `bun scripts/test-isolated.mjs` uses real correction, confirmation,
  settings and live UI with native IPC replaced. Includes explicit decline,
  confirmation retry after projection failure, bounded double-click, no learning
  from drafts, meeting navigation and all accepted Notes/editor regressions.
- Standalone TypeScript and `git diff --check` are targeted non-build checks.

Use the project-pinned/cache-compatible toolchain and lockfile; one portable
Cargo job suffices. Local run used Rust 1.97.1, Bun 1.3.10 and Node 22.16.0. Portable
test `Cargo.lock` is derived from the product lockfile, with no product dependency
change. The feature's final handoff records exact counts and implementation SHA.

Deferred acceptance: both target Macs, actual same-audio RU/EN comparison with
selected Whisper model/tokenizer (hints on/off), meaningful quality measurements,
real signed-in subscription synthetic Q&A/summary spelling and uncertainty,
normal #15 summary integration, production export/desktop ABI and installed UI.
Synthetic fixtures are transport/persistence evidence, not substitutes for these
gates. Coordinator owns independent review and any build/physical allocations.


### Final local verification (2026-11-18)

- Portable SQLite/API/ASR-dispatch + inherited version/speaker tests: **37 passed,
  0 failed, 0 ignored**. Fake model/character tokenizer, not real recognition.
- Isolated production Codex bodies/fake-process suite: **29 passed, 0 failed,
  3 optional actual-CLI localhost tests ignored**. No signed-in service call.
- Frontend: **48/48 isolated test files passed**. Initial 46/47 run exposed an
  incomplete existing settings IPC fixture; the complete new DTO fixes it and
  the final full rerun is green.
- Standalone TypeScript (`--noEmit --incremental false`) and diff whitespace:
  **passed**. Portable dependency lock membership checked against product lock.
- Two additional regressions were observed RED before their fixes: legacy casing
  overriding first explicit approval, and an identical-text later correction
  reusing an earlier confirmation draft. Both pass in the final suites.
- No native desktop/Next production build or real model/audio/service test ran.
  Portable scaffolding emits expected dead-code warnings for unused inherited
  bodies/DTO fields; no test failure is hidden by those warnings.
- The host temporarily stalled lightweight test processes under observed memory
  pressure. No cause/ownership was inferred and no unrelated process was stopped.
  A formatter version probe was stopped after stalling; no automated formatting
  completion is claimed.
