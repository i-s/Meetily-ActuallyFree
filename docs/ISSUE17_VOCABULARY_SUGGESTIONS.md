# Issue #17: local vocabulary suggestions

Live issue read on 2026-11-18:
**[macOS 13/16] Предложения пополнить словарь**,
https://github.com/i-s/Meetily-ActuallyFree/issues/17.
Exact live body and timeline were read from the public GitHub issue page's
embedded issue payload; `totalCommentsCount = 0`, timeline pagination complete.
`gh` was absent and the anonymous REST API was rate-limited (403); no owner
credentials or private data were accessed.

Accepted parent: `ff957416929742730e7d26e4f0de3ee4dfbd59ed` (#16 plus forward
approval migration01601). The assigned checkout was clean at that exact commit.
Planning AGENTS/GLOSSARY, issue-tracker/domain guidance, requirements, spec, ADR
and research were inspected. Prior fork `42b0607` supplied storage, vocabulary
editing and local Whisper consumers; #16 supplied explicit additive confirmation,
first-approved spelling, revision CAS and approval provenance. These are reused,
not a second dictionary. Local non-AI scope was explicitly approved in this task.

## Ordinary entrypoint

Library → saved meeting → **Transcript versions** → **Предложить термины**.
No scan runs when the page opens. A request offers at most five exact spellings
from the selected committed transcript, with line ID, available audio time and
an exact bounded excerpt. RU/EN acronyms (`API`, `СУБД`), internal CamelCase
(`OpenAPI`, `ЁлкаAPI`) and letter+digit tokens (`M4`, `Ёлка2`, `GPT-4`) qualify.
Ordinary title-case names are deliberately not guessed. A source spelling can
still be wrong; the panel says to review it before confirming.

**Запомнить написание** is the only suggestion action which writes. **Отклонить**,
**Закрыть предложения**, scans and errors never write dictionary, approvals,
transcripts, Notes or history. Closing suppresses currently displayed terms, and
individual rejection/acceptance suppresses that case-folded key in memory until
the mounted meeting panel is left. No durable rejection history, automatic scan,
toast, timer, recording hook or notification is introduced. Explicit repeated
scans may show other candidates. A new visit may offer an unconfirmed term again;
there is no continuous stream of notifications.

**Добавить вручную** uses the same `RememberTerm` control/#16 confirmation API,
without falsely claiming a transcript correction happened. It remains available
on missing context, scan failure and empty results. Existing post-correction
Remember flow, independent corrections and Settings vocabulary editor remain.

## Native and UI ownership

- `database/vocabulary_suggestions.rs::context` starts a read transaction, reads
  the selected version/revision, at most **2,001 row identities** (one lookahead),
  and at most **2,000 row texts**. It SQL-substrings each row to the remaining
  **100,000 Unicode scalar values + one lookahead scalar before Rust materialization**.
  The returned text aggregate is at most100,000 scalars. A clipped trailing token
  is omitted, never offered as a prefix. Response fields are only version ID,
  revision, row ID/text/audio start, per-row truncation and overall truncation.
  No `settings_json`, `source_map`, speakers, Notes, contacts, summaries, audio or
  unrelated meeting text is read into the response. Only identity/timing columns
  participate in the bounded initial row query; full row texts are not fetched.
- `api_get_vocabulary_suggestion_context` exposes that read. Missing legacy
  selection errors without calling `ensure` or initializing history. The ordinary
  version browser retains its own existing initialization responsibility. Generic
  #14 `selected`/`get_selected_transcript` and their complete snapshot stay intact.
- `api_get_whisper_vocabulary` supplies existing global and meeting exclusions;
  reading legacy ASR hints does not approve them. No generated suggestion enters
  the canonical store before confirmation.
- `lib/vocabulary-suggestions.ts` independently limits processing to2,000rows,
  **100,000 UTF-16 code units**,100distinct candidates and5displayed suggestions.
  Native scalar and frontend UTF-16 units differ for supplementary characters;
  frontend is a further restriction, not a claim that they are equal. Token and
  excerpt clipping preserve surrogate pairs, and truncated tokens are not guessed.
  Case-folded dedup retains the first source spelling; no spelling normalization,
  identity inference or cross-meeting learning is performed.
- `VocabularySuggestions` is keyed by meeting. Version/revision changes hide
  candidates and invalidate pending reads/completions; no stale result can update
  a different meeting. UI reads are not the final authority for acceptance.
- `api_confirm_vocabulary_suggestion` passes meeting/version/revision/source-row
  proof to `VocabularyRepository::confirm_suggestion`. It uses the **same #16
  `update_global`/BEGIN IMMEDIATE transaction**, checking exact current selection
  and whole exact source token before any dictionary write. This serializes proof
  with competing writers, rather than checking before a second transaction. Source
  proof SQL also fetches only the first100,001 Unicode scalars and shares the
  clipped-token helper (100,000-scalar per-row proof limit), not an unbounded row.
  A stale selection, correction, speaker edit, missing/cross-meeting row, case guess
  or substring fails without changing dictionary or approval revision. Already
  dispatched confirmations retain their original meeting/source identity and may
  complete after navigation if still valid; late completions cannot update the new
  panel. Committed confirmations are not revoked by a later edit/navigation.
- Existing normalization,1000-character capacity, additive write, first-approved
  spelling, approval revision and idempotence are unchanged. Full-store errors
  cannot partially approve. Manual `api_confirm_vocabulary_term` signature is
  unchanged. Migrations01600/01601 are unmodified and must both be integrated.
  No new migration/schema and no new queue, provider or API fallback.

Notes/editor preparation and summary/version selection remain owned by #12/#14.
The suggestion panel never invokes their write/preparation barrier or changes
Notes identity. #15 continues to own scheduling. No #13 Codex process is started;
if AI suggestions are later introduced, they must use the isolated subscription
CLI boundary and agreed shared scheduling rather than this local selector.

## Acceptance criteria and evidence

| Live AC | Implemented behavior / portable evidence |
| --- | --- |
| Concrete spelling, accept/decline in meeting context | Ordinary `TranscriptVersions` component controls show exact source token/excerpt/row/time; UI tests accept or decline from that panel. |
| Confirmation alone changes dictionary; reject/close do not alter user data | Scan/reject/close tests observe zero mutating invocations; native reader tests check unchanged SQLite `total_changes()` for initialized and missing-legacy-selection reads. Only native guarded confirmation changes vocabulary. |
| Confirmed spelling not overwritten, repeated suggestions no duplicates | Existing global/meeting terms excluded case-insensitively; same native #16 transaction preserves first-approved spelling and revision-idempotent duplicate confirmation. Concurrent confirmation and preapproved-case tests execute actual SQLite code. |
| No recording/Notes interruption or repeated-rejection notification flood | User-requested bounded saved-meeting panel, memory suppression, no recording hook, auto-focus, timer or notification. Real Notes/version editor regression suite remains green. Physical responsiveness remains a Mac gate. |
| AI uses agreed text-only CLI; service failure does not block manual entry | No AI/service use at all (conditional AC). No API, paid fallback, credentials or audio access. Empty/error scan preserves ordinary manual confirmation. Real-service behavior is not claimed. |

## Initial portable verification (8c0c1a6)

- `tests/vocabulary`: **52 passed,0failed,0ignored** after bounded-reader work
  (45 inherited +7 new). Includes actual repository and extracted production native
  command bodies. Only Tauri state/transport and ASR engine boundary are stubbed;
  SQLite transactions, migrations01600/01601 and version/revision triggers are real.
  New tests cover no-write/missing selection, exact mixed-language source approval,
  wrong version/revision/row/case/substring, edited source, concurrent duplicate,
  full-store atomic failure, oversized Unicode row + metadata exclusion, >row cap,
  first-approved casing, version-only switch and bounded source-proof suffix rejection. Inherited tests cover disk reopen,
  legacy upgrade/reapproval, canonical-slot invalidation, correction/history,
  source/identity and Notes independence plus Whisper dispatch/token limits.
- Selector: **7passed** including the observed RED→GREEN supplementary-letter
  clipping regression. Pure production selector, no model or transport mock.
- Ordinary entrypoint: **8passed** through real `TranscriptVersions`, suggestion
  panel and `RememberTerm`; Tauri transport and styled Button are mocked. Covers
  scan/decline/close/rescan, exact proof arguments/explicit acceptance, double click,
  existing terms, empty/error manual addition, failure/retry, held reads and late
  completions across meeting/version/revision changes. No Notes barrier is called.
- Inherited isolated vocabulary editor/Remember3 and transcript-version30 tests
  also pass. Nine focused frontend files total **48tests** after Unicode regression.
  Page-editor tests intentionally log simulated conversion/storage errors; those
  are expected failure-path fixtures, not failing tests.
- Scoped TypeScript check of four changed frontend files passes (`incremental:false`);
  no whole-project TypeScript/Next/desktop build. `git diff --check` passes.

Commands use existing local Bun1.3.10, matching frontend dependencies read through
a local ignored symlink, and the warm portable Rust1.97.1 vocabulary target with
`CARGO_BUILD_JOBS=1`, `--offline`. No dependencies installed and no concurrent heavy
build. Fixture-only unused/dead-code warnings are not product failures.

## Remaining gates and honest limits

Not installed, packaged, published or physically tested. M4 Pro/48GB and
M1 Pro/32GB ordinary WebView usability, long-meeting recording/Notes responsiveness,
and usefulness on real RU/EN transcripts remain unqualified. Tests use synthetic,
nonprivate text/SQLite data, not audio/model accuracy or real user data. No real
service/auth/subscription test ran because this feature does not invoke one;
existing #13/#15 real-subscription gates remain independently open. Sparse or
incorrect ASR spellings and text beyond the stated bounds may be omitted; users
must review suggestions and can manually enter any correct name/term. Existing
Whisper224-token prefix and Parakeet-no-hints limits remain #16 behavior.

Author self-review only (no delegates authorized); coordinator acceptance and any
independent review are separate. No Mac operation, full desktop/Next, packaging,
Actions, push, PR, merge, release, or issue closure is included.


## R1: combining marks are token continuations

Independent review of `8c0c1a6761ceac5f6a06f3e2ebbf0b455da961e4` identified a
fragment-approval P2: `е\u0308лкаAPI`, `E\u0308lkaAPI` and
`OpenAPI\u0301Client` were split at their combining marks. Reproduced with the
actual native command and SQLite: the fragment `е` changed approved terms from
`ExistingAPI` to `ExistingAPI\nе`, advancing approval revision1→2. Production
selector tests also reproduced suffix/prefix suggestions. R1 regression runs
failed before implementation (selector5new failures; native3new failures).

Selection and budget lookahead now include all Unicode general-category **M**
marks (Mn/Mc/Me) as continuations alongside L/N and existing `_`/`-` connectors.
A token starting with a mark is consumed and skipped whole in the selector.
Eligible marked spellings retain their exact original code points; no NFC/NFD
normalization or silent spelling substitution is performed. Rust shares the same
L/N/M token regex for bounded SQL-prefix tail removal and whole-source proof;
suggestion-term validation accepts complete marked spellings. Direct requests for
fragments cannot approve either side of a mark, even when the mark lies at or
straddles a budget boundary. `regex` was already a product dependency; only the
portable harness adds that existing dependency/four cache-resolved lock entries.
Unrelated lock entries/versions are preserved and `--offline --locked` passes.

Focused R1 verification: **55native tests passed,0failed,0ignored** (52inherited +
3R1), **54frontend tests passed** in9isolated files (selector12, ordinary flow9,
inherited vocabulary3/version30), scoped four-file TypeScript check and diff check.
Fixtures include both reported scripts, Mn/Mc/Me and a supplementary mark,
mark-before/at/after100,000Unicode-scalar native and100,000UTF-16 frontend limits,
read-only clipped context, direct fragment rejection with exact stored terms and
approval revision unchanged, full exact-source acceptance and an ordinary valid
control. Ordinary `TranscriptVersions` controls transport the full decomposed
spelling. Existing #16 manual commands still allow explicitly confirmed composed
and decomposed entries independently; no manual normalization/dedup semantics,
approval migration or generic transcript/Notes interface changed.

This prevents new fragment suggestions/approvals; it cannot identify or silently
remove fragment spellings already confirmed on an older build. Existing entries
require explicit user review/editing in Personal vocabulary. Conservative term
coverage and the previously documented physical Mac, full-app and real-service
gates remain unchanged. No heavy/Next build, delegate, Mac operation, service/auth,
Actions, push or publication was performed for R1.
