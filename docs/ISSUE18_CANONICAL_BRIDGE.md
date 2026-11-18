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
