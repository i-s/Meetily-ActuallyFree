# Issue #19 — explicit audio-only cleanup

Live issue read 2026-11-18: **[macOS 15/16] Очистка накопленного аудио**,
https://github.com/i-s/Meetily-ActuallyFree/issues/19. Exact title/body read through
GitHub REST; comments list empty. Planning AGENTS, glossary, requirements/spec,
research and accepted ADR were read. Work starts at accepted #18
`79c0c1107bbfb8d62d2a4ddbd4daedd137332cbb`. Accepted #15
`d7ed5d5fe9168051021c9fea1ce511c181275a36` queue/storage paths were inspected
read-only. #18 owns merging its accepted histories and the canonical summary
bridge; this change does not recreate that merge.

## Normal path and consent

**All meetings → Audio storage → Review audio cleanup → select unchecked
recordings → Review selected audio → Delete listed audio.** The normal native
commands are `api_audio_storage`, `api_prepare_audio_cleanup` and
`api_confirm_audio_cleanup`, registered in `lib.rs`.

The visible total is observed logical audio bytes in the database's saved
recording folders, including protected `.checkpoints` audio/pending files. It is
not a full-disk scan, allocated blocks or a promised amount of free space.
Canonical folder aliases and registered checkpoint subfolders are counted once. Inaccessible/linked folders make the
measurement explicitly incomplete, not silently zero/complete. Recovery traversal
is read-only and bounded to its root and one source-directory level. Unknown
nested recovery layouts fail closed. Orphan folders not associated with any
library meeting are outside this inventory and are not deleted.

The app suggests reviewing recordings **older than 30 days**. This is an explicitly
labelled application convenience heuristic, **not a user-agreed storage limit**.
Nothing is preselected. Newer/undated recordings require the additional
“Show newer or undated recordings too” action. Up to 100 meetings may be selected
per review. Exact selected titles/dates/final-track filenames and bytes appear in
the final confirmation, alongside the retained-material and irreversible-deletion
warnings. No inventory/read/review/focus/navigation action unlinks a file.

Native process-owned plans bind an unpredictable UUID to exact meeting/folder,
metadata bytes and file identities. They expire after 15 minutes, are consumed
once, and the store retains at most 16 plans. A restart loses plans, never restores
consent. The confirm command accepts the token, not arbitrary file paths. Failed
or abandoned replies never become an assumed success; the UI clears scope and
requires a fresh unchecked selection/review for retries.

## Conservative file scope

`audio_cleanup.rs` owns inventory, snapshots and unlink outcomes. Only ordinary,
single-link final audio files whose exact name is declared by completed
`metadata.json` as `audio_file`, `mic_audio_file` or `system_audio_file` are
eligible. Filenames must have the existing `audio`, `mic` or `system` stem and a
supported audio extension. Folder/ancestor symlinks, parent traversal, hardlinks,
shared/overlapping meeting folders, foreign metadata meeting IDs, missing/malformed
metadata, unfinished recording status, unknown audio and `.checkpoints`/`.pending`
material block deletion. Ambiguous legacy folders are deliberately retained;
cleanup never guesses ownership from an extension alone.

Only exact `remove_file` operations are used: never recursive folder removal,
metadata rewrite, schema migration, meeting deletion, queue control, source
relabelling or transcript-version mutation. Recording JSON/backups, track commit
proofs and other artifacts remain. Selected transcripts, archived/corrected
versions, Notes/revisions/recovery associations, summaries, actions/anchors,
`post_call_jobs`, `post_call_results`, candidate/checkpoint metadata and unrelated
recordings are unchanged.

## Ownership and state changes

Confirmation runs in a process-owned Tokio task, detached from the IPC waiter's
lifetime. It uses non-waiting admission to the existing offline operation lock and
engine lifecycle lock, then rechecks active/stopping capture. Busy operations
refuse cleanup rather than scheduling delayed deletion after a recording/model
pass. The offline guard covers ordinary/manual and queued #14 refinement and
meeting diarization. Recovery (including final-audio detection/checkpoint cleanup), direct-file
diarization and voice enrollment now move this existing guard into their blocking
readers, so abandoning an IPC waiter cannot release it early. Audio-only recovery
save transfers its guard through validation and retains it through DB association. Storage
scans and unlink I/O run on blocking workers, not the capture/async core thread.

After admission, `BEGIN IMMEDIATE` pins current meeting/folder/version/queue
ownership and fences other SQLite writers for the bounded selected batch. Jobs in
queued/running/deferred/waiting (and any unknown nonterminal state) protect their
audio; running transcript candidates also protect it. Completed/cancelled jobs
remain unchanged. A cancelled inference still draining owns offline admission;
changing its durable status is not proof its native worker has stopped. No job
is automatically cancelled by cleanup.

State is re-read after the write reservation, not trusted from preview. A claim,
retry or refinement committed while cleanup was waiting is seen before unlink.
Claims/retries submitted after the reservation cannot start using the files
through a new durable state until it releases. Each file is rechecked against
reviewed metadata/identity before unlink. This is cooperative **app-owned**
concurrency protection, not a transaction over SQLite plus the filesystem or
protection against a hostile external process replacing names in the final
stat/unlink window. External removal/replacement observed before the final check
fails closed. Capture startup may briefly wait while admitted deletion owns its
lifecycle guard; wall-clock latency on real Mac/storage is not qualified here.

## Truthful results and retained readers

Every reviewed file gets `removed`, `blocked` or `error`, with an error reason and
zero removed bytes unless its unlink succeeded. A changed meeting does not hide
successful removal of another meeting. Permission errors preserve failed files;
there is no whole-batch success toast or fictional all-or-nothing rollback.
Reported **Removed file bytes** are successful logical unlink bytes, explicitly
**not measured freed disk space**: open file handles, snapshots and filesystem
allocation may retain blocks. If the result is lost, the UI says it cannot confirm
and asks to refresh/review remaining files, without adding an assumed total.

Native completion emits `audio-cleanup-finished` even when execution returns an
error after admission. `useSavedAudioRevision` invalidates existing players,
version availability and open retranscription dialogs across WebViews; focus also
re-observes files. Library materials refresh from the same notification. Existing
native audio resolution/saved-source checks observe missing tracks, and the normal
player explicitly reports unavailable audio. Reprocessing is disabled when saved
sources are absent; saved versions/corrections remain readable/selectable. Search
and Markdown retain #18's canonical selected text/Notes/summary snapshot path and
do not depend on audio presence.

## Verification and limits

Only generated marker bytes (not decodable speech), temporary folders and temporary
SQLite databases are used. No actual user audio, database, auth state, private file,
model or installed application was read or modified. The portable native runner
compiles the production cleanup module, command bodies and library repository
against explicit **already-built** offline dependencies with rustc; it invokes no
Cargo/dependency/desktop build. Tauri transport/AppHandle and active-recorder flags
are fixture adapters, not a claim of real IPC/device qualification. Actual existing
operation/lifecycle lock functions are included. Current migrations and the exact
accepted #15 queue migration are used for the real-schema preservation fixture.

Evidence recorded before final local handoff:

- Native fixture suite: 12 passed (expired/scoped one-shot consent, unrelated meeting,
  recovery/unknown audio/alias/link rejection, changed files/fresh retry, actual
  permission-denied unlink and partial results, queue/version changes after
  review, command capture/offline/lifecycle refusal, queue transition while
  cleanup waits, and abandoned recovery waiter/native-reader ownership). Real-schema fixture compares all table rows before/after and
  invokes actual library search/export to retain RU/EN corrected text, source/time,
  Notes/revision, user summary, task and queue artifacts with audio unavailable.
- Frontend storage fixture: explicit unchecked scope and exact review, no unlink
  before confirmation, protected rows, retained-material warning, partial errors,
  lost-response handling and fresh selection. Player fixture reproduced stale
  availability red, then verifies native notification closes/clears the old player.
- Full isolated frontend suite initially exposed four event-transport mocks that
  were narrower than the new boundary; those fixtures were updated. Final result
  is appended with the local commit/handoff evidence.
- A fixture marker's literal byte expectation was corrected (24 bytes, not 22).
  Initial cached-rustc harness selected a different Tokio build from SQLite; the
  runner now resolves the exact dependency fingerprint. Neither issue required
  changing production dependency versions or building/downloading dependencies.

No full native/Next build, standalone whole-project TypeScript check, inference,
model/audio-dependent test, playback codec, real Tauri asset protocol/picker,
installed-app, physical M4/M1 Mac, DMG/packaging, Actions, push, release or issue
closure is claimed. Heavy verification belongs to #18's coordinated slot.
Independent review is coordinator-owned; this task uses author self-review only
under the explicit no-agents instruction. Final convergence must preserve #18's
canonical summary bridge and #15's priority/claim/epoch semantics.

## Final pre-convergence checks — 2026-11-19

- Narrow cached-rustc native runner: **12 passed, 0 failed, 0 ignored**.
- Complete existing isolated frontend runner: **54/54 files passed**. Expected
  negative-test stderr (simulated save/read/storage failures) is present; no failed
  test files remained. The first full pass was 50/54, second 53/54 while boundary
  mocks were corrected; both intermediate failures are recorded, not hidden.
- `git diff --check`: passed.
- Existing shutdown/recovery portable harnesses now include the actual shared
  offline guard helpers required by the recovery command. Those Cargo/FFmpeg
  harnesses were not rerun in this lightweight slot.

No #18 convergence head has yet been supplied. The local implementation is based
on the exact accepted #18 SHA above, with accepted #15 schema exercised read-only
in generated fixtures. The normal queue UI still belongs to #18's converged head;
integration must preserve its canonical summary bridge and check local-stage
retry presentation when source audio has been explicitly removed. No claim of
merged-convergence or full application-build qualification follows from this pass.

The final storage-accounting regression reproduced a registered checkpoint-source
folder being counted both on its own and through its parent (80 vs 76 generated
bytes). Aggregation now deduplicates that already-traversed checkpoint scope while
still counting independent nested meeting folders. The abandoned-reader fixture
uses the actual recovery command body and admission helpers with a deterministic
blocking decoder adapter; it does not run FFmpeg or establish acoustic behavior.

## R1 follow-up — 2026-11-19 (supersedes initial qualification gaps)

See [ISSUE19_R1.md](ISSUE19_R1.md) for the reproduced Stop/offline lock inversion,
separate internal finalization cleanup, unchanged-player focus/rate fix, scoped
native event and file-identity observation, and the demonstrated Rust diarization
waiter lifetime gap versus unqualified Tauri cancellation behavior. The requested
portable FFmpeg harnesses have now actually run: shutdown **45/45**, complete
recovery/SQLite **83/83**. Narrow native suite **14/14**, mounted playback **3/3**,
full isolated frontend **54/54 files**. All fixtures are generated/disposable.
The accepted bridge head is now supplied as
`2d388cf15adb95f2d37ab1e9ff17b82df723e328`; preserve this standalone R1 commit for
recheck first, then integrate that exact head only after P1/P2 acceptance.
