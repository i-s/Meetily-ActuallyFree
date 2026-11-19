# Issue #20 — single personal macOS candidate and batched owner record

**Software/build preparation only. Physical acceptance: NOT RUN.**
Real Zoom and Discord remain **observation-only, `qualified:false`**; this candidate
cannot complete the required real-source auto-start/auto-stop AC. Do not enable a
synthetic qualification override or rename manual recording as automation.
No issue closure, release, default-branch push, owner-machine access, credential
operation, upload of meeting material, or installation is authorized by this note.

## 1. Authority, source and present evidence

Installed-startup follow-up: the confirmed SQLx missing-history failure is repaired
by restoring authentic fork migration bytes, not bypassing validation. The
[exact lineage/checksum audit and generated-DB tests](ISSUE20_MIGRATION_LINEAGE.md)
record the source repair and added native workflow gate. Owner relaunch on a
corrected final artifact remains unobserved here.

Live issue/body/comments and linked AC read 2026-11-19:
[#20](https://github.com/i-s/Meetily-ActuallyFree/issues/20),
[#5](https://github.com/i-s/Meetily-ActuallyFree/issues/5),
[#6](https://github.com/i-s/Meetily-ActuallyFree/issues/6),
[#7](https://github.com/i-s/Meetily-ActuallyFree/issues/7),
[#8](https://github.com/i-s/Meetily-ActuallyFree/issues/8), and blockers
[#10](https://github.com/i-s/Meetily-ActuallyFree/issues/10),
[#11](https://github.com/i-s/Meetily-ActuallyFree/issues/11),
[#15](https://github.com/i-s/Meetily-ActuallyFree/issues/15),
[#17](https://github.com/i-s/Meetily-ActuallyFree/issues/17),
[#19](https://github.com/i-s/Meetily-ActuallyFree/issues/19). All had zero comments.
Also read planning AGENTS, glossary, requirements, macOS spec, ADR 0001 and the
landscape/audio/Codex/call-signals research in `local-meeting-helper`.

Accepted implementation base: **`50f6e973a637f8da9239df058d7df6d0bece2730`**,
not the originally researched `e091a7a` and not historical PR21's source `aa97cdf`.
The coordinator handoff supplies the final local source SHA, parent(s), source
tree, workflow commit/tree and any tree-equivalent remote import separately.
Do not copy a SHA from this paragraph into a build without that final handoff.
App version remains **0.2.18** in package.json, Cargo.toml and tauri.conf.json.
2026-11-19 authenticated repository release-list check returned `[]`; latest
release returned 404. No version bump or publishing is needed for a personal
Actions artifact. Recheck live release state before any later release work.
Version text is not candidate identity: several different sources use 0.2.18.

### Preparation evidence (not owner acceptance)

| Check on this accepted application source | Result / limit |
| --- | --- |
| Frozen frontend install | PASS; Node 22.14.0, pnpm 10.28.2; lockfile unchanged. Actions uses Node 24 / pnpm 11.9.0, so this is not identical-host qualification. |
| Next production build from **frontend cwd** | PASS; Next 14.2.35, type/lint and 12 static pages. Nonfatal old Browserslist and ambiguous Tailwind-class warnings retained. |
| Actual `frontend/out` | PASS; eight required routes and 248 nonempty local JS/CSS references verified by `scripts/verify_macos_candidate.py export frontend/out`. This does not prove WebView rendering. |
| Frontend behavior | PASS, 73/73 files, separate Bun 1.3.10 processes via existing `frontend/scripts/test-isolated.mjs`. No desktop transport/models/physical devices. |
| Portable qualification tools/build orchestration | PASS, original 66 Python tests; additional eight candidate input/output checks cover version/path/identity/updater drift, missing routes/chunks and traversal. |
| Cleanup/version/queue/native command bodies | PASS, 43 tests via existing `test-audio-cleanup-portable.py`, Rust 1.97.1, explicit warm dependencies. Generated SQLite/files; Tauri/model/device adapters, not a desktop build. Covers exact consent, active/draining/queue races, partial unlink, playback identity and preserved search/export. |
| Full native `cargo check` / macOS compile | **NOT RUN** locally: Linux `pkg-config` absent; known GTK/WebKit prerequisites missing. No reinstall or repeated failed dependency builds. Final Actions native gates remain NOT RUN. |
| Real models, subscription CLI, network capture, DMG, installation, two Macs | **NOT RUN**. No owner credential operations or recordings used. |

Reproduce portable verification from checkout root (installed tools required):

```sh
python3 -I -B -m unittest discover -s tests -v
python3 -I -B scripts/verify_macos_candidate.py source .
(cd frontend && PNPM_CONFIG_ENABLE_GLOBAL_VIRTUAL_STORE=false pnpm install --frozen-lockfile)
(cd frontend && NEXT_TELEMETRY_DISABLED=1 pnpm run build)
python3 -I -B scripts/verify_macos_candidate.py export frontend/out
(cd frontend && pnpm dlx bun@1.3.10 scripts/test-isolated.mjs)
```

Serialize Next with all native embedding of `frontend/out`; no simultaneous
standalone tsc. Do not count cached `.next` output or a root-cwd build as an export.
The bounded Rust test helper's explicit local cache paths are recorded in the
coordinator evidence handoff; they are not portable public prerequisites.

## 2. One final Actions build — coordinator only

**Recommended existing remote branch/ref:**
`refs/heads/meetily/issue-5-actions-build`, existing open
[PR21](https://github.com/i-s/Meetily-ActuallyFree/pull/21),
workflow **`.github/workflows/issue5-macos-artifact.yml`** (display name now
“Issue 20 - final pinned macOS artifact only”). Reuse this route, do not create a
second DMG workflow or dispatch the main-only release machinery.

Historical PR21 head `56eb2f9c984c5c03176a7576b3cfe654b2255fd0` pins `aa97cdf`
and tree `8a75052`; its artifact does **not** contain #5–19. That remains true
until the coordinator performs the final reviewed repin/update below.

1. Independently review the final source and workflow commits. Verify base,
   parents and exact source tree; no frontend/native implementation changed by
   this packaging preparation. Source commit and workflow pin commit are distinct
   to avoid a self-referential source hash. Preserve the accepted history locally.
2. Inspect **live** PR21 head/base, auto-merge and merge-queue state, all workflow
   triggers on current remote/default/source refs, and current Actions runs. Do
   not enqueue/merge PR21. Confirm no running/pending equivalent candidate build.
3. Make source reachable once without opening a source PR. If GitHub App import
   changes commit metadata, compare the complete tree to the reviewed local tree;
   set `SOURCE_SHA` to the actual imported source, retain `ORIGINAL_LOCAL_SHA`,
   keep `SOURCE_TREE` identical. Do not claim original SHA was checked out.
   Recheck source-staging branch push effects before importing it.
4. Update existing PR21 branch **once** with the final reviewed workflow pin.
   That synchronize event (not an additional dispatch) is the intended single
   DMG trigger. A new PR plus dispatch or a second workflow-file edit would
   duplicate work. Coordinator owns the write, PR linking, checks and monitoring.
5. Verify the run's actual checkout, tree, workflow SHA, PR head and event SHA.
   Stop on mismatch. A failed build is diagnostics, not an installer; review a
   concrete failure before any newly authorized rerun.

### Trigger/effect audit on this checkout

- Artifact workflow: `pull_request` to main; opened/synchronize/reopened;
  path filter exactly its YAML; same repository and exact existing task-branch
  guard; **contents:read**, credential persistence off; Apple Silicon `macos-15`;
  concurrency `issue5-macos-artifact`, cancel-in-progress false. No dispatch,
  push trigger, release environment, signing secret, publisher, tag/write token,
  `pull_request_target`, or `workflow_run` chain.
- `pr-checks.yml`: all PRs run Linux frontend checks (including Next), not a DMG;
  push is main-only. This additional PR check is expected, not duplicate packaging.
- `build-macos.yml`: manual, main-only with protected `macos-release` environment;
  **not this route**. `publish-macos.yml` is a manual contents-write publisher;
  `preview=true` still publishes a prerelease and is forbidden here.
- Other build/devtest/validation/smoke workflows are manual or reusable;
  `submit-virustotal.yml` runs on published release or manual dispatch. No release
  means no intended VirusTotal upload. Re-audit live revisions before remote writes.

### What the final workflow does and retains

It verifies checkout SHA/tree/cleanliness and aligned 0.2.18/source identity,
records SDK/OS/tool/lock/model hashes, installs frozen dependencies, builds Next
from frontend cwd and verifies its actual export, builds the Metal `llama-helper`,
runs existing native stream/core-audio/pipeline tests, then uses the existing
`frontend/scripts/build-macos-apple-silicon.sh` to package one aarch64 DMG.
Tauri's build hook repeats Next sequentially; that is within the **same one**
packaging run, not an intermediate candidate. Output is checked again.

A read-only mounted DMG must contain arm64 executable `meetily`, `llama-helper`,
`ffmpeg`; strict valid code signature, minimum macOS 14.2, expected bundle ID and
version; no direct build-host/Homebrew dependency in these binaries; working
FFmpeg synthetic encode/decode and helper ping/shutdown. Three bundled model
files must be byte-identical to pinned source, not just nonempty. This is limited
inspection of those binaries, not proof of every runtime-loaded dependency.

Success artifact: **`issue20-macos-<run-id>-<attempt>`**, retained 14 days:
`Meetily-issue20-aarch64.dmg`, `SHA256SUMS.txt`, `issue20-provenance.json`,
`source-identity.json`, build-inputs, export checks, native/build logs, signature,
binary and bundled-model inspections. Failure artifact is named `issue20-failure-*`
and is **not installable**. Download the success artifact from Actions, **not
Releases**. Preserve it locally before expiration. Record artifact ID/digest from
GitHub metadata separately from the DMG's SHA-256; they are different objects.

Pinned source/locks make the procedure repeatable, not bit-reproducible:
macos-15 image/SDK, Homebrew, Node patch, signatures/timestamps and external
FFmpeg bytes can vary. FFmpeg build-time URL is versioned but lacks an independent
expected archive hash; cached binaries are startup-checked, not content-pinned.
The workflow records the resulting binary and final DMG hashes. Do not describe
this remaining input limit as a reproducible-byte guarantee.

## 3. Installer, payload, permission and update boundary

**Final installer path once built/downloaded:**
`<private-download>/issue20-macos-<run-id>-<attempt>/Meetily-issue20-aarch64.dmg`.
**Proposed installed path after owner-controlled preparation:**
`~/Applications/Meetily-Issue20/Meetily - Actually Free.app`.
No DMG exists from this preparation; run ID, artifact ID, digest and DMG hash are
**NOT RUN / unassigned**, not fabricated paths to an existing installer.

### Included versus prepared locally

| Payload | Status |
| --- | --- |
| arm64 app, Metal llama-helper, FFmpeg | Built/bundled by Actions; verify actual artifact hashes/startup. No Linux binary substitution. |
| `segmentation-3.0-fp16.onnx`, `wespeaker-resnet34-LM.onnx`, `xvec_transform.npz` | Checked-in Pyannote/WeSpeaker resources, bundled and compared byte-for-byte. These are not speech-to-text weights. |
| ASR weights: selected multilingual Whisper or supported Parakeet | **Not supplied by the DMG**. Prepare successfully on each Mac before offline tests; record live and post-call engine/model/hash/parameters independently. Never infer RU support from an engine name. |
| Nemotron / other optional models; local LLM weights | Separate optional preparation, not silently bundled/activated or substituted. Record selected-engine failure; retain source labels if appropriate. |
| Codex CLI, ChatGPT subscription login | **Not bundled**. Owner's installed compatible CLI and subscription only; no agent installation/login/auth-file access, API key or alternate provider fallback. |
| Python / probe tools | Evidence-tool prerequisites, not needed merely to install app. Existing Swift observation probe needs reviewed native build if used; no Xcode repair on either Mac here. |

No ASR/model/CLI download or owner access was performed for this record.

### Read-only inspection before any launch

Set `DOWNLOAD` to a new empty extracted-artifact directory, `REPO` to the matching
source checkout and `EVIDENCE` to a new private local evidence folder outside git.
Keep evidence/recordings off cloud-synced folders. Preserve the untouched download.

```sh
(cd "$DOWNLOAD" && shasum -a 256 -c SHA256SUMS.txt)
cat "$DOWNLOAD/issue20-provenance.json"
hdiutil attach "$DOWNLOAD/Meetily-issue20-aarch64.dmg" -readonly -nobrowse
# Set APP to the mounted app path reported above; do NOT launch it yet.
codesign --verify --deep --strict "$APP"
codesign -dvvv --entitlements :- "$APP" > "$EVIDENCE/signature.txt" 2>&1
/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$APP/Contents/Info.plist"
spctl --assess --type execute --verbose=4 "$APP" > "$EVIDENCE/gatekeeper.txt" 2>&1
# spctl rejection is expected for ad-hoc/not-notarized; record it, do not bypass globally.
xattr -l "$DOWNLOAD/Meetily-issue20-aarch64.dmg" > "$EVIDENCE/quarantine.txt"
```

Confirm provenance SHA/tree/version/hash against the coordinator's final pin,
Actions metadata and downloaded hash. Inspect the three binary architectures,
dependencies, hashes and bundled model hashes against artifact records. Detach
the mounted volume after inspection using its actual mountpoint.

Ad-hoc signature (`signingIdentity: "-"`), hardened runtime, audio-input entitlement;
**not Developer ID signed and not notarized**. Valid `codesign --verify` is not
Gatekeeper trust. Download method can affect quarantine. Do not remove quarantine
recursively or disable Gatekeeper/SIP. Owner may later use macOS Privacy & Security
“Open Anyway” for this hash-verified app; blocked launch is a recorded outcome,
not permission to change global policy.

### Data and installation are owner-controlled, never automatic isolation

Same bundle ID **com.meetily.ai** and existing data locations are intentional.
A renamed .app, `HOME` override or separate app directory does **not** isolate data.
Before owner launch use the detailed bounded inventory/closed-app backup/restore
procedure in [#5 §same-account boundary](ISSUE5_MACOS_BUILD.md#proposed-same-account-test-boundary--owner-decision-required).
Relevant roots include `~/Library/Application Support/Meetily` (SQLite with
WAL/SHM/models), `~/Library/Application Support/com.meetily.ai` (plugin stores),
actual recordings root (default `~/Movies/meetily-recordings`), preferences and
saved state. Custom paths may exist. First launch may migrate the shared DB.
Copying an app is not a data backup; old binaries must not be launched on a newer
schema without the matching closed-app data backup. TCC/keychain state may remain
outside file rollback. No account/bundle-ID/data-path change is proposed here.

The owner must operate installation, protected settings and any existing-data
backup/move/restore under that boundary; **not performed by this preparation**.
All test meetings below are generated/owned test material only. If clean synthetic
state cannot be established without exposing real data, stop before launching.

Record microphone and Screen & System Audio Recording consent, source readiness,
and Accessibility for observation-only Zoom/Discord when applicable. Deny each
permission once in the planned test, record the actionable error and recovery,
never treat missing audio as silence. Reopen when macOS requires it. Entitlements
or Info.plist purpose strings do not grant permissions automatically.

### Launch / later manual update

After owner-controlled installation/preflight, launch the exact app binary, not a
possibly different LaunchServices copy, for timing evidence:

```sh
APP="$HOME/Applications/Meetily-Issue20/Meetily - Actually Free.app"
MEETILY_ISSUE8_TIMING=1 "$APP/Contents/MacOS/meetily" > "$EVIDENCE/app.log" 2>&1
```

Record executable PID/path/hash and app version. Verify microphone/system devices,
audio auto-save and recordings root before recording. macOS in-app automatic
updater path is disabled by `UpdateCheckProvider`; no updater artifacts are built.
Do not use the Windows upstream latest.json or promise auto-update. A later manual
update needs a newly reviewed source/artifact/hash, quit/drain, closed-app backup,
owner replacement of the chosen .app and the short restart/persistence check.
Keep the old app and matching data backup for rollback. Never replace a running
app or downgrade only the binary against migrated data.

## 4. Single batched physical protocol — repeat on BOTH Macs

This section is the owner-facing checklist **and record**. Copy this document
outside git for execution; replace NOT RUN only with observed result, date,
operator, artifact/meeting IDs, exact steps and local evidence paths/hashes. Use
PASS / FAIL / BLOCKED / NOT RUN separately; no checkbox alone establishes a pass.
Preserve failures and restrictions, not just the final successful retry.

### A. Common identity and safe fixture preparation

| Field | M4 Pro / 48 GB | M1 Pro / 32 GB |
| --- | --- | --- |
| Execution date/operator; macOS build; client versions | NOT RUN | NOT RUN |
| Local/actual source SHA + tree; workflow/PR/event SHAs | NOT RUN | NOT RUN |
| Run URL/ID/attempt; artifact ID/digest; DMG SHA-256 | NOT RUN | NOT RUN |
| App path/PID/hash/version; minimum OS/signature/quarantine | NOT RUN | NOT RUN |
| Backup boundary / synthetic-only state; rollback evidence | NOT RUN | NOT RUN |
| Input/output route/device IDs, rates, volume, headset model | NOT RUN | NOT RUN |
| Live/post-call ASR engines/models/hashes/settings; diarizer | NOT RUN | NOT RUN |
| Codex version, compatible flags, subscription status (no secrets) | NOT RUN | NOT RUN |
| Corpus/reference/synthesis version+voice+seed; audio/text hashes | NOT RUN | NOT RUN |

Use the same owned RU/EN/mixed corpus on both Macs and all power/route comparisons.
Use the six reference phrases and local generation schedule in
[#8 physical protocol](ISSUE8_QUALIFICATION.md), plus distinct early/late facts:
RU “Решение в начале: код маяка 427”; EN “Early decision: the test budget is 73”.
No real names, private meetings, owner recordings or credentials. Use distinct
near/remote voices from owned material or consenting test participant. Record
synthesis/local-recording method and actual speech boundaries; no invented timing.
Keep a short quality screen before the long matrix and retain failed model results.

### B. Short correctness + source separation (each Mac, both routes)

1. With source auto-save ready, run mic-only/system-silent, system-only/near-silent,
   alternating RU/EN and deliberate simultaneous speech, then silence. Test built-in
   speakers **and** the actual headset. Inspect/play mic/system/mix separately,
   record alignment, actual text/times/provenance and omissions/duplication.
2. Use existing `issue6_audio_probe.py hash/tracks/transcript`, the
   [#6 evidence schema and method](ISSUE6_AUDIO_PROBE.md), non-destructive WAV
   derivatives via bundled FFmpeg, and original file/metadata/transcript hashes.
   Separate acoustic leakage from software duplication and wrong labels; track
   correlation alone is not attribution evidence. Do not mute mic during remote
   speech to manufacture success. Remote-only text must not be falsely attributed
   to the owner; double-talk must retain near speech. Anonymous speaker IDs are
   not identity. Unqualified source labels must stay honest, not forced to `You`.
3. Stop, wait for final save/ASR drain, export, quit/reopen, find each generated
   meeting, compare text/times/manual labels/Notes, replay all available sources
   and export again. Save actual duration/bytes/hashes. This repeats historical
   #5 persistence on the **new final artifact**, not an earlier installer.
4. Swap input/output while recording, unplug/reconnect headset, deny/regrant mic
   and system permission in separate controlled takes. Record visibility, missing
   intervals, route transitions, device errors and recovery; no fabricated full
   integrity across a gap. Keep source provenance through all label changes.

### C. Zoom / Discord observation gate (not automation acceptance)

Per client/version/Mac journal idle, app-open, settings/mic-test, preview/waiting,
ringing, actual join initially muted, speech, silence, mute/unmute, reconnect,
device change, leave with client still open, and next true call. Record helper PID
changes and assistant's own capture to detect feedback loops. Discord: separately
server voice, DM, group DM; support is **not yet physically established** for any.
Observe read-only Accessibility/CoreAudio paths, not partner-only RPC scopes,
self-bot or user-token workarounds. Preserve permission/unknown/observation errors.

Reuse [#7 probe/journal](ISSUE7_CALL_SIGNALS.md),
[#10 observation](ISSUE10_ZOOM_AUTOMATION.md),
[#11 observation/compare](ISSUE11_DISCORD_OBSERVATION.md), and the
[combined authority limits](ISSUE10_11_INTEGRATION.md). Compare candidate events
with independently timestamped truth: false starts/stops and join/end latency,
mute/silence continuity, late callbacks, manual Stop/keep-recording, next-session
reset. Current native real classes must remain `qualified:false`, and observation
must **not** start recording. Manual synthetic recording can exercise content
alongside calls, but is not a pass for #10/#11. If the required verified automatic
mechanism is still absent after evidence, record **BLOCKED**, remaining work and
ADR suitability decision; do not quietly relax the AC.

### D. Long RU/EN + concurrent Q&A and power

Four matched runs: each Mac × AC/battery, **>=5400 seconds each**. Both audio
routes must have the short matrix above; record which route each long run used.
Do not average one machine's result into another. A battery interruption below
5400 seconds is incomplete/failure, not a 90-minute pass.

Follow [#8](ISSUE8_QUALIFICATION.md) with `MEETILY_ISSUE8_TIMING=1`. Preserve:

```sh
python3 -I -B "$REPO/scripts/issue8_qualification.py" sample "$PID" \
  --duration 5400 --interval 5 > "$EVIDENCE/resources.json"
# New filename for every question; synthetic text only. No login operation here.
python3 -I -B "$REPO/scripts/issue8_qualification.py" codex \
  --timeout 60 > "$EVIDENCE/qna-001.json"
# Only after Stop and final ASR drain:
python3 -I -B "$REPO/scripts/issue8_qualification.py" events \
  "$EVIDENCE/app.log" > "$EVIDENCE/asr-observations.json"
```

Also exercise **in-app** subscription Codex Q&A, not only the standalone probe.
At roughly 1/15/30/45/60/75/89 minutes ask “What did I miss?” (last five minutes)
and specific current facts while ASR is active. At minute 75 ask the **early**
427/73 facts, edit a current unrelated fact and repeat: the answer must cite the
correct old source rather than hallucinate or use only recent context. Inspect
visible context/evidence, Notes attribution and selected transcript revision.

Measure speech-end → stable visible text on aligned monotonic recording clock
at least every minute, around Q&A and through the final minute; include uncertainty,
RU/EN/mixed accuracy and missing words. Record max/p50/p95, early/middle/late lag,
queue growth/service timing, peak RSS and **system** swap, final drain/counts,
battery percent/thermal/energy mode and workload. Target live lag <=10 s, available
service Q&A <=60 s. Sampled dispatcher timing is **not** WebView latency. Include
matched Q&A-off windows and source-audio integrity comparisons. Outages/timeouts
are separate failures/limits, not successful sub-60s answers. No hidden backlog
or lost final tail. Use `docs/fixtures/issue8-qualification.example.json` as the
external run journal; its `qualification` subcommand reports blocked/failed or
manual-review-required, **never grants physical acceptance**.

### E. Notes, vocabulary, canonical artifacts and next-meeting queue

Use only generated meetings A and B; snapshot selected version/Notes/summary/task
IDs, sources and content before/after each action.

1. Type RU/EN rich Notes during capture, edit quickly while save A is in flight,
   navigate/remount and reopen. Confirm last committed content/revision survives;
   an unsuccessful newer save must not make the committed revision invisible.
   Ask Q&A and later summary: Notes are personal thoughts, not spoken agreement.
2. Correct a term and explicitly Remember it; reject/close other suggestions.
   Only confirm changes vocabulary; confirmed spelling wins, repeat proposals
   deduplicate, repeated rejection does not spam. Add a manual term with Codex
   unavailable. Check later meeting B's ASR hints and text-only summary context;
   this is not model retraining. Record exact config and unsupported-engine limits.
3. On Stop observe queued local repeat ASR → selected diarization → Codex summary,
   visible stage/result and selection of candidate transcript, not destructive
   overwrite. Keep manual edits/originals, source/timestamps and Notes. Record
   short summary, decisions, grounded tasks/owners/deadlines and open questions.
4. Defer/resume on battery; start B while A's heavy work is pending/running. Record
   pause/yield, bounded worker ownership and recovery when B stops. Retry failed
   stages; cancel while working and while awaiting CLI. Quit/restart mid-queue.
   No lost progress, unbounded duplicate tasks, stale commits or deleted revisions.
   Record CLI failure/no-network/limit outcomes, not just fake success.
5. Correct Notes after a summary, including two superseding drafts; summary becomes
   stale on committed changes, refresh uses the committed revision, generated task
   IDs/completion stay stable and curated owner mappings survive. Verify in the
   normal library/meeting screen, not only a separate queue test UI.
6. Search early/late words and Notes; inspect snippets, selected version and retained
   summary. Export Markdown before/after corrections, restart and cleanup; compare
   actual text, task statuses/IDs/owners and source distinctions. No hidden sync.

### F. Prepared-model offline / network / sign-in / cancellation

Perform network changes **only by the owner in a controlled test window**, not
by this agent and not unannounced on a work network. Prepare exact local models
first; preserve settings and record selected providers. No model swap/fallback
when unavailable. Test cold app restart offline, not only a warm loaded model.

- Offline capture, Notes save, live ASR, repeat ASR, selected local diarization,
  search/playback/export must work with those prepared assets. Codex fails visibly
  without blocking them; local results persist and summary can retry later.
- With owner-approved subscription login already available, record CLI version,
  subscription status (no auth-file/token reads), successful answer and elapsed.
  Separately observe signed-out/expired/limit/unavailable CLI and cancellation.
  Owner alone performs any login/logout; **no credential operation by worker**.
  No separately billed API key, alternate provider, browser login automation or
  credential copying. If state unavailable, mark test NOT RUN rather than fake it.
- Record process-attributed network observations for capture-only, offline work,
  Q&A, summary and vocabulary proposals. Correlate timestamp/request ID with the
  reviewed **text** payload (generated data), child CLI process and destination.
  Permit agreed meeting text/Notes only through subscription Codex/OpenAI.
  Distinguish model/dependency downloads and ordinary OS traffic from content
  uploads; prepare models before content tests. Endpoint/IP capture alone cannot
  prove TLS payload privacy. Check source/CLI invocation and actual process
  observations together; unexpected content destination/audio access is FAIL and
  must stop the content operation. No audio attachment/upload, separate API
  fallback or unrequested telemetry/artifact publication is acceptable.
- Cancel/start/Stop rapidly, restart app after failed save/CLI/queue stages, change
  devices and deny permissions. Observe child process termination, bounded queue,
  visible actionable errors and preserved last good content. No stale late result
  may overwrite a newer take. Keep failure evidence private and redact secrets.

### G. Exact-consent cleanup — generated data only, last

First export and hash every generated meeting's transcript, manual edits, Notes,
summary, tasks and vocabulary. Preserve private test backups outside the app.
**Never select owner recordings or a whole-meeting Delete action.**

1. In normal library audio storage view record total bytes, selected recordings,
   candidate paths/sizes, protected/unsupported counts and the app's advisory
   proposal rule: review recordings **older than 30 days** (based on meeting
   creation date). This is an explicit convenience heuristic, not an agreed quota
   or auto-delete policy; every recording may be kept. Confirm no automatic deletion.
2. Open review for A; inspect exact selected audio and retained materials. Cancel
   and close once; compare every file/hash unchanged. No confirmation = no unlink.
3. Reopen review then change queue state/start a recording or make A a running
   reader before confirming. Native ownership/state revalidation must reject
   unsafe/stale scope, not merely rely on a disabled frontend button. Record the
   protected source, error and unchanged bytes; retry only from fresh review.
4. Confirm a fresh explicit scope for completed generated A. One-time consent
   affects only listed audio; duplicate/replayed confirmation must not extend it.
   Show actual bytes freed per file; retain all non-audio materials and source
   metadata. Playback/repeat ASR now explicitly unavailable, not a stale player
   or an apparent empty successful retranscription. B must remain intact.
5. For a **generated-only disposable fixture**, owner may make one selected file
   undeletable or change it after review. Record partial/stale failure, remaining
   file(s), actual freed bytes and fresh retry; do not inject faults into real
   data or use privileged deletion. Unsupported failure injection = NOT RUN.
6. Reopen app; search and Markdown still contain A's selected transcript/manual
   edits/Notes/summary/tasks. Compare IDs/revisions/text hashes to pre-cleanup,
   apart from expected audio availability. Test playback replacement/removal
   revalidation without any destructive action on unrelated paths.

## 5. Required result record — all physical entries initially NOT RUN

Each cell must link to **local** evidence IDs/paths/hashes, observed outcome and
limits. Keep individual cases/power/routes distinct in the copied record.
Do not attach recordings, DBs, models, crash dumps, CLI auth or private logs to git,
GitHub or chat. A redacted human summary is separate from uploading evidence.

| Gate (live AC coverage) | M4 Pro / 48 GB | M1 Pro / 32 GB |
| --- | --- | --- |
| Artifact identity / install / permissions / short capture / reopen (#20.1, #5) | NOT RUN | NOT RUN |
| Speakers remote-only/near-only/double-talk RU+EN (#20.2, #6) | NOT RUN | NOT RUN |
| Headset remote-only/near-only/double-talk RU+EN (#20.2, #6) | NOT RUN | NOT RUN |
| Zoom transition truth/errors/latency/manual-Stop/next call (#10, #7) | NOT RUN; automation BLOCKED | NOT RUN; automation BLOCKED |
| Discord server / DM / group DM support and all transition negatives (#11, #7) | NOT RUN; automation BLOCKED | NOT RUN; automation BLOCKED |
| In-person manual start/pause/resume/Stop, both routes (#20.2) | NOT RUN | NOT RUN |
| 90min AC: quality, display lag, Q&A, backlog/drain, memory/swap (#20.3, #8) | NOT RUN | NOT RUN |
| 90min battery: same metrics and integrity (#20.3, #8) | NOT RUN | NOT RUN |
| Notes / vocabulary confirmed+rejected / early-context Q&A (#20.4, #17) | NOT RUN | NOT RUN |
| Queue battery/defer/yield to B/retry/cancel/restart, grounded summary (#20.4, #15) | NOT RUN | NOT RUN |
| Search/Markdown/canonical task+Notes continuity (#20.4) | NOT RUN | NOT RUN |
| Prepared-model cold-offline local paths and Codex failure (#20.5) | NOT RUN | NOT RUN |
| Subscription auth/error/limit; process+payload network boundary (#20.6) | NOT RUN | NOT RUN |
| Permission denial, source device change, cancel/restart (#20.7) | NOT RUN | NOT RUN |
| Exact consent/cancel/state race/partial failure/retained materials (#20.4, #19) | NOT RUN | NOT RUN |
| Manual update and matched data rollback (#20.1, #20.7) | NOT RUN | NOT RUN |

For each long run append: ID, route/power/duration, corpus/model hashes, accuracy,
lag min/median/p95/max + uncertainty, early/middle/late queue, peak RSS/swap,
Q&A request IDs/start/end/status/elapsed/context windows, final drain and audio
integrity evidence. For each failure append reproducible input/state, observed
wrong output/error, preserved material and retry/recovery result.

Final per-Mac decision: **NOT RUN**. Common configuration decision: **NOT RUN**.
Remaining work already known: real-source Zoom/Discord qualification and required
physical acoustic/latency/offline/network/lifecycle evidence. Windows, sync,
distribution and cross-meeting person recognition are explicitly out of scope.
No amount of portable test success or a hosted DMG alone closes #20.
