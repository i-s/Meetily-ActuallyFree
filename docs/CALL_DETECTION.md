# Call detection qualification

Meeting Detection now distinguishes a running application from an ongoing call.
`meeting_detection.rs` collects per-application observations; the pure tracker in
`meeting_detection/evidence.rs` emits one offer after two consecutive positive
polls and an end after at least 45 seconds of continuously observed inactivity.
Unknown evidence never starts or ends a call. Default polling is 15 seconds.
Process/helper order and notification/interval edits do not create new sessions.

## Native evidence

- **Zoom on macOS:** bounded, read-only Accessibility inspection of Zoom's menu
  and all accessible windows. Enabled English/Russian Leave/End meeting controls
  confirm a call; generic Leave/End also requires Participants in the same window.
  Focus, microphone mute, and camera state are not requirements. A complete Home
  screen with Join/New Meeting and disabled Leave confirms inactivity. Hidden or
  unresponsive UI remains unknown. Accessibility permission is requested only by
  the explicit Settings button. No AXValue/chat bodies are read or stored.
- **Discord on Windows:** exact Discord/DiscordPTB/DiscordCanary process IDs,
  UI Automation call controls, and corroborating WASAPI capture-session state.
  Voice Connected plus Disconnect, or a call-specific Leave/End button, confirms
  a call. Microphone use alone (including a settings mic test) cannot start one.
  Muted/deafened calls can remain active from UI evidence. Missing accessibility,
  minimized/tray-only windows, unknown languages, partial scans and timeouts stay
  unknown. A single COM worker bounds caller waiting without spawning replacement
  threads for a stuck provider. English/Russian labels need physical qualification.
- **Other Windows apps:** existing microphone/camera consent-store evidence is
  retained, with browser meeting titles for browsers. This is weaker than the
  Zoom/Discord call controls and can still include an app's device test. There is
  no fallback from unavailable media evidence to process presence.
- Other macOS apps and Linux have no qualified call-evidence adapter. They report
  unknown instead of repeatedly treating an idle application as a meeting.

Diagnostics in Settings > Meeting detection inspect native state on demand and
show fixed explanations. They do not expose window titles, channel names, message
text or audio. The inspection does not advance the tracker's confirmation count.

## Offers and automation

Events and Windows notification actions carry a stable application key plus a
session ID. OS notifications retain the existing consent and Do Not Disturb gates. The frontend verifies
that the same call is still present before starting from an offer and stores the
session ID with automatic recording ownership. End events must match that
session; manually started recordings retain their existing ownership behavior.
Labs automation remains opt-in. `active_media` is retained for event compatibility
and now means confirmed call evidence for Zoom/Discord, including muted calls.
Changing application capture routing is outside this feature.

If a minimized Discord window stops exposing controls, an already detected call
is retained as unknown. Automatic stop currently requires exiting Discord. Disconnecting while Discord
remains open does not establish another session, so a later call may not produce
a new offer until Discord is fully quit and restarted. This favors keeping a recording over stopping it on missing evidence.
Keep Discord's window open for the first physical check; switching focus alone
should not require keeping it frontmost. Discord/Electron accessibility support
and UI labels vary by version.

## Verification and physical acceptance

Run `python3 frontend/scripts/test-meeting-detection-portable.py` with rustc on
PATH for production classifier/tracker fixtures. Native platform builds run
`cargo test -p meetily --lib meeting_detection`. Frontend regression files in
`tests/meeting-detection` must run in separate Bun processes. Portable fixtures
check decision rules; they do not demonstrate that a particular installed Zoom
or Discord version exposes the expected native controls.

Physical checklist for the Preview:

1. Enable Meeting detection, open **Check detection**, and on macOS explicitly
   grant Meetily Accessibility permission if requested. After replacing an ad-hoc
   signed Preview, macOS may require removing/re-adding its permission entry.
2. Leave Zoom/Discord open with no call for several polling intervals. Expect no
   offer. Settings and microphone tests must not create an offer either.
3. Join a real call. Allow two polls; expect one offer. Dismiss it and change the
   notification option: the same call must not repeatedly offer to record.
4. Switch applications and mute/deafen. Check the diagnostic explanation without
   expecting missing microphone activity to end a confirmed call.
5. Leave the call with its app still open. After readable idle evidence and the
   end grace, join another call: expect a new session/offer.
6. Test Labs auto-start/stop separately; ensure a manually started recording is
   never stopped by a detected call ending. For Discord also test minimize/tray
   and report Unknown results with app version and diagnostic reason.

No physical Zoom/macOS or Discord/Windows acceptance is claimed by these fixtures.
The second PR's native workflow results and artifact links record actual build
qualification separately. The bundle identifier and version remain unchanged;
Preview packaging is not a release or updater publication.

Windows Preview preparation exposes separate MSVC/CMake setup, LLVM download,
SHA-256 verification, extraction and libclang checks in Actions. Their timeouts
are 5, 15, 5, 15 and 2 minutes respectively. The shared LLVM bootstrap defaults
to the complete local setup; CI uses its individual phases with timestamped
diagnostics. Extraction rechecks the pinned hash before unpacking. A cached LLVM
installation skips archive work but still runs the libclang/version check.
LLVM extraction uses 7-Zip for both XZ and TAR layers because Windows' bundled
tar timed out on the checksum-verified archive. Each layer logs its boundary;
the temporary uncompressed TAR is removed after extraction (including errors).
Local bootstrap use requires 7-Zip on PATH or in Program Files/7-Zip.
The unsigned NSIS payload check accounts for Tauri's first `UNK` → `NSS` bundle
marker patch, then compares the entire executable byte for byte. All other
differences fail verification; resource and sidecar hashes remain exact checks.

Windows Preview now builds independent CPU and CUDA matrix jobs, with separate
Rust caches and artifacts. CUDA targets the requested RTX 3080 Laptop (`sm_86`)
using pinned CUDA 13.0.2 compiler/runtime components, without installing a driver.
The Preview packager stages CUDA runtime DLLs from that toolkit, follows their
PE dependency graph, rejects unresolved dependencies, and includes the toolkit
license. `nvcuda.dll` remains owned by the user's NVIDIA driver. The CUDA app ZIP
uses 7-Zip to support large CUDA DLLs. Installed executable imports must establish
CUDA/cuBLAS linkage; installed resources retain byte-for-byte hash checks.
Dependency fixtures cover transitive imports, cycles, missing DLLs/license,
system/driver exclusions and the CUDA 13 `bin/x64` layout.

Hosted runners can verify compilation, NSIS installation and CPU sidecars, but
cannot establish GPU startup, inference speed or real-call performance. Metadata
explicitly records this limitation. CPU native regressions run in the CPU job;
the CUDA job does not execute the CUDA-linked app on a runner without a GPU.
The CUDA backend accelerates **Whisper**. **Parakeet** has a separate Settings >
Labs > Parakeet on the GPU switch using DirectML for its encoder; enabling CUDA
does not change that preference. Test the CUDA Preview on the physical laptop
with a CUDA 13-compatible NVIDIA driver before treating acceleration as qualified.
