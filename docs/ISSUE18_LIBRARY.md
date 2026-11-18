# Issue #18 — local library, search and Markdown

Live issue read on 2026-11-18: **[macOS 14/16] Библиотека, поиск и Markdown**,
https://github.com/i-s/Meetily-ActuallyFree/issues/18. Live comments list was empty.
Accepted software base: `f6b9b2af77aada0543fb0c565034520fba815482`.

## Existing fork and scope

The accepted checkout already contains `/meetings`, `GlobalSearchDialog`, a local
audio player, `ExportMeetingsDialog`, `MeetingExportDialog`, and persistent Notes.
These normal callers are extended; there is no second library, export-only demo,
model invocation, network search or migration/deletion of original meeting data.
The map's historical `/home` description is not this checkout's current route.
Planning inputs were the local project's glossary, accepted ADR, requirements,
macOS assistant specification, agent conventions and supporting research. Notes
mean the user's own thoughts, not recognized speech or a participant's agreement.
Issue #19 deletion is unchanged and out of scope. No provider/AI/Q&A implementation
was imported from another branch.

## Normal entrypoints and owners

- Sidebar **All meetings** → `/meetings`: local meeting list including empty,
  audio-only, Notes-only, and no-summary meetings. Each existing row shows
  transcript count, Notes/summary presence and summary state, and observed audio
  availability. Failed refreshes keep the last list and display an error/retry,
  rather than manufacture an empty library. Material observations refresh on
  entry, workspace Notes/meeting changes, window focus and explicit Retry.
- The library search field now searches saved transcript and Notes text, in
  addition to title/group filtering, with source-labelled snippets. The shared
  command bar (**Ctrl/Cmd+K**, sidebar search) adds **Your Notes — not recognized
  speech** and uses the same native text-search repository. Search hits only
  navigate: no current/live Notes, transcript or chat-context mutator is called.
- `library.rs` owns local text search, material reads and export capture;
  `library_commands.rs` exposes `api_library_materials`, `api_search_library`,
  and `api_export_meeting_snapshot`. The existing `PeopleRepository::global_search`
  retains people/group/action search and ranking, delegating its meeting/text
  kinds to the library repository.
- `useLibrary.ts` owns debounced query generations and material observations.
  Changing/clearing a query removes old hits immediately; unmount/late replies
  cannot publish old results. Errors are visible and retryable. Global search
  similarly clears old hits before loading a new query. Deep links preserve exact
  nonnegative fractional audio seconds, transcript row ID and `section=notes`.
- Meeting details still uses the normal audio player and selected transcript.
  Notes deep links choose the Notes/document tab and focus **Your notes — not
  recognized speech**. Audio absence is explicit and does not disable saved text.
  Page content is keyed by meeting ID, never a transcript version. A late search
  row no longer present in the selected transcript is reported instead of
  pretending its old anchor exists.

## Search behavior and bounds

SQLite's built-in `lower()`/`LIKE` are ASCII-only. Text search uses Rust Unicode
lowercasing on visible saved title, transcript and Notes text (RU/EN mixed case),
with literal matching for `%`, `_`, backslash and quotes. It does not search rich
editor IDs, caches, links or hidden summary JSON. Legacy Notes with rich JSON but
no Markdown use visible rich content, including table cells; no row is rewritten.
Summary search retains the existing legacy/Markdown/BlockNote visibility parser.

A SQLite read transaction pins each search. Reads use 256-row pages and retain a
bounded result set, rather than load all meeting text into the WebView. Library
search returns up to 200 distinct meetings and explicitly asks to refine the query
at that boundary; global search retains its existing 40-result normal caller
limit (maximum native global limit 100). This is bounded *result memory*, not a
search index or constant-time latency promise: a rare match may scan the entire
local library, and SQL offset pages/sorting can cost more for very large libraries.
The library's local title/group matches can also appear outside its text-hit cap.

Normal transcript deep-link pagination no longer stops after 40 pages. The
synchronous in-flight owner serializes offsets; the old 100ms debounce stranded
fast sequential searches after page two. Generation-checked completion prevents
an old meeting's load from resetting another meeting's pagination state.

## Export consistency and source labels

Both **library row/bulk Export** and **meeting details Export** use
`captureMeetingExport` → `api_export_meeting_snapshot` → `renderMeetingMarkdown`.
Detail **Complete meeting** is available even with no transcript/summary; it
includes Notes, transcript and available summary. The library defaults to
Markdown with all materials checked. Notes-only and selected material exports
remain available; other existing formats reuse the same captured content.

1. `notesForContext(meeting:<id>)` flushes/revalidates the production Notes store.
   Pending conversion/recovery, failed persistence or revision conflict blocks
   export visibly; it cannot silently substitute an older Notes conversion.
2. A single native SQLite read transaction captures current title/date, committed
   Notes (+ revision and exact rich JSON), canonical selected transcript rows,
   current summary/status and saved action items. Expected Notes revision is
   checked. Missing meeting/read/parse failures propagate, not empty success.
3. A second Notes flush/read rejects an edit or cross-WebView change during
   capture. The returned native snapshot is detached. Per-meeting capture time
   and Notes revision are written to the document. Batch meetings have separate
   capture times, not a falsely claimed cross-meeting atomic snapshot.
4. Subsequent edits/selection after capture are not included: this is an explicit
   **saved snapshot**, not a live export. A summary edit still pending in its
   separate editor is not claimed to be saved. Audio availability is a filesystem
   observation made around capture time, not part of SQLite or a retention lease.
5. React meeting/dialog generation guards cancel abandoned exports before opening
   the picker and again before writing its destination. Native writes already
   submitted cannot be cancelled; a late navigation cannot roll back completed OS
   I/O. Picker cancellation returns false and failed writes never toast success.

Markdown has separate `spoken_transcript`, `user_notes`, `meeting_summary` and
optional saved-action sections. Notes explicitly say they are user-authored, not
recognized speech or agreed conclusions. Missing audio, transcript, Notes and
summary are explicit. Transcript speakers/times/text are retained without
identity inference or filtering through the loaded UI pages. Source material is
inside length-safe text fences: Markdown-looking participant/user text cannot
forge another source section. Notes/summary Markdown source is preserved verbatim
inside these fences rather than rendered as export headings. Legacy rich-only
Notes additionally retain the original blocks as JSON, with readable table/text
conversion; original SQLite records are not normalized or overwritten.

The OS save picker supplies the destination and its overwrite confirmation/fs
grant. The exporter never appends an extension *after* selection: that would write
a different, unconfirmed path. Only the suggested basename is sanitized (Unicode
RU/EN retained, separators/control characters removed, reserved device names
escaped); the exact selected output path is honored. Audio paths are not embedded
or copied, and exported text continues to work after audio removal.

## Read-only #14 compatibility

No #14 branch merge was performed. Accepted standalone source
`c3d7f8a7516c16d6ea63f686a2352a078c6b1274` and combined #12/#14 source
`b231d184d03b914348b9ca1eeb4d71ded433d55a` were inspected with `git show`.
Their contract keeps `transcripts` as the selected editable projection; candidates
and archived summaries are separate. Selection/correction writes and summary
archiving/clearing are atomic. Export and search deliberately read canonical rows,
not candidate JSON or old summary backups. No call to a model or version mutator
is needed for library reads.

`test-library-portable.py` optionally takes `MEETILY_LIBRARY_VERSIONS_REF=<SHA>`.
It reads the actual version repository, identity helper and migrations into a
throwaway fixture (only include-file paths are relocated); nothing is merged.
The compatibility test calls actual selected/begin/finish/set_status/select and
correct_text functions with synthetic text. It proves unselected candidates stay
out of search/export, actual selection/correction changes both consumers, archived
summaries stay absent and Notes/revision are retained. Combined UI integration must
preserve the meeting-ID key, Notes editor ownership and projection refresh from
both branches; conflict resolution still requires the coordinator's review.

## Verification evidence and limits

Executed on Linux with synthetic text/temporary SQLite and generated non-audio
marker bytes only; no private recordings or databases were used.

- Red/green frontend repros: stale prop title/omitted Notes-only materials,
  swallowed read errors, unconfirmed appended destination, legacy summary format,
  and pagination stranded at 200 of 4,501 rows. Each failed before its fix.
- `tests/library/`: production shared renderer/capture, actual detail hook/dialog
  with real Notes store, actual batch export dialog/picker pipeline, actual library
  page/search hook, global search dialog, and production pagination hook. Boundary
  adapters (Tauri, picker/fs, browser/BlockNote view and unrelated UI contexts) are
  mocked, not claimed as real desktop IPC. Covers cancelled navigation/picker,
  failed writes, late replies, Notes edit during capture, Unicode labels, legacy
  summaries/rich Notes, source-text fence safety and 46-page navigation.
- Initial full isolated frontend runner: **45/45 files passed**. Two further
  renderer assertions were added and their file reran **6/6 passed**. Final full
  run/build evidence will be appended below.
- Standalone TypeScript `--noEmit`: **passed** before production build.
- Portable native library commands/repository/global-search body: **6 tests**,
  real migrations/SQLite; combined compatibility adds one test and runs #14's
  repository suite: **32 passed, 0 failed, 0 ignored**. The standalone accepted
  #14 compatibility test separately passed. WAL test pins coherent old/new title,
  text and summary across an uncommitted/committed projection transaction; it is
  not a physical recording test. Large export fixture has 1,205 rows, late search
  fixture 770 rows; audio removal only removes a synthetic marker file.
- Native fixture uses offline/locked root dependency versions, one Cargo job and
  existing coordinator-approved cache. Its initial bootstrap errors (yanked spin
  during generate-lock and cross-target metadata download) were fixed by retaining
  the root lock and Linux-filtered metadata; no dependency substitution/network
  download/cache cleanup occurred. Tauri transport alone is adapted in commands.
- One global-search Bun run stalled before fixture execution in the runtime
  transpiler cache; it was stopped. With `BUN_RUNTIME_TRANSPILER_CACHE_PATH=0`, the
  isolated test and subsequent full runner passed; no cache was removed.

Production frontend export, full desktop native build, real Tauri picker/asset
protocol/playback, installed app, physical M4/M1 Mac and release are separate
facts. No DMG, Actions, push, PR, package, release or Mac operation is part of #18.
No claim of acoustic/ASR quality or target-Mac acceptance follows from these tests.

## R1 — search the visible Notes representation

Independent review of `ef1678051de86104297eb703ad670767d55a25e0` found that
`notes_text` preferred raw Markdown and joined rich inline leaves with invented
spaces. The normal library/global search therefore missed `план api` in
`План **API**: _обсудить_`, matched a hidden Markdown link destination, and missed
`api` when adjacent styled runs displayed `AP` + `I`. These were reproduced red
through the production SQL search path before the fix.

`notes_text` now prefers a structurally valid, nonempty rich block array, joining
inline text/link runs without adding characters. Paragraphs, list items, nested
blocks and table rows have line boundaries; cells have tab boundaries. Editor
IDs, link destinations, styles and other hidden properties do not enter text.
A valid empty paragraph remains empty; a missing/empty block array or malformed
rich data uses the legacy Markdown path, matching the ordinary editor's
Markdown-only fallback. Matching and snippet extraction receive this same visible
string; snippets do not use offsets into raw Markdown/JSON.

No native Markdown parser was present in the accepted dependency graph. Legacy
Markdown uses pinned `pulldown-cmark = 0.13.6`, with default features disabled,
CommonMark plus tables/strikethrough/task lists. Its MIT license and Rust 1.71.1
minimum were checked (the workspace minimum is 1.77.2). Parser events retain
visible link labels, code/literal punctuation and decoded escapes/entities,
exclude hidden link/reference destinations/titles and code-language metadata,
and preserve block/cell boundaries. It does not render HTML, execute code, open
links or perform network requests. This is a read-only searchable projection:
**Markdown export bytes/source distinctions and original saved Notes are unchanged.**

The lock adds only `pulldown-cmark`; all prior package records and dependency edges
are unchanged except the new `meetily` dependency. The first Cargo update proposed
unrelated graph rewrites, which were not retained. The original graph plus the
Cargo-resolved parser package was validated by `cargo tree --offline --locked -p
meetily --target x86_64-unknown-linux-gnu --depth 1` and the fixture's locked version
assertions. No whole-workspace/native desktop build was started.

### R1 final verification

- Production `api_search_library` **and actual `api_global_search` command body**
  (only Tauri State transport adapted), real SQLite/migrations: **14 passed**.
  Cases include all review examples, rich preference over stale Markdown,
  adjacent inline runs, links/references/titles, literals/code/escapes/entities,
  rich and Markdown tables/lists, malformed/empty-block fallback, exact full
  short snippets and an exact long 220-character visible snippet window.
- Combined read-only `b231d184d03b914348b9ca1eeb4d71ded433d55a` compatibility plus
  the version repository suite: **40 passed, 0 failed, 0 ignored**. This includes
  actual selection/correction and the unchanged source-labelled export snapshot
  tests. The cached, single-job command waited for Cargo's package cache lock and
  compiled only the fixture binary; it finished in 17m35s wall time under load,
  then tests took 10.98s. No lock/cache cleanup was used.
- Default full isolated frontend runner: **44/45 files passed**; the first
  `tests/library/batch-dialog.test.tsx` test exceeded its default 5s deadline under
  load (the file executed all 13 assertions). Its isolated `--timeout 60000` rerun
  passed both tests. A fresh complete rerun of the identical 45-file set, still
  one Bun process per file with a bounded 60s per-test limit, passed **45/45 files,
  206 tests**. No frontend product or export-test behavior was changed for R1.
- `git diff ef167805 --` the renderer, exporter, detail hook and export dialogs is
  empty. R1 changes searchable text, its parser dependency, native tests/fixture
  and this documentation only.

### Production build evidence remains scoped to its actual source

The reserved bounded Next attempt ran from the real `frontend/` directory at
clean commit **`ef1678051de86104297eb703ad670767d55a25e0`**, tree
**`4cf01139bf95f92a22098d56e9bace3a811cf299`**, with Node 22.16, two CPU/one worker
settings, a 4GB heap cap and a 900-second timeout. It logged **Compiled successfully**
then **Linting and checking validity of types ...**, and exited **124** at its
limit. This is optimized frontend compilation evidence, **not a completed
production build/static export**. Log: `/tmp/issue18-next-build.log` (local only).
Standalone TypeScript passed during implementation before that build; no
concurrent standalone checking was started during it. R1 has no frontend source
changes, but no fresh production export or full desktop-native build is claimed.

The build slot was released, and an exact-cwd `/proc` check found no surviving
Next/worker process. An immediate coordinator notification after the background
exit failed with `parent_not_active`; it was relayed successfully when the R1 turn
became active. No second competing export, DMG, Actions, push or Mac operation was
started. Full production export and real installed macOS UI/picker/audio playback
remain qualification limits, not silently promoted acceptance passes.


## Authorized combined integration qualification — 2026-11-18

The local #18 head `2b7dbca968aed6feb479fc927568d52178ba715c` is merged
with accepted Notes/version head `226ff989997e49a404ab8b684e6168f453f31c4f`
using a two-parent feature merge. No default branch or remote is changed.
The three textual conflicts retain both command/repository modules and combine
the Notes focus/deep-link section with the version preparation barrier. Notes
remain keyed by meeting, not selected transcript; conversion, revision CAS and
summary flush must finish before selection/correction, and editing is disabled
only while that operation owns the barrier.

The actual combined caller test exposed a pre-existing sibling-key collision:
`TranscriptVersions` and `MeetingExportDialog` both used the bare meeting ID.
A focused assertion failed with React's duplicate-key warning before the selector
key was namespaced to `versions:<meeting>`. It now passes without the warning;
this does not change Notes editor ownership or transcript revision identity.

### Fresh verification on the combined sources

- `tests/library/combined-entrypoints.test.tsx`: **5 passed, 81 assertions**.
  Actual detail page, version selection/correction, rich Notes editor/store,
  library page/search hook, detail and batch export callers are exercised together.
  Immediate formatted RU/EN edits wait for conversion and persist exact rich JSON
  and Markdown at the expected revision; selection/correction refreshes the
  chosen speech projection without remounting Notes. Archived/dirty summaries
  disappear from search and both Markdown exports. Competing Notes writers block
  mutation/export until the normal explicit reviewed replacement; picker write
  failures surface, cancellation produces no success/write, and navigation while
  a picker waits cannot write the abandoned snapshot. Transport, BlockNote engine,
  picker/filesystem and unrelated chrome remain synthetic boundary adapters.
- All frontend files ran in **separate Bun 1.3.10 processes**, with transpiler
  cache disabled and a 60,000ms per-test limit: **51/51 files, 238 tests passed**.
  This includes #18 4,501-row deep-link pagination and #14 action time navigation.
  Log: `/tmp/issue18-final-combined-frontend.log`.
- Portable library fixture now defaults to the current merged version repository,
  while preserving explicit read-only compatibility-ref support. Real production
  search/export/Notes repository bodies, all checked-in SQL migrations and real
  SQLite: **44 passed, 0 failed, 0 ignored**, offline/locked with one Cargo job.
  The selected-version case writes formatted Notes through native revision CAS,
  selects/corrects real snapshots, verifies exact Notes JSON/Markdown/revision,
  rejects a stale export revision, and searches joined adjacent rich runs.
  Log: `/tmp/issue18-final-combined-library-sql.log`.
- Portable recovery fixture: **83 passed, 0 failed, 0 ignored**, real SQLite and
  cached FFmpeg with synthetic fixtures, offline and one Cargo job. Covers all
  Notes/version attachment, conflict, migration-zero, idempotent retry and disk
  reopen cases. Log: `/tmp/issue18-final-combined-recovery.log`.
- Standalone TypeScript `--noEmit`: **passed** before the production build.
  Log: `/tmp/issue18-final-combined-tsc.log`.
- #14 `transcript_versions.rs` and `ActionItemsList.tsx` are unchanged from the
  accepted incoming head: original action anchors and audio-time fallback remain.
  #18 pagination/generation guards, source distinctions and deep links remain.

Initial harness errors (the menu is `Export…`, not `Export`) and a new native test
assertion that treated export Markdown's String as Option were corrected before
these final green runs; neither required a product behavior workaround.

One sequential bounded final production frontend export is authorized after this
merge commit. Its exact SHA, phase, log and exit status will be reported separately;
no success is implied here. No native build/test embedding `frontend/out` or
standalone TypeScript check runs concurrently with it. Local package version
remains `0.2.18`; `gh release list` returned an empty list on 2026-11-18. This is
not a published release, installed app or macOS hardware qualification. No DMG,
Actions, push, PR, #19 deletion, agents, private data or Mac operations were used.

## Combined R1 — action extraction ownership (2026-11-18)

The reviewed parent is `fc09643030a91cbc76eba1680ee7913cdfb156a5`.
`MeetingDocument` previously invoked the real `syncMeetingActionItems` helper and
installed its completion without checking summary/projection/navigation changes.
The helper first reconciled tasks, then independently saved the trimmed summary
by meeting ID. A delayed completion could therefore resurrect archived text or
replace a newer summary-process row; a JS cancellation check alone could not
protect native task/anchor or summary writes.

### Native ownership and atomic data flow

The additive `database/repositories/action_sync.rs` commands are:

- `api_action_sync_context({ meetingId })`: under the native write lock, capture a
  terminal committed summary, its opaque ownership token, selected version ID and
  revision, and the actual current transcript rows. Legacy selection initialization
  uses the existing #14 owner. Pending jobs or missing summaries return `null`.
- `api_commit_action_sync({ context, items, sourceFingerprint, summary })`: under
  one native write transaction, compare meeting, selected version/revision, summary
  token and complete canonical summary content/status. Stale context returns
  `null` without changing tasks, anchors, source marker or summary. Validate every
  proposed anchor/time against this meeting's selected canonical rows. Reconcile
  tasks with the existing curated/done-item preservation logic, trim the summary
  and update its timestamp atomically; return task rows from that transaction.
  A failed trim rolls back the preceding task/marker reconciliation as well.

Migration **20261118001800_action_sync_ownership.sql** adds
`summary_processes.action_sync_token`, initializes existing rows without altering
summary content/timestamps, and rotates the token on every INSERT and UPDATE.
The UPDATE trigger is recursion-safe when only the token changes. Ownership is
not a timestamp, fingerprint or meeting PK: existing-row UPSERT reset, completion,
committed edits and delete/reinsert all invalidate old extraction, even if the
same transcript revision and byte-identical result are restored later.

**Writer obligations (#15 shared contract):** leave `action_sync_token` to SQLite;
do not copy it into reset/UPSERT/completion payloads. Existing summary SQL writers
need no extra parameter, callback or JS promise. Automated extraction must use the
checked context/atomic command rather than separate generic task and summary
writes. Existing #14 selection/correction signatures are unchanged.

### Real helper, backfill and UI

`action-item-sync.ts` first compares the caller's visible summary projection with
that of the captured native document, allowing normal loaders to omit native
metadata. Drafts, original Markdown and anchors are then derived only from that
captured document/transcript, never the supplied paginated UI or old backfill
transcript. The transaction still compares the full canonical native content and
ownership, not merely the visible projection. A stale caller or native no-op emits
no success event and has no meeting-only fallback write.

Both section extraction and no-section backfill use this protocol. No-section
backfill is **marker only**: reading an already-trimmed summary must not erase the
tasks it previously produced. Recognized sections retain the existing extraction,
deduplication and curated/done-task preservation behavior.

`MeetingDocument` tracks extraction promises by owning meeting. The existing
version preparation barrier drains rich Notes/summary conversion and autosave,
then pending extraction before selection/correction can archive the summary.
Another window/job is protected independently by native ownership checks. After
navigation, a previous meeting's pending reply cannot block the new meeting's
barrier. UI completions are cancelled on summary/projection/navigation changes;
a synchronous optional `NotesEditor.onEdit` epoch also protects rich summary
drafts whose Markdown conversion has not finished. Late task snapshots are not
installed: current tasks are reread with request/meeting guards so later curated
edits and anchors are retained. User Notes ownership/content are unchanged.

### Executed reproductions and limits

- Replaced the combined caller fixture's `action-item-sync` mock with the real
  helper. The refined focused run temporarily restored the exact parent helper
  and document implementations: **0 passed, 3 failed**. With accurate native
  UPDATE-only save semantics, the absent row stayed absent but the mounted UI
  resurrected old summary text after both selection and correction. An existing
  newer same-version summary row was overwritten in the third case. The actual
  page invalidation callback ran without remounting the document. Restoring the
  fix made all 16 combined caller tests pass. Log of exact-parent red:
  `/tmp/issue18-r1-action-real-path-red.log`.
- Focused insert-only ownership mutation: same-row identical regeneration test
  **failed**, proving the UPDATE trigger is required. Restoring it passed.
- Removing the UI completion guard: pending rich draft test **failed** (summary
  editor remounted from engine 2 to 8). Restoring it preserved the exact editor.
- Delayed task snapshot regression failed before current-task rereading, then
  passed with current edited text/anchor retained. Navigation/barrier regression
  failed before per-meeting promise ownership, then passed without waiting for
  the departed meeting's reply.
- Real combined caller group now covers both pre-commit and post-commit delayed
  replies, selection/correction, ordinary success, current UI/native/export
  agreement, newer summary rows/drafts, task edits and navigation. Its final run
  and the full isolated frontend/native/TypeScript results are reported with the
  scoped commit. Boundary adapters still stand in for Tauri/BlockNote/picker/fs;
  this is not a real WebView/desktop IPC execution.
- Five new native cases execute real SQLite interleavings using production
  guarded context/commit, task repositories and version selection/correction:
  stale selection/correction; same-row UPSERT pending→completed identical bytes,
  delete/reinsert, edits/new content at unchanged transcript revision; ordinary
  success/curated tasks/Notes/anchors and marker-only reads; bad anchors and forced
  trim rollback; transcript-only revision changes with unchanged summary token.
  All migrations and the native export repository participate. Summary-job writes
  in the interleaving fixture use the production SQL shape, not a running model
  or background summary engine.

Only warmed, device-free tests and standalone TypeScript checking qualify this
correction. No new production export, native desktop build, agent, push, PR,
Actions, DMG, release, Mac or private-data operation is part of R1. The earlier
combined production export timeout is not upgraded to a pass by these tests.


Final R1 lightweight verification:

| Verification | Result | Local log |
| --- | --- | --- |
| Actual combined detail/version/Notes/action-sync/library callers | 16 passed, 139 assertions | `/tmp/issue18-r1-action-ui-green.log` |
| Real action helper / canonical context / anchors / backfill | 6 passed, 23 assertions | `/tmp/issue18-r1-action-helper.log` |
| Entire isolated frontend suite, Bun 1.3.10, cache disabled, 60s per-test | 52/52 files, 255 passed, 0 failed | `/tmp/issue18-r1-action-frontend-final.log` |
| Production SQLite library/action/version suites, offline/locked, one warmed Cargo job | 50 passed, 0 failed, 0 ignored | `/tmp/issue18-r1-action-native-all.log` |
| Recovery/Notes/version/all migrations, cached real FFmpeg and synthetic fixtures | 83 passed, 0 failed, 0 ignored | `/tmp/issue18-r1-action-recovery.log` |
| Standalone TypeScript `--noEmit --project frontend/tsconfig.json` | exit 0 | `/tmp/issue18-r1-action-tsc-final.log` |

The legacy generic task-only API remains signature-compatible for compatibility;
normal extraction/backfill no longer uses it, and the frontend export is marked
deprecated for summary extraction. No automatic summary writer should chain that
API with a meeting-only `api_save_meeting_summary` call.
