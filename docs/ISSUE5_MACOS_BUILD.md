# Issue #5: pinned Apple Silicon build and physical recording handoff

Issue: https://github.com/i-s/Meetily-ActuallyFree/issues/5

Status: **prepared locally; blocked on physical Mac build/install/recording**.
No DMG was built or installed in this work. No release was published. Neither
M4 Pro / 48 GB nor M1 Pro / 32 GB has been tested. Linux checks below establish
script orchestration only, never macOS capture or ASR quality.

## Source and build preparation

The inspected starting source and merge-base with `origin/main` are both
`e091a7a99b0681553b8ef6bb6a8e4b3948f8a940`. This identifies source, **not an
installed DMG**. Build the final issue-5 commit (record `git rev-parse HEAD`),
which includes the local-script fix; keep the starting SHA separately. Do not
substitute a published preview based on matching version strings.

Use a disposable macOS account for qualification: this app shares its data
roots and bundle identifier with existing installs. Do not replace a working
installation or open its live database for this test. The owner performs the
installation below; full-stop does not authorize the agent to install it.

On a physical Apple Silicon Mac, macOS 14.2 or newer, native ARM64 Terminal:

1. Install Xcode Command Line Tools (`xcode-select --install`), Homebrew, native
   Rust/rustup, Node 24, and pnpm 11.9.0. The current candidate workflow uses
   Rust 1.97.1; use that toolchain for this reproduction. Record exact patch
   versions and SDK, rather than assuming the workflow proves a local build.
2. `brew install cmake pkg-config openssl@3`. Builds/downloads require network
   access; allow disk space for dependencies, Rust output, DMG, ASR weights and
   recordings. Neither 32 GB nor 48 GB RAM alone proves runtime performance.
3. Clone into a new folder. Fetch the task commit from the coordinator's branch
   or import its local Git bundle; check it out detached. No checkout/reset of
   the planning workspace is needed. Check `git status --short` is empty.
4. From that checkout, run the following. Use `set -o pipefail` so a failed build
   cannot be hidden by `tee`. Keep evidence outside Git and private recordings
   out of PRs/issues.

```bash
set -euo pipefail
rustup toolchain install 1.97.1 --profile minimal
rustup override set 1.97.1
export MACOSX_DEPLOYMENT_TARGET=14.2
export PNPM_CONFIG_ENABLE_GLOBAL_VIRTUAL_STORE=false
EVIDENCE_DIR="$(mktemp -d "$HOME/meetily-issue5.XXXXXX")"
{
  git rev-parse HEAD
  git status --short
  sw_vers
  uname -m
  system_profiler SPHardwareDataType | sed '/Serial Number/d; /Hardware UUID/d; /Provisioning UDID/d'
  xcode-select -p
  xcodebuild -version || xcrun --version
  xcrun --show-sdk-version
  rustc -Vv
  cargo -V
  node -v
  pnpm -v
  brew list --versions cmake pkg-config openssl@3
  shasum -a 256 Cargo.lock frontend/pnpm-lock.yaml
} > "$EVIDENCE_DIR/environment.txt" 2>&1
(cd frontend && pnpm install --frozen-lockfile)
python3 -m unittest discover -s tests -p 'test_macos_build.py' -v
(cd frontend && pnpm dlx bun@1.3.10 scripts/test-isolated.mjs)
(cd frontend && pnpm exec tsc --noEmit)
bash frontend/scripts/build-macos-apple-silicon.sh 2>&1 | tee "$EVIDENCE_DIR/build.log"
(cd frontend && pnpm exec tsc --noEmit)
printf 'Evidence: %s\n' "$EVIDENCE_DIR"
```

The script rejects Linux/Intel/Rosetta, configures OpenSSL 3 for the Metal
`llama-helper`, builds/stages that sidecar first, then invokes Tauri with locked
Cargo dependencies. Tauri runs the Next build and merges `tauri.macos.conf.json`
(minimum OS 14.2, DMG, templates and diarization resources). `build/ffmpeg.rs`
downloads/caches FFmpeg for the target. Both `llama-helper` and FFmpeg must be in
the final bundle. Do not run native builds/tests embedding `frontend/out`
concurrently with Next builds, or standalone typecheck during a Next build.

This is a repeatable procedure, not a claim of bit-for-bit reproducible DMGs:
SDK/Homebrew versions, timestamps/signatures, cached/downloaded FFmpeg and
runtime model downloads are additional inputs. Record their hashes. No version
bump or publication is needed for a development rebuild.

## Inspect the exact DMG before installation

Find the new DMG under `target/aarch64-apple-darwin/release/bundle/dmg/`.
Set `DMG` to that exact file (do not accidentally select an old candidate):

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

In the disposable account, copy the mounted app into that account's
`~/Applications` (create it if needed), eject the DMG and launch that installed
copy. Keep the working app in `/Applications` untouched. Ad-hoc signing is not notarization. If
Gatekeeper blocks it, use Privacy & Security → Open Anyway for this known
candidate; do not disable Gatekeeper globally. Grant Microphone and Audio
Capture (wording varies by OS) and retry the meters while audible media plays.
Record grant/deny/retry behavior and any requested app restart. Do not treat a
silent system meter as proof of permission while nothing is playing.

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
APP="$HOME/Applications/Meetily - Actually Free.app"
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
| Physical date/operator, exact macOS/build | Not tested | Not tested |
| Source SHA / dirty state / DMG SHA-256 / app version | Not tested | Not tested |
| Tool/SDK versions, binary architecture/deployment/dependencies/signature | Not tested | Not tested |
| Zoom / Discord versions; tested client | Not tested | Not tested |
| ASR/diarizer names, settings and weight hashes | Not tested | Not tested |
| Microphone / output devices; headphones/speakers | Not tested | Not tested |
| Permission prompts / deny / grant / retry | Not tested | Not tested |
| Three take ids, durations, track hashes and listening results | Not tested | Not tested |
| Transcript text/time/source before and after Cmd-Q/relaunch | Not tested | Not tested |
| Errors and limits (including other Mac) | No physical access | No physical access |

Issue acceptance status:

- AC1: source pinned; actual macOS/tool/client/model versions **pending**.
- AC2: repeatable preparation/build/install instructions supplied; actual Mac
  execution and installability **pending**.
- AC3: source-track and restart protocol supplied; recording/persistence **pending**.
- AC4: neither Mac tested, second-Mac limitation explicit; Linux is not capture
  evidence. Gate remains blocked until at least the chosen physical Mac passes
  and the other Mac's untested limits are recorded.

Portable checks: `bash -n frontend/scripts/build-macos-apple-silicon.sh` and
`python3 -m unittest discover -s tests -p 'test_macos_build.py' -v`.
The Python tests run the public script with fake build commands to cover
sidecar-before-app ordering, locked dependencies, OpenSSL/deployment propagation,
missing headers and platform rejection. They do not compile/link/launch a Mac
binary, download weights, or exercise physical audio. TDD was not claimed:
no test seam had been pre-agreed; these are targeted regression checks.
