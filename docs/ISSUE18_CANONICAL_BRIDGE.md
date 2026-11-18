# Canonical post-call result bridge (#15 + #18 convergence)

## Authorized design

Merge accepted `d7ed5d5fe9168051021c9fea1ce511c181275a36` into accepted
`79c0c1107bbfb8d62d2a4ddbd4daedd137332cbb`, preserving both histories.
One native bridge projects a validated immutable post-call artifact into existing
summary/task tables. Normal detail, library search and both Markdown exports keep
using those canonical tables; no competing frontend summary store is introduced.

A queue claim captures the current summary ownership token (including absence).
Publication atomically checks the job epoch, selected transcript version/revision,
Notes revision and captured summary ownership. A manually edited summary or an
independently renewed/replaced row cannot be overwritten. Immutable artifact
publication still succeeds when canonical projection is blocked by ownership;
history remains readable. An exact retry never rewrites an already projected
artifact or the user's later edits. Claims/tasks are derived from validated
artifact evidence, not stale UI text; curated/done tasks survive reconciliation.
Provenance records artifact tuple, job epoch, original JSON and resulting summary
token. Recovery can project a completed pre-bridge artifact only into an absent
canonical summary; existing legacy/manual summaries are not assumed disposable.

The bridge runs inside artifact publication, not a WebView-owned promise. Native
read-side reconciliation supports retained completed artifacts after upgrades.
The normal detail editor refresh observes canonical updates without replacing a
newer dirty summary draft. Generation/navigation guards reject late reads. Notes,
recording priority, genuine client gates, candidate selection and spelling
suggestions retain their accepted owners.

## Plan

1. Add failing SQLite bridge tests through production queue publication and normal
   library/export repositories; add normal frontend caller coverage.
2. Implement unique #18 bridge migration, native ownership capture/projection,
   retained-artifact reconciliation and provenance. Preserve existing #18 action
   transaction guards and same-row ownership renewal.
3. Wire normal detail/library refresh to canonical changes with dirty-draft and
   stale-result guards; verify both exports and task anchors.
4. Run warmed isolated frontend/native tests and types. Inspect the full merge.
   Commit with two parents and required attribution.
5. Check resources/prerequisites; run sequential production Next export on the
   committed combined SHA, verify all static output, then full native check with
   available prerequisites. Do not disable checks or change the system. Diagnose
   recoverable failures; report actual blockers and exact logs/SHA.
6. Return completion/blocker to coordinator; no delegates/push/PR/packaging/CI/Mac
   or private/user-data operations.

## Implemented flow and qualification

`post_call/worker.rs::snapshot` captures a once-per-epoch summary owner before
inference. `store::publish` retains the original immutable bytes on tuple conflict,
then `store::projection::apply` performs canonical materialization inside the same
transaction. The bridge checks captured ownership, job epoch/status, exact selected
version/revision, Notes revision and vocabulary identity. Every summary write still
rotates #18's action ownership token. A blocked canonical projection retains the
completed immutable artifact for history; it does not weaken native client gates.

Projection evidence is revalidated against actual current speech/Notes. Notes
cannot establish decisions, tasks, owners or deadlines. Tasks use exact speech row
IDs/times, retain original version sources, and remain separate canonical rows.
Curated/done tasks are preserved. Normal summary Markdown includes literalized
supported claims/evidence and uncertainty, not another auto-extractable task
section. Provenance contains the immutable source JSON and tuple/job epoch; both
Markdown exports identify that provenance, and complete detail export now includes
tasks just like the library export.

Retained completed artifacts can be reconciled after upgrade/restart or on normal
summary/library/export reads. Recovery is absent-row-only and checks the same exact
current tuple. Durable applied receipts prevent reapplication after version changes
or user edits/deletion. Unbound legacy vocabulary artifacts remain history only.
Existing legacy summary rows are not assumed replaceable.

`current_meeting_summaries` hides stale untouched generated rows after selected
speech or Notes changes, without physically deleting rows needed by pending manual
saves or #14's archive-before-clear. Manual rows remain user-owned. Normal summary
reads no longer require a `transcript_chunks` row; fresh #15 results do not have that
legacy side table. The #18 action transaction also reads the current-summary view,
so an old generated summary cannot be blessed with a new transcript context.

Normal detail refresh listens for native queue changes/focus and reads the canonical
summary command. It rejects late navigation/view/generation replies and immediate
rich summary drafts before asynchronous Markdown conversion. It does not remount
Notes. Library material/search hooks observe the same durable publication event.
Existing genuine client qualification, capture-priority policy, explicit candidate
selection, Notes CAS and suggestions/approval interfaces are unchanged.

### Light evidence before committed heavy verification

- Original production publication regression failed with `canonical summary missing`
  in normal export (`/tmp/issue18-bridge-red.log`), then passed with the bridge.
- Real SQLite library/action/version/bridge suite: **59 passed, 0 failed, 0 ignored**.
  Includes fresh/retained artifacts, same-row identical renewal, delete/reinsert,
  manual summary/task edits, task anchors, Notes changes, selected text correction,
  original-summary archival, exact retries, stale raw edits, original immutable
  bytes, forced task-write rollback of job/artifact/summary/tasks and retry.
  `/tmp/issue18-convergence-library-native.log`.
- Production queue/runner/priority tests: **28 passed**;
  `/tmp/issue18-bridge-queue.log`.
- Actual production worker/native scheduler + fake isolated CLI boundary:
  **4 passed**, including normal canonical-command visibility after completion;
  `/tmp/issue18-convergence-worker-final.log`.
- Real SQLite recovery/Notes/version/all merged migrations with cached real FFmpeg:
  **85 passed**; `/tmp/issue18-convergence-recovery.log`.
- Actual combined detail/version/rich Notes/action/library/both exports callers:
  **18 passed**, including native publication refresh, tasks/provenance in both
  exports, pending rich draft and navigation protection. Frontend/native transport,
  BlockNote engine and filesystem/picker remain synthetic boundaries.
- Full isolated combined frontend suite: **71/71 files, 373 passed**;
  `/tmp/issue18-convergence-frontend-final.log`. An initial 70/71 run found an event
  boundary mock missing `transformCallback`; the mock was corrected, not the
  production path. Standalone TypeScript exited **0** before heavy verification;
  `/tmp/issue18-convergence-tsc.log`.

Heavy verification will be run sequentially from the committed combined tree:
Next production build/export (checks enabled), all-route/static-payload verification,
then full desktop native check with available prerequisites. No heavy result is
implied by the light tests. No installer/package, CI, push/PR, other agent, Mac or
private/user-data operation is authorized or performed.

## Canonical bridge R1 — three P2 corrections (2026-11-19)

Exact reviewed parent: `e32eacb371920bdd50951f03477d7add9970e7e0`.

1. **Unicode task identity:** projection previously compared SQLite's ASCII-only
   `lower(trim(text))` against Rust Unicode-lowercase proposals. Projection now
   uses the ordinary task reconciliation normalization on both stored rows and
   proposals (`action_item::normalize_task_text`, the existing shared trim/lower
   helper). A retained completed/edited «Отправить отчёт» keeps its original ID,
   exact curated text, done/edited state, anchor and original-version source.
   Distinct supported Unicode tasks still insert normally; vocabulary refresh
   retains both immutable artifact revisions.
2. **Meeting owner mapping:** projection now calls the existing transaction-owned
   `action_item::resolve_owner` rather than its own global-only query. The ordinary
   resolver first respects this meeting's speaker/contact association (Alice →
   Alice Smith), then uses the genuine existing global match. Unknown names remain
   labels without a contact; source labels such as `You` are not identities even
   if an artificial global contact has that name. No identity is created. Portable
   queue/worker/recovery fixtures include the unchanged production resolver body
   and real shared normalization so these are not replacement mocks.
3. **Still-focused Notes commits:** `NotesSnapshot.committedRevision` exposes only
   a native read or successful save acknowledgement, never pending draft/conversion
   revisions. The detail subscribes to its own `meeting:<id>` snapshot and rechecks
   the native canonical view when that committed revision changes. After the await,
   it validates the same committed revision along with meeting/view/request/draft
   guards before applying a reply. Pending/failed Notes writes and another target's
   commits do not clear the summary. Dirty rich and saved manual summary ownership
   remains protected. Subscription cleanup retains navigation isolation; no focus
   change, queue event or editor remount is needed to invalidate old generated text.

### Executed red → green evidence

On the exact parent implementation, two new ordinary-publication SQLite tests
failed: Cyrillic republication produced **3 instead of 2 tasks** (a spurious unchecked
copy), and owner resolution chose **global-alice instead of meeting-alice**. One
mounted real detail/editor/Notes-store test failed: authoritative canonical read and
native export both excluded the old generated summary after a Notes save, while
its editor remained mounted (**1 instead of 0**). These red runs are recorded in
`/tmp/issue18-bridge-r1-native-red.log` and `/tmp/issue18-bridge-r1-ui-red.log`.
All three regressions now pass. Controls cover completed vs edited uppercase task
text, a distinct Unicode task, genuine global fallback, unknown/source labels with
no new identities, stable task/anchor/source rows, unchanged immutable artifacts,
pending/failed Notes writes, unrelated targets, manual/dirty summaries and delayed
canonical replies. Normal detail and library Markdown exports agree with the
canonical absence after the successful Notes commit.

Fresh lightweight verification on the corrected sources:

| Check | Result | Local log |
| --- | --- | --- |
| Ordinary publication / library / actions / versions / bridge SQLite | 61 passed, 0 failed, 0 ignored | `/tmp/issue18-bridge-r1-native-all.log` |
| Mounted combined normal callers | 23 passed | `/tmp/issue18-bridge-r1-ui-all.log` |
| Notes store native-ack revision tests | 12 passed | `/tmp/issue18-bridge-r1-notes.log` |
| Full isolated frontend, Bun 1.3.10, transpiler cache disabled, 60s test limit | 71/71 files, 379 passed | `/tmp/issue18-bridge-r1-frontend-final.log` |
| Warm portable queue / priority / publication | 28 passed | `/tmp/issue18-bridge-r1-queue.log` |
| Warm production worker with synthetic transport/model and fake CLI | 4 passed | `/tmp/issue18-bridge-r1-worker.log` |
| Warm SQLite / cached FFmpeg recovery fixture | 85 passed | `/tmp/issue18-bridge-r1-recovery.log` |
| Standalone TypeScript `--noEmit` | exit 0 | `/tmp/issue18-bridge-r1-tsc.log` |

No migration, evidence rule, immutable-artifact publication or native transaction
boundary changes are part of R1. Genuine-client gates and capture policy are
unchanged. Verification uses real production Rust/SQLite and real frontend callers
with previously documented transport/BlockNote/picker/filesystem adapters; it does
not establish actual Tauri IPC, model/acoustic/device/Mac qualification. Author
self-review only; no delegates. No heavy export or full desktop-native check was
repeated: the successful production export/static verification applies **only to
parent e32eacb**, and its full desktop-native prerequisite blocker remains separate.
No packaging, Actions, push/PR, system changes, Mac or private-data operation.
