# Issue #13 — live questions through subscription Codex CLI

Live issue and comments were read on 2026-11-18. Exact title:
**[macOS 09/16] Вопросы по встрече через Codex CLI**; comments: empty.
Accepted software base: `f6b9b2af77aada0543fb0c565034520fba815482`.
This is implementation/portable qualification, not an installed or physically
qualified Mac build, release, or completed real-subscription acceptance test.

The accepted Notes contracts in [ISSUE12_NOTES.md](ISSUE12_NOTES.md), #8's
[qualification experiments](ISSUE8_QUALIFICATION.md), local requirements/spec/ADR/
research and the fixed fork `origin/fix/codex-call-detection` at
`2169cabcac80a22f5a8fea6c54d13e8b971bb261` were inspected. The fork's process adapter
and regressions were reused, not its generic provider dispatch or capture changes.
A residual global-instructions leak in that adapter was reproduced and fixed here.

## User entrypoints and scope

- Recording → **Ask AI** is now `recording/LiveCodexChat.tsx`, reached through the
  existing `LivePanel`. It never calls `ask_live_assistant` or a summary resolver.
- **Что я пропустил?** is an always-available action, not a natural-language hint
  to a recency-ranked retriever. Arbitrary questions include all available earlier
  text from this current recording, subject to an explicit 512 KiB payload limit.
  Oversize requests fail visibly; no silent 12,000-character truncation remains.
- Settings → **Live questions** exposes executable selection, installation/version,
  missing/signed-out/unsupported/blocked/ready status, the exact private auth home
  and a properly POSIX-quoted one-time terminal login command. A readiness probe
  runs only local version/help/login-status commands: it does not test network or
  quota and does not send meeting text.
- Explicit executable paths are saved natively as absolute paths in
  `codex-cli-path.txt`. Standard install locations are also searched for Dock
  launches without shell startup files. No CLI is downloaded or bundled.
- Saved-library/person/group AI and the legacy generic provider APIs are outside
  this issue. They are not called by this live feature. A new low-level adapter is
  deliberately outside the generic provider resolver so there is no API/key/
  local-model/other-CLI fallback, even when summary settings choose another model.

## Request-time data flow and truthful sources

1. The frontend freezes delivered ASR turns synchronously **before any await**.
   A later ASR arrival with an earlier speech timestamp does not enter that ask.
2. `codex_live_cutoff()` atomically reads the current native recording identity and
   its fractional **active recording clock**, excluding pauses. This native
   acceptance time, not the last ASR timestamp or a stale displayed timer, anchors
   the request. The UI refuses if the captured target and returned target differ.
3. Questions use `[0, cutoff]`; lookback uses `[max(0, cutoff - 300), cutoff]`.
   Only complete turns wholly within that interval are included: start is at/after
   its lower bound, end is at/before cutoff. No word timestamps exist, so a turn
   straddling a boundary is omitted rather than trimmed or relabeled at a made-up
   time. UI/prompt explicitly say "whole turns only" and warn that ASR may lag.
4. Notes must already be loaded and saved. Loading/conversion/pending/conflict/
   recovery errors block the ask with an honest message. The accepted
   `notesForContext(target)` boundary returns a detached committed revision; the
   caller additionally rejects a change to the click-time Notes document while
   preparing context. Edits **after dispatch** remain independent and do not change
   the already authorized snapshot.
5. Native `current_text_context()` copies the recording folder, active clock and
   accumulator together, releasing the manager lock before DB or CLI work. Every
   submitted sequence/text/start/end tuple must belong to that native current
   recording. Original native source labels are retained separately from edited
   display labels. Missing timing is rejected, never invented as zero.
6. Native Notes are read only for this exact recording target and must match the
   submitted committed Markdown/revision. No other meeting, contact, summary,
   audio file, directory contents, chat history, arbitrary persona or workspace
   is read into the model input. The folder, rich Notes JSON and local request ID
   are not sent to the model. Only question, bounded transcript and separately
   named `user_notes` with revision enter the prompt.
7. Notes are whole-meeting personal thoughts, **not** timestamped speech, a
   colleague's promise, or proof of an agreement in the five-minute interval.
   The prompt and returned source metadata distinguish them. A Notes-only answer
   must be uncertain. An unsupported confident answer or unknown evidence ID is
   withheld. Model semantic correctness still requires real-service qualification;
   a valid citation ID alone cannot prove that every claim is entailed.
8. The CLI returns a JSON answer with supplied transcript IDs, `notes_used` and
   `uncertain`. Native code validates IDs and adds exact original seconds and
   native sequence IDs. UI citation buttons jump to the actual transcript row;
   native sequence IDs also survive a WebView history resync that changes UI IDs.
   Arbitrary model-written `[99:59]` text is not made clickable. Strict live
   rendering disables external links/images; Notes receive a revision label,
   never a speech timestamp. History is local per recording in sessionStorage
   and is not resubmitted as model context.

## Isolation discovery, auth contract and one-time owner action

**Qualified CLI version: `codex-cli 0.167.2`, Unix process-group platforms only.**
Other versions/platforms fail closed until their emitted requests and cleanup
semantics are qualified. The executable is a trusted installed dependency; its
version string is not a cryptographic attestation or an OS sandbox for a malicious
binary. macOS desktop behavior remains a physical gate; Windows is intentionally
unavailable rather than claiming descendant cleanup without a Job Object.

A real-CLI localhost test initially failed: shared `CODEX_HOME/AGENTS.md` reached
its outgoing request despite `--ignore-user-config`, `--ignore-rules` and
`project_doc_max_bytes=0`. Public 0.167.2 source confirms the unconditional global
provider in `codex-rs/codex-home/src/instructions/mod.rs`; the project-doc budget is
not a global-instruction suppression knob. No provider/system/managed-policy
instructions were replaced to hide this defect.

Production now forces a dedicated persistent **`<canonical app-data root>/
codex-live-auth`** and a separate private, nonrepo temporary cwd. The CLI alone
owns login/credentials/state there. Meetily never reads, copies or symlinks auth
secrets from the general Codex home, and never falls back to it. The dedicated
home must have a canonical parent, be a real directory owned by the current user,
and is protected with mode 0700 on Unix. A final-directory symlink, either
`AGENTS.md` or `AGENTS.override.md`, or an unverifiable path blocks invocation.
These checks run before **each** production subprocess, including after preflight.
This dedicated directory is reserved for the CLI login, not for other sessions or
personalization. A hostile concurrent same-user filesystem writer is outside the
trusted-installed-binary/app-owned-directory boundary.

**Final owner action, not performed by this issue:** open Settings → Live
questions, select the qualified installed executable, and run the exact displayed
terminal command once, choosing ChatGPT subscription login. For example only:

```sh
CODEX_HOME='/Users/<you>/Library/Application Support/Meetily/codex-live-auth' '/opt/homebrew/bin/codex' login
```

Use the actual canonical path/executable shown in Settings, including its quoting;
spaces and apostrophes are handled. Do not copy `auth.json`, config, skills or
AGENTS files. Re-check local status afterward. Existing general-CLI login is not
presented as this isolated login. No owner login, pairing or credential prompt was
performed here. The real subscription smoke is genuinely **pending** that action.

The child environment is cleared and rebuilt from OS/home/locale essentials,
fixed runtime PATH entries plus the executable directory, CI/NO_COLOR, and forced
CODEX_HOME. No API keys, custom provider URLs, proxy/tool credentials, parent
session IDs, NODE_OPTIONS or preload settings are inherited. Generation pins
`model_provider="openai"` and `forced_login_method="chatgpt"`; API-key login is
rejected before meeting stdin. Config/rules, shell/unified exec/patch, MCP, apps,
plugins, hooks, collaboration, browser/search/images, memories, skills and other
execution surfaces are explicitly disabled. Provider/system/managed policy is
left intact. Outgoing `tools: []` and absence of unrelated personalization were
observed in the **real** CLI's localhost requests, not inferred from flag names.

`--ephemeral` avoids normal session rollouts; it is not a promise that the CLI or
OpenAI has no diagnostics/retention. Model execution is remote OpenAI execution,
not local LLM inference. Product code does not log raw prompts, answers, stdout,
stderr or credentials. Errors expose safe categories, not raw CLI diagnostics.

## Native lifetime and bounded behavior

- One active live native job, no unbounded request queue. UUID cancellation
  tombstones (bounded to 64) cover cancel arriving before ask IPC; another request
  or old cancellation cannot cancel a different meeting's job.
- Each probe has a five-second limit. The complete generation, including its
  version/help/auth probes, is bounded to 60 seconds. UI also bounds preparation
  plus invocation to 60 seconds and sends cancellation at its deadline.
- stdin, stdout and stderr are handled concurrently. Each output pipe is bounded
  to 1 MiB; overflow kills the process group immediately rather than waiting for a
  blocked writer. Transcript text is stdin data, never argv or shell interpolation.
- A dedicated Unix process group and kill-on-drop guard own descendants, including
  npm launchers. Cancel/timeout/future-drop kills that group; error paths attempt
  reaping with a bounded cleanup wait. A native 100 ms ownership watchdog cancels
  on Stop/session change, and ownership is rechecked before accepting a result.
- UI unmount/session change aborts and suppresses late results. A crashed WebView
  cannot be the sole cancellation guarantee: native ownership/deadlines persist.
  The CLI does not own capture, ASR or Notes state; no audio lock spans DB/network/
  child execution. Fake-CLI scheduling and mounted UI tests establish independent
  work/Notes progress, **not** actual hardware recorder responsiveness.

## Interfaces for #15 / #16

Tauri entrypoints registered in `lib.rs`:

- `codex_cli_get_path()` → optional saved absolute executable.
- `codex_cli_save_path(path)` → validate/save explicit path or clear to discovery.
- `codex_cli_get_status(path?)` → `{installed,path,version,logged_in,error,
  auth_home,readiness}`. This does not send model input. `ready` is a local
  installation/login result, not a network/quota attestation.
- `codex_live_cutoff()` → `{target,asOfSeconds}` from the active native session.
- `codex_live_ask(requestId, request)` → `LiveAnswer` or visible failure.
- `codex_live_cancel(requestId)` → cancel/tombstone only that request.

Types live in `lib/live-codex.ts` and native `codex_cli/context.rs`:
`QuestionSnapshot` has target, mode, question, from/as-of seconds, exact transcript
turns with sequence IDs, and `{source:"user_notes",markdown,revision}`.
`LiveAnswer` has answer, uncertainty, verified `{id,sequenceId,time,text}` evidence,
optional cited Notes revision and the actual interval. Unknown request fields are
rejected. This is a **live recording** contract, not a saved-meeting query API.

Reusable native provider boundary:

```text
commands::isolated_auth_home() -> Result<PathBuf,String>  [crate-visible]
commands::codex_cli_get_path() -> Result<Option<String>,String>
cli::probe(configured_path, auth_home) -> CodexCliStatus
cli::generate(configured_path, auth_home, model, system_text, user_text,
              Option<&CancellationToken>) -> Result<String,String>
```

`DEFAULT_MODEL` delegates model choice to the qualified CLI's subscription default;
no API provider/model resolver is consulted. System/user strings here are prompt
formatting, not a claim of a separate CLI system-role API. All isolation, version,
auth, process and deadline checks remain in this adapter. Do not weaken them in a
new summary caller, or pass meeting folders as cwd/auth home.

- **#15:** reuse this provider and the same dedicated auth-home resolver. Own the
  durable native queue, native cancellation token and priority/yield policy there;
  `Jobs` here is transient live admission, not a persistent post-call queue.
  A post-call caller must capture #14's selected projection and independent
  revisioned Notes, scope them to the explicit saved meeting, validate stale
  versions, persist results/retry state, and preserve previous results on failure.
  Do not call the live service with a stopped-meeting identity or route through a
  generic summary API as a fallback. A different timeout policy requires an
  explicit bounded contract, not silently removing this adapter's deadline.
- **#16:** no glossary or unrelated corpus is implicitly loaded here. Add only
  owner-confirmed, bounded vocabulary as an explicitly named new schema source
  with its own scope/revision/limit tests. It must not become speech evidence or
  authorize reading old meetings/contacts. Updating personal AGENTS or Codex
  config is **not** the vocabulary integration seam. ASR hints remain #16's own
  engine-specific interface; this issue makes no model-training promise.

## Verification, reproduction and remaining gates

Portable verification on Linux uses only synthetic fixtures and the accepted
workspace lock. The harness compiles real CLI/context/service/command bodies and
actual recording text-clock/context helper bodies. Only Tauri state transport,
recorder-manager methods and Notes DB transport are stubbed; it is not a desktop
ABI, SQLite integration, device or real ASR test.

```sh
# Run every frontend file in a separate process, from frontend/.
bun scripts/test-isolated.mjs
# Small module build only; use the coordinator-approved warmed target/jobs slot.
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=<warmed-target> python3 -I frontend/scripts/test-codex-portable.py
# Opt-in REAL installed CLI, empty synthetic homes + localhost Responses fixture.
MEETILY_CODEX_TEST_BINARY=/absolute/codex CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=<warmed-target> python3 -I frontend/scripts/test-codex-portable.py real_binary --ignored
```

- Full isolated frontend suite: **41/41 files passed**, including request-window,
  long-context, delayed-ASR/Notes snapshot, actual live UI/cancel/switch/deadline/
  offline/citation and Settings callers. Notes remain editable/saveable while the
  fake CLI waits. The deadline test advances the production 60-second callback;
  it does not wait 60 real seconds. Existing recording-notice tests intentionally
  print their synthetic disk-error diagnostics while passing.
- Portable native default suite: **25 passed, 3 real-CLI tests ignored**. It covers
  actual command dispatch, source/Notes verification, both AGENTS filenames,
  path/owner/symlink policy, CLI version rejection, API-key refusal, safe error
  categories, tool/partial-response rejection, output limits, group cleanup and
  changing native ownership. The clock helper test supplies active=600 s versus
  wall=1200 s; it checks the selected interface, not physical pause clock accuracy.
- Optional real CLI **0.167.2** localhost controls: **3 passed separately**:
  dedicated-home isolation, recovered stream error, and shared-home positive
  control. No credentials or authorization headers, real account inference,
  private text, recordings, models or external provider service were used. The
  fixture-only routing override is test code, never a product parameter. Intended
  synthetic speech/Notes are present; unrelated global/project/config/host-skill
  sentinels are absent in the safe request and tools are empty.
- Executed red regressions found inherited environment/provider routing, global
  AGENTS forwarding, and output-overflow waiting for timeout. Their green checks
  exercise actual child processes/real CLI requests, not source-string matches.
- Sanitized local evidence logs (not committed diagnostic artifacts):
  `/tmp/meetily-issue13-native-final.log`,
  `/tmp/meetily-issue13-real-cli-final.log`,
  `/tmp/meetily-issue13-frontend-final.log`. Reproduce with the commands above;
  temporary synthetic prompts/homes are removed by fixture ownership.
- Standalone frontend types passed during implementation. The final production
  export is assigned a serialized coordinator slot after the local implementation
  commit; its exact SHA/result must be reported separately, not inferred here.
  No full desktop/native, DMG, install, Actions, push, PR, release or Mac/private
  data operation is part of this implementation.

**Pending:** owner sign-in to this isolated home and a real subscription smoke on
owned RU/EN synthetic text; then both target Macs, Dock path discovery, physical
90-minute recording/ASR responsiveness during successful/offline/auth/quota/cancel/
timeout asks, pauses/resumes, delayed ASR and meeting switching, exact citations
and Notes provenance. Check capture continuity, ASR lag/queue and saved audio,
Notes edits and restart behavior instead of treating scheduler/mock results as
hardware evidence. Real-service answer latency <=60 s and semantic accuracy are
not established by a localhost fixture. No subscription login is requested now.
