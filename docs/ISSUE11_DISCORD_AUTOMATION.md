# Issue #11: Discord macOS adapter and qualification gate

Live issue read on 2026-11-18 (body and comments endpoint, zero comments):
**[[macOS 07/16] Автостарт и автостоп Discord](https://github.com/i-s/Meetily-ActuallyFree/issues/11)**.
Assigned/implemented base: `4269de67c6747b2f5fce72596826c05dadbb3e1c`.

## Outcome: software path implemented, automatic recording NOT qualified

No actual Mac observations exist for this adapter. **No supported Discord call
class has been established.** The macOS worker always publishes `qualified=false`.
Both the frontend bridge and recorder's just-before-capture check require
`qualified=true`, so enabling observation or Labs automation does **not** capture
audio on speculative AX evidence. There is deliberately no user switch to bypass
this gate. Synthetic qualified fixtures test the software wiring, not Discord.
The mandatory auto-start/auto-stop requirement remains blocked, not replaced by
manual recording or claimed complete. Final combined-build Mac qualification is
still required; a trustworthy leave signal with Discord left open is unresolved.

| Call/client class | Implemented observation | Actual qualification |
| --- | --- | --- |
| Stable desktop guild voice | Bounded read-only AX candidate below | Not tested on either Mac; no supported claim |
| Stable desktop DM | Same candidate, if those controls exist | Not tested; ringing vs joined coverage unknown |
| Stable desktop group DM | Same candidate, if those controls exist | Not tested; reconnect/leave coverage unknown |
| Stage, PTB, Canary, browser Discord | Not selected | Unsupported by this adapter |
| Windows | No native Discord probe | Unsupported; not inherited from prior Windows CAM work |

The adapter cannot determine guild/DM/GDM from its controls; do not infer a call
type or participant identity. It never reads channel/participant/message text for
classification. English/Russian exact candidate strings are **hypotheses**, not
an assertion that the installed client exposes those labels or locales reliably.

## Research and prior fork evaluation

Read checkout AGENTS/development map, #7 signal and #9 recording notes, plus the
planning checkout's AGENTS, agent guidance, glossary, requirements, specification,
ADR-0001 and call-signal/repository research. #7 already records the absence of a
qualified automatic signal. Nothing in #9 qualifies a Discord lifecycle signal.

Re-inspected `origin/fix/codex-call-detection` at
`2169cabcac80a22f5a8fea6c54d13e8b971bb261`, its `meeting_detection/macos.rs`,
`evidence.rs` and `docs/CALL_DETECTION.md`. Reused **only** bounded AX read
primitives, CF ownership and conservative lifecycle ideas. That fork's macOS
adapter is Zoom-specific and reports a physical Zoom preview as Unknown even
with permission/visible controls. It supplies no qualified macOS Discord
implementation. No wholesale branch import, Windows signal port or privileged
Discord transport was made. The accepted base already contains the earlier
capture/checkpoint fixes; this work changes neither audio separation nor ASR.

| Previously known signal | Decision |
| --- | --- |
| Process/executable identity | Routes scans and can identify observed client absence; never join |
| Core Audio process I/O / mic/camera use | Not a join/leave contract: settings, ringing, mute, helpers and Meetily capture confound it; not consumed |
| Discord IPC/RPC | Public transport does not grant `rpc`/`rpc.voice.read` partner scopes; no entitlements assumed or RPC dependency added |
| Social SDK / Voice Gateway | Own integration/connection, not passive coverage of arbitrary installed-client calls; not used |
| AX UI | Available read-only OS interface with explicit Accessibility permission, but Discord semantics/version/locale/background coverage unqualified |

No self-bot, user token, private credential extraction, browser debugging,
client injection, packet inspection or chat-history scanning. No product-text
provider or paid API is added. All observation is local; capture/ASR remain on
the existing local paths. No Mac/user-data operations, packaging, installation,
publication, Actions, push or issue closure were performed.

## Actual entrypoints and integration interfaces

Native isolated owner: `src-tauri/src/discord_automation/{mod,evidence,macos}.rs`.
Only narrow shared changes: native module/startup/command registration, one
backward-compatible `MeetingDetectionSettings.discord_automation` boolean
(default false), a settings accessor and generic Discord fallback exclusion.
`stop_meeting_detection` temporarily pauses this worker too; normal restart
unpauses it. Generic/Zoom monitor restarts do not recreate the Discord tracker.

1. Native startup calls `discord_automation::initialize` once. Its lifetime
   worker polls every 3 seconds only when master detection and the Discord
   observation switch are on and Discord is not ignored. Disabled snapshots
   pause/reset candidate/grace evidence, not the active session. Exactly one
   blocking AX job is awaited at a time, independent of the capture thread.
2. Stable main desktop executable is selected by exact suffix
   `Discord.app/Contents/MacOS/Discord`, not Helper/name/mic matching. Process
   refresh requests executable identity only, not command lines/environment.
   Empty/unreadable process inventory and unreadable candidate identity give
   Unknown. No stable client gives Inactive, **never Active**.
3. Read-only CF/AX traversal is bounded: 700 ms shared scan budget, 50 ms per
   message target, 16 windows, 128 children per array, 768 nodes, depth 14,
   2 KiB strings. A message already in flight may overrun the shared deadline
   by its own timeout; OS scheduling and process enumeration are not hard
   real-time bounds. Copy/Create/retained CF references have one owner/release
   and never cross the blocking-thread boundary. Scans never request permission,
   click/activate a control, or open a window. No raw labels are logged/persisted.
4. Candidate Active requires exact enabled **Voice Connected + Disconnect**
   controls in the same window and a complete scan. Generic Disconnect alone,
   ringing, a client process, mic settings, Join Voice, mute/unmute, title text,
   separate-window matches and incomplete/limited AX reads cannot confirm join.
   Reconnect labels suppress positive evidence. Missing/hidden/unsupported UI
   gives Unknown; no connected-status text is read from messages or AXValue.
5. Production `Tracker` uses worker-supplied monotonic milliseconds. Candidate
   Active needs two observations at least 3 seconds apart. Inactive needs 45
   continuous observed seconds to end a session. Unknown resets both evidence
   timers and never ends an active session, no matter how long it lasts. A
   reconnect returning before confirmed end retains the same session. Device,
   channel, mute/deafen and silence are not independent lifecycle inputs.
   App epoch UUID + incrementing session number prevent PID/session reuse.
6. `discord-call-state` and `get_discord_automation_status` expose:
   `{enabled, state: active|inactive|unknown, reason, session_id, transition:
   start|end|none, qualified}`. No raw labels, participant IDs or audio. Repeated
   snapshots allow main-WebView reload reconciliation; settings display the
   signal, reason and limits even with Labs automation off. A native settings
   change is rechecked by the status command before authorizing a pending start.
7. The main root layout registers the separate Discord bridge. **Do not map
   Unknown to `meeting-ended` or set `active_media=true` for Discord.** Generic
   detection excludes Discord even if a custom keyword would match it. This
   event namespace is deliberately independent of concurrent Zoom work.
8. `lib/discord-automation.ts` checks the qualification gate, Labs preference,
   current native state and current recording state, then uses the ordinary
   `markAutomatedStart` → `launchRecording` → `useRecordingStart` path. The hook
   rechecks qualification/session/enablement after STT preparation and before
   `recordingService.startRecordingWithDevices`. Thus a revoked/stale request
   is not downgraded into a manual capture. The dedicated request marker also
   survives expiry of the generic pending handoff until consumed/cancelled by
   preflight; requests older than 60 seconds cannot authorize capture.
9. Automation-owned process keys are opaque `discord-macos:<epoch>:<session>`;
   they are **not capture target IDs** and never alter the user's audio source
   selection or broaden selected-app capture. Start still obeys #9's fail-closed
   source/model checks. Other-app and manually started recordings are not stopped.
   End uses `requestRecordingStop` (normal Stop/save), never a private capture
   command. Existing Pause/Resume/Stop and Don't stop it remain available.
10. A sessionStorage per-call claim is written before dispatch. Manual Stop,
    failed start, Don't stop it, route reload, mute/channel/device changes and
    disable/re-enable do not rearm the same detected call. Storage failure is
    fail-closed. A genuinely new confirmed session can start. Duplicate Stop
    requests are suppressed and ownership is rechecked across async awaits.

**Coordinator integration:** preserve the isolated Discord modules/event names.
Shared conflict surfaces with Zoom are `lib.rs`, `meeting_detection.rs`,
`layout.tsx`, `useRecordingStart.ts`, and `MeetingDetectionSettings.tsx` plus the
additive development-map link. Keep both adapters' settings fields when merging;
spread/round-trip settings instead of reconstructing only known fields. Do not
let generic process/media events adopt a Discord session. The existing
`meeting-automation` handoff format is unchanged. No capture/audio/diarization
interfaces are changed. A future shared lifecycle owner can consume this typed
snapshot, but must preserve qualification, Unknown, ownership and session claims.

## Acceptance criteria disposition

1. **Inventory explicit, physical verification deferred:** guild voice, DM,
   GDM individually listed above. None qualified; unsupported clients explicit.
2. **Software entrypoint wiring implemented/tested with synthetic fixtures;
   real join/leave blocked.** Ringing/process/mic alone cannot authorize capture.
   No qualified leave-with-client-open detector exists; missing UI stays Unknown.
3. **Conservative lifecycle implemented/tested:** mute/silence/device/channel
   do not split, Unknown/reconnect retain session, confirmed client absence has
   grace. Actual Discord behavior/version/route transitions not tested on Mac.
4. **Software controls/status implemented/tested:** qualification/Unknown reason
   visible, ordinary manual controls unchanged, same-session manual Stop obeyed.
   No claims about physical UI permissions or recording reliability on a Mac.
5. **No privileged RPC/token workaround:** satisfied by architecture/source;
   **Mac scenario not run**. This AC and the full issue remain open.

## Verification and remaining gates

Portable Rust tests compile the exact production `evidence.rs` with
`rustc --edition=2021 --test src-tauri/src/discord_automation/evidence.rs` (or normal
`cargo test --lib discord_automation` with project prerequisites). They test exact
controls, incomplete evidence, negative settings/ringing cases, same-window
constraints, mute/device/channel invariance, start debounce, unknown duration,
reconnect and leave-grace reset. These are device-free fixtures, not AX samples.

Frontend files must run in separate Bun 1.3.10 processes:

- `tests/lib/discord-automation.test.ts`: qualification gate, ordinary pending
  ownership, session suppression across reload/manual Stop, Unknown/reconnect,
  end-during-start, stale fresh-state checks, disabled automation, other/manual
  owners, new-session rearm and unavailable storage.
- `tests/hooks/discord-recording-entrypoints.test.tsx`: production bridge → real
  launcher → mounted `useRecordingStart` → recording-service boundary, and real
  `requestRecordingStop` event dispatch. Native/service/audio are mocked. Proves
  unqualified evidence cannot capture, qualified **synthetic** fixture can traverse
  the normal software path, changed evidence after model preparation cancels it,
  and manual Start remains available without adopting automation ownership.
- `tests/hooks/discord-settings.test.tsx`: mounted normal settings, native settings
  command, independent opt-in, live permission/Unknown reason and qualification UI.
- Existing `tests/lib/meeting-automation.test.ts`: unchanged generic handoff.

No actual audio, models, Discord IPC, Mac AX or private call fixtures are used.
No claim of full native/macOS build, installed app or physical acceptance follows
from these tests. Final measured results are recorded in the implementation
handoff, not inferred from the presence of tests.

### Executed results on this Linux worktree

- Exact production `evidence.rs`, Rust 1.85.1 standalone test compilation:
  **5 passed, 0 failed, 0 ignored** (final formatted source).
- Bun 1.3.10, separate processes: Discord bridge **8 passed**; mounted recorder
  Start/Stop entrypoints **1 passed / 15 assertions**; mounted settings **1 passed /
  6 assertions**; unchanged generic automation handoff **5 passed**. No failures
  in the final isolated run.
- An intermediate expanded entrypoint run hit Bun's default 5-second test timeout
  under shared-machine I/O contention (no assertion failure). Final hook tests
  explicitly allow 120 seconds and passed; this is not a Mac latency measurement.
- TypeScript `tsc --project frontend/tsconfig.json --noEmit` passed in both the
  initial and final verification runs (final process exit code 0).
- `git diff --check` passed. No full Tauri/macOS build or hardware/model/audio
  tests ran. The portable Rust invocation does not compile `mod.rs`/macOS FFI or
  the added generic-detector test inside the full native application.
- Dependencies were reused from the accepted #9 worktree through an ignored local
  `frontend/node_modules` symlink; isolated downloaded Bun/Rust runtimes stayed
  outside the repository. These are not published or portable test prerequisites.

For final combined Mac build on both M4 Pro/48 GB and M1 Pro/32 GB, record OS and
Discord versions/locales/permission status and separately exercise guild, DM and
GDM: idle, settings/mic test, incoming/outgoing ringing/cancel/reject, join already
muted, silence, unmute/deafen, channel/device switches, foreground/background or
minimized UI, permission loss, leave with client open, process quit, short and
long reconnect, manual Pause/Resume/Stop and Don't stop it. Collect nonprivate
state/reason/timestamp evidence; distinguish unobservable from observed leave.
Until a validated contract handles both join **and leave**, retain the hard
qualification gate. Merely changing `qualified` to true is not a completed fix.
