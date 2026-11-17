# Issue #5: pinned Apple Silicon build and physical recording handoff

Issue: https://github.com/i-s/Meetily-ActuallyFree/issues/5

Status: **Actions artifact preparation; physical installation/capture gate blocked**.
No release is authorized. No agent may install or change data on the user's Mac.

## Source and Actions build

Starting base: `e091a7a99b0681553b8ef6bb6a8e4b3948f8a940`.
Reviewed local source: `bd782bfedabb2f1d5b8057cb89baa3ded1656b93`.
Git push with the runtime CLI credential returned 403. The supported GitHub App
imported the exact source tree as commit
`589f538793cf1c648cfce2e79807a23b7793c466`; both commits have tree
`ea192e5a5c6928f8b25890b57d21263c90e3fe6b`. Commit metadata differs, files do not.
The workflow checks out **589f538...**, and records it as `source_sha`; it never
claims to have checked out bd782bf. `original_local_sha` records that origin.
The workflow revision is a separate `workflow_sha`, obtained from `GITHUB_SHA`.

`.github/workflows/issue5-macos-artifact.yml` builds only this pinned source on
hosted Apple Silicon `macos-15`, using Rust 1.97.1, Node 24 and pnpm 11.9.0.
It installs OpenSSL 3, invokes the locked source build script (Metal sidecar
first, Tauri/Next/FFmpeg second), verifies the DMG and uploads an Actions
artifact. Build inputs record exact SDK, OS, tool versions, lockfile and bundled
model hashes. It checks sidecar startup and synthetic FFmpeg encode/decode;
it does not claim a physical microphone test, ASR quality, or installed-app test.

The new workflow is triggered only by a push changing its own file on
`meetily/issue-5-actions-build`, with an additional repository/ref job guard.
This permits a new workflow to run without adding it to main to enable manual
dispatch. It has contents-read permissions, no release environment, signing
secrets, release/tag writes, reusable publisher or deployment. The existing
main-only `build-macos.yml` / `macos-release` protection is unchanged. Before
each branch update recheck live workflow triggers and any PR auto-merge/queue.
There were no workflow_run triggers in the audited default/source workflows.

Download `issue5-macos-<run-id>-<attempt>` from the successful run, not from
GitHub Releases. It contains `Meetily-issue5-aarch64.dmg`, `SHA256SUMS.txt`,
`issue5-provenance.json`, `build-inputs.txt`, binary inspection and build logs.
Record the actual run URL/ID, workflow SHA, source SHA/tree, artifact id/digest
and the locally recomputed DMG hash. Check metadata and `shasum -a 256 -c
SHA256SUMS.txt` agree before mounting. Artifact retention is 14 days; preserve
the downloaded artifact locally. A failed run's diagnostics are not a DMG.

## User-reported physical preparation (not agent observation)

The owner reports a clean checkout of bd782bf on MacBook Pro M4 Pro / 48 GB,
macOS 26.2 (25C56): bundle hash matched, `git bundle verify` passed, 3 Python
checks / 26 isolated frontend files / TypeScript / Next build passed. Native
compilation stopped at cidre because full Xcode was unavailable. No DMG was
produced, installed app/data were unchanged, and permissions/capture/restart
were not checked. M1 Pro / 32 GB remains untested. **Do not fix Xcode or
continue local native compilation; Actions is the build route now.**

This is a repeatable build procedure, not a claim of bit-identical DMGs:
SDK/Homebrew versions, signatures/timestamps and downloaded/cached FFmpeg are
additional inputs. No application version bump, release or local native build
is needed for this task.

## Proposed same-account test boundary — owner decision required

No new macOS account, bundle identifier, application data path or migration
change is proposed. Merely renaming/copying the app or setting HOME does not
isolate it. First download/hash/mount/inspect the DMG without launching Meetily.
That phase leaves application data unopened. Installation, permissions and all
steps touching existing data below wait for the owner's decision.

Source evidence for shared state:

- `frontend/src-tauri/src/paths.rs:install_data_root` always resolves macOS
  storage to `~/Library/Application Support/Meetily`: SQLite (including WAL/SHM),
  models, settings, crash/session records and other app-managed files share it.
- `paths.rs:migrate_legacy_data` runs at startup (`lib.rs`) and copies known
  legacy data from Tauri's `app_data_dir`, identified by `com.meetily.ai`, when
  absent in the main root. Moving only one of these roots is insufficient.
- `audio/recording_preferences.rs` loads `recording_preferences.json` via Tauri
  Store and supports a custom `save_folder`; the default is
  `~/Movies/meetily-recordings`. Record both actual and default roots before
  proceeding. A new recording directory does not isolate database/settings.
- `database/manager.rs` opens the shared database and applies migrations during
  startup, before a user can choose a test folder. A separate .app path cannot
  protect an existing database from those writes.
- `lib.rs` registers a single-instance plugin and requests notification consent
  during setup. All existing Meetily instances/helpers must be stopped; use the
  explicit candidate executable path rather than `open -a` which may resolve
  the working app. Never run both versions concurrently.
- Frontend IndexedDB (`services/indexedDBService.ts`, startup recovery hooks)
  and localStorage (`ConfigContext.tsx`, transcript edits/theme/preferences)
  are persistent WebKit state. Source sets no custom WebView data directory.
  Identifier-scoped WebKit/cache/preferences locations must be inventoried on
  the actual Mac before launch; do not assume the two data roots are exhaustive.
- No direct Keychain/keyring calls were found in the app Rust source/dependency
  declarations. This does not establish absence of framework/plugin/OS state.
  TCC microphone/audio/notification grants are OS-managed and may be reused or
  changed for the same identifier/ad-hoc signature. File backups do not undo
  them. Keychain, TCC and LaunchServices are not isolated by a different folder.

Proposed reversible maintenance window, **not yet authorized to execute**:

1. Keep the working .app untouched at its current location, record its version
   and executable hash, and stop Meetily and its sidecars. Check for open DB/
   recording handles before copying. Do not kill unrelated audio applications.
2. Read-only inventory the two Application Support roots, actual/default audio
   roots, symlink targets, and identifier-scoped state in `~/Library/WebKit`,
   `~/Library/Caches`, `~/Library/Preferences`, `~/Library/HTTPStorages` and
   `~/Library/Saved Application State`. These are inventory candidates, not
   proven exact WebKit paths; report actual paths and stop if ownership is unclear.
3. With the app closed, take a dated local backup on encrypted storage of the
   entire two data roots (not only SQLite), all recordings/custom roots and the
   identified WebKit/preferences state, preserving ACLs/xattrs/resource forks
   (for example `ditto --rsrc --extattr --acl`). Include DB WAL/SHM if present.
   Hash source/backup files, inspect the copied DB read-only with `PRAGMA
   quick_check`, and verify restore space. Keep an owner-managed Time Machine/
   APFS backup as a broader recovery option; its existence/restore capability
   has not been verified here. Do not regard copying the .app as a data backup.
4. After approval and verified backups, hold originals of both Application
   Support roots and identified app-owned WebKit state aside on the same volume,
   preserving a path manifest and original absence/presence. Do not delete or
   merge them. macOS preferences/WebKit may have caching/background processes:
   confirm state is quiescent, otherwise stop. This is a rollback scheme for
   known files, not proven full OS isolation. Do not restore or reset TCC or
   Keychain databases programmatically.
5. Copy the candidate to a dedicated directory such as
   `~/Applications/Meetily-Issue5/Meetily - Actually Free.app` without replacing
   the working app. User launches that exact copy and handles Gatekeeper/TCC.
   Configure a dedicated empty test recordings folder before recording; keep
   summary/Q&A, cloud providers and auto-call recording off. Use only test data.
6. After recording/restart checks, quit candidate/helpers, preserve its test
   roots/results separately, then restore held originals to their **empty**
   original locations, including verified WebKit state; never overlay candidate
   migrations/settings onto old data. Restore initially absent paths to absence
   only after identifying them as newly generated test state. Reopen the original
   executable and verify its history, audio paths and settings against the
   pre-test inventory. If rollback fails, stop and use the verified backup.

Residual risk requiring explicit acceptance: TCC grants, notification consent,
LaunchServices registration, preference caches and possible framework Keychain
state may persist despite file rollback; the original app could need permission
regrant. No complete same-account isolation is established. If the owner does
not accept this bounded rollback/OS-permission risk, stop at read-only DMG
inspection; do not launch either candidate or a migration on working data.
The coordinator must obtain that decision and the actual-path inventory before
installation/data operations. No backup/move/install/TCC operation has run.

## Inspect the exact DMG before installation

Use the exact DMG downloaded from the Actions artifact. Create a local evidence
folder for inspection logs and set `EVIDENCE_DIR` to it. Set `DMG` to that file:

```bash
DMG='/absolute/path/to/the-new-candidate.dmg'
shasum -a 256 "$DMG" | tee "$EVIDENCE_DIR/dmg.sha256"
hdiutil attach "$DMG" -readonly -nobrowse
# Set APP to the mounted app path printed by hdiutil, then:
APP='/Volumes/REPLACE/Meetily - Actually Free.app'
for name in meetily llama-helper ffmpeg; do
  file "$APP/Contents/MacOS/$name"
  xcrun vtool -arch arm64 -show-build "$APP/Contents/MacOS/$name"
  otool -L "$APP/Contents/MacOS/$name"
  shasum -a 256 "$APP/Contents/MacOS/$name"
done | tee "$EVIDENCE_DIR/binaries.txt"
/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Print :NSMicrophoneUsageDescription' "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Print :NSAudioCaptureUsageDescription' "$APP/Contents/Info.plist"
codesign --verify --deep --strict "$APP"
codesign -d --entitlements :- "$APP"
"$APP/Contents/MacOS/ffmpeg" -version > "$EVIDENCE_DIR/ffmpeg-version.txt"
printf '%s\n' '{"type":"ping"}' '{"type":"shutdown"}' | "$APP/Contents/MacOS/llama-helper"
find "$APP/Contents/Resources/resources/diarization" -type f -exec shasum -a 256 {} \; > "$EVIDENCE_DIR/bundled-models.sha256"
```

Require ARM64 for all three binaries, Mach-O minimum no later than 14.2, plist
minimum 14.2, valid strict signature, microphone entitlement, sidecar `pong` and
`goodbye`, and the three bundled diarization files:
`segmentation-3.0-fp16.onnx`, `wespeaker-resnet34-LM.onnx`, `xvec_transform.npz`.
Inspect `otool -L` for unavailable absolute build-machine dependencies. These
manual checks must be recorded; their presence in this document is not a pass.
For full packaging/encode/decode checks see
[the existing release runbook](../.github/workflows/MACOS_RELEASE.md). Do not
run its publish steps for issue #5.

Do not launch or install yet. After the coordinator obtains approval for the
same-account boundary above, the owner installs only in the agreed candidate
folder and handles Privacy & Security → Open Anyway (ad-hoc signed, not
notarized), Microphone and Audio Capture permissions. No global Gatekeeper
or TCC disable/reset. Record grant/deny/retry and requested restart; play audible
media during the system-meter probe. Static inspection below is not capture.

## Models, privacy, and short recording

Choose local ASR in setup; record the exact engine/model/quantization and live
versus post-call settings. Download required ASR weights successfully before
testing offline. Bundled Pyannote/WeSpeaker resources are not ASR weights;
Nemotron and optional Whisper are separate downloads. Optional models need not
be activated to pass this build gate. Do not silently switch engines on failure.
Use a local model compatible with the test language and note failures honestly.

Keep summary/Q&A disabled for this gate. The pinned source has Claude CLI and
other providers; Codex CLI integration is a later issue. Do not substitute
Claude or an API key. All capture, ASR and diarization here remain local. Only
future text/Notes operations through subscription-authorized Codex are allowed
by the product requirement; no audio upload or paid API is part of this test.

Record Zoom and Discord versions from each installed client's About window,
even if only one is used for the short call (mark absent/not exercised explicitly).
Use only your own test phrases and a consenting test participant or a second
device playing your own prerecorded voice. Manual start/stop is sufficient for
#5; call automation, echo correction, 90-minute load and Codex are #6–20 scope.

Keep mic and system capture enabled throughout three short takes, with one
source intentionally silent in the first two. Use distinct meeting titles:

| Take | Input and expected observation | Evidence after Stop |
| --- | --- | --- |
| `issue5-mic` | System playback silent. Say “Микрофон: зелёный маяк, сорок два. Local microphone test.” | Own phrase audible in mic; listen to system separately; transcript text/time/source recorded. |
| `issue5-system` | User silent, headphones to avoid intentional acoustic bleed. Remote source says “Система: синий корабль, семьдесят три. Remote system test.” | Remote phrase audible in system; inspect mic separately, record any leakage rather than relabeling it. |
| `issue5-both` | Alternate own and remote phrases, then brief overlap; record output route. | Both source tracks and mixed playback playable; text/times and source labels retained. |

Wait for final ASR/save completion after Stop. In Settings → Recording record
the actual recordings root (default `~/Movies/meetily-recordings`) and ensure
audio auto-save is enabled before the takes. For each
meeting locate `audio.mp4`, `mic.mp4`, `system.mp4`; listen separately, note
missing/truncated audio and alignment. Silence in an intentionally silent
source is expected. `You` means microphone provenance, not proven identity.
A speaker number is not a person's identity. Speaker leakage is evidence for
#6, not something to hide by changing labels in this gate.

Export the transcript to Markdown before quitting. Record expected phrase,
actual text, time, source, meeting title/id, file sizes and hashes. Quit with
Cmd-Q, reopen the installed copy, find each meeting in the library, play all
three files again and export the transcript again. Compare text, timestamps
and source labels before/after (export metadata may differ). Pass only when
audio and transcripts remain accessible without recovery/re-recording.

```bash
# Set TAKE to the actual meeting folder; keep these artifacts private/local.
TAKE='/absolute/path/to/one/test-meeting'
APP="$HOME/Applications/Meetily-Issue5/Meetily - Actually Free.app"
for name in audio mic system; do
  test -s "$TAKE/$name.mp4"
  shasum -a 256 "$TAKE/$name.mp4"
  "$APP/Contents/MacOS/ffmpeg" -v error -i "$TAKE/$name.mp4" -f null -
done
codesign --verify --deep --strict "$APP"
```

Runtime data lives in `~/Library/Application Support/Meetily` (SQLite and models),
with plugin stores under `~/Library/Application Support/com.meetily.ai`.
Hash downloaded weights with
`find "$HOME/Library/Application Support/Meetily/models" -type f -exec shasum -a 256 {} \;`.
Record selected model names alongside these hashes. No writes should appear
inside the installed app. Do not commit databases, audio, model binaries or logs.

## Physical evidence record (currently unexecuted)

Fill one copy per Mac. A pass on M4 does not qualify M1, nor does a newer OS
qualify the 14.2 minimum. Attach sanitized text evidence to the coordinator;
retain recordings locally. Mark every missing check **not tested**, never pass.

| Field | M4 Pro / 48 GB | M1 Pro / 32 GB |
| --- | --- | --- |
| Physical date/operator, exact macOS/build | User-reported M4 Pro, macOS 26.2 (25C56); no capture | Not tested |
| Source SHA / dirty state / DMG SHA-256 / app version | Not tested | Not tested |
| Tool/SDK versions, binary architecture/deployment/dependencies/signature | Not tested | Not tested |
| Zoom / Discord versions; tested client | Not tested | Not tested |
| ASR/diarizer names, settings and weight hashes | Not tested | Not tested |
| Microphone / output devices; headphones/speakers | Not tested | Not tested |
| Permission prompts / deny / grant / retry | Not tested | Not tested |
| Three take ids, durations, track hashes and listening results | Not tested | Not tested |
| Transcript text/time/source before and after Cmd-Q/relaunch | Not tested | Not tested |
| Errors and limits (including other Mac) | User reports cidre/Xcode native build stop; Actions requested | Not tested |

Issue acceptance status:

- AC1: source pinned; actual macOS/tool/client/model versions **pending**.
- AC2: repeatable preparation/build/install instructions supplied; actual Mac
  execution and installability **pending**.
- AC3: source-track and restart protocol supplied; recording/persistence **pending**.
- AC4: M4 user-reported preparation only, second-Mac limitation explicit; Linux is not capture
  evidence. Gate remains blocked until at least the chosen physical Mac passes
  and the other Mac's untested limits are recorded.

Portable checks: `bash -n frontend/scripts/build-macos-apple-silicon.sh` and
`python3 -m unittest discover -s tests -p 'test_macos_build.py' -v`.
The Python tests run the public script with fake build commands to cover
sidecar-before-app ordering, locked dependencies, OpenSSL/deployment propagation,
missing headers and platform rejection. They do not compile/link/launch a Mac
binary, download weights, or exercise physical audio. TDD was not claimed:
no test seam had been pre-agreed; these are targeted regression checks.
