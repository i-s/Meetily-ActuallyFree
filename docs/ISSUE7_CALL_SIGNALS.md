# Issue #7: Zoom / Discord call-signal qualification kit

**Issue:** [[macOS 03/16] Проверка обнаружения звонков Zoom и Discord](https://github.com/i-s/Meetily-ActuallyFree/issues/7).
Live issue read 2026-11-18: open, zero comments. Scope is investigation and
software preparation for #7 only. No physical observations have been collected.
**Automation is not achieved or selected.** No app recording behavior is changed.
The owner wants one final DMG and batched physical acceptance after all tickets;
this kit does not trigger packaging, builds, releases or Mac operations.

## Prior work checked before implementation

- Base `56eb2f9c984c5c03176a7576b3cfe654b2255fd0` includes the reviewed #5
  idle-audio fix. See [MACOS_IDLE_AUDIO.md](MACOS_IDLE_AUDIO.md). A silent tap
  remaining alive is not a call signal, nor is its shutdown proof of leave.
- Local fork ref `origin/fix/codex-call-detection` at
  `2169cabcac80a22f5a8fea6c54d13e8b971bb261` contains the prior candidate:
  `meeting_detection/evidence.rs` conservative tracking, `macos.rs` bounded Zoom
  AX inspection, and `docs/CALL_DETECTION.md`. Commits `d34f263` and `55b4dd7`
  add/further diagnose the adapter. Its qualification note explicitly reports
  Zoom **Unknown on a physical preview** despite granted AX, English UI and
  visible toolbar. It has no qualified macOS Discord adapter. Do not import the
  165-file branch wholesale or mistake its Windows Discord support for macOS.
- This checkout's `meeting_detection.rs` still has process-name fallback on
  macOS/Linux; persisted Discord keywords are removed. It offers notifications
  for running apps, not the required observed join/leave. This kit neither
  promotes that fallback nor silently enables it. Leave detection/automation off
  in the installed app during signal-only investigation.
- Planning requirements, specification and `macos-call-signals.md` were read in
  the separate planning repository. HAL process I/O is a hypothesis; AX client
  controls are another hypothesis. Zoom Meeting SDK callbacks cannot be assumed
  to attach to an arbitrary installed Zoom; Apps speaker callbacks have role
  restrictions. Discord partner-only scopes are not unlocked by a user consent
  dialog, tester registration, or the existence of the IPC transport.

## Software and ownership

- `scripts/macos-call-signals.swift`: one read-only native snapshot using public
  Core Audio C properties: process object list, PID, bundle ID, input/output I/O
  flags, default input/output device object IDs and per-property OSStatus.
  No audio device is opened, no audio samples are accessed, and no AX permission
  prompt, token, RPC, process launch/control or network request is made.
- `scripts/macos_call_signals.py record`: one local journal writer; repeatedly
  invokes the separately supplied probe with a 3-second timeout. A daemon stdin
  reader queues ground-truth markers; it does not own native audio. Default
  interval 500 ms, bounded duration 15 minutes (maximum 2 hours). Explicit quit,
  stdin EOF, duration, or interruption ends collection. Output is exclusive-create
  with mode 0600, never silently overwrites a prior journal. Native process
  failures produce unknown samples; stderr/private paths are not retained.
- `evaluate`: portable deterministic replay of **unqualified** I/O-only rule
  (input OR output), per app. Start needs 2 positive samples; stop needs 45,000 ms
  uninterrupted negative observations; gaps over 1,500 ms and unknowns reset
  confirmation/grace without stopping. These are comparison knobs, not measured
  production parameters or a proven detector. Ground truth never drives the
  candidate. Mute, silence, reconnect and preview are truth labels, not APIs.
- Exact bundle lists identify known app/helper processes; no process-name
  substring guessing. Explicit companion PIDs are excluded from aggregation.
  Unknown identity or selected-process property failure makes that sample
  unknown. Helper PID rotation within a mapped bundle does not change identity.
  All HAL rows are retained locally so missing helper mappings can be reviewed.
  No bundle mapping claims to be exhaustive; attribution must be checked against
  installed clients. Default-device IDs are observations, **not** necessarily
  Zoom/Discord's actual selected devices, nor stable IDs across boots.

This is intentionally outside the recording path: no callbacks, inference,
models, Tauri commands, database/schema, frontend automation or source labels
change. It cannot self-start capture, and does not modify #6's double-talk path.

## Portable checks (can run now)

```sh
python3 -I -m unittest discover -s tests -p test_macos_call_signals.py -v
python3 -I scripts/macos_call_signals.py --help
python3 -I scripts/macos_call_signals.py evaluate /private/local/session.jsonl
```

Synthetic fixtures establish property-failure handling, exact identity, explicit
self exclusion, helper replacement, continuous negative grace, sampling gaps,
truth isolation, mic-test/connected signature collisions, false starts/stops,
latency bookkeeping and uncovered states. They do **not** establish real HAL
flags, native ABI/SDK compatibility, app capabilities or physical reliability.
Swift source is uncompiled on this Linux workstation. A compatible macOS SDK
compile gate must occur in the final combined **CI preparation** before supplying
an executable probe. Do not ask the owner to do a local native build.

Example future CI-only command (not executed here, not a workflow trigger):

```sh
xcrun swiftc -O -framework CoreAudio -framework Foundation \
  scripts/macos-call-signals.swift -o /CI/artifacts/macos-call-signals
```

Deployment availability and C property constants need a compatible SDK, with
runtime macOS 14.2+; an API missing on a target is recorded as unsupported, not a
false negative. The probe is an optional standalone diagnostics artifact, not a
DMG or an application install. Do not add a build trigger for this ticket.

## Batched physical session — pending, do not execute during preparation

After **all** software tickets and independent review, the parent supplies one
final app candidate and a CI-compiled probe. #5 physical stability is a dependency
for companion-capture interference tests. No standalone DMG request for #7.
Use only consenting participants/test accounts; no private meetings or cloud ASR.
This probe records metadata only. It does not need real speech to classify I/O.

### Inventory and manifest

Create a private folder outside the repository; retain journal, exact manifest,
probe SHA-256, app/source SHA, and a human-readable outcome sheet together. Record
actual macOS build, app versions, permission state (mic/system audio/AX separately),
UI language, machine, power mode, per-app input/output device settings, default
route and transport. Repeat independently on M4 Pro/48 GB and M1 Pro/32 GB; a
success on one is not evidence for the other.

Example **template only**, replace every `FILL` and verify the bundle mappings;
copy it to `/private/local/manifest.json` outside git. Defaults below are common
main-client identifiers, not an assertion about installed helper identities:

```json
{
  "machine": "FILL: M4 Pro or M1 Pro; RAM; power mode",
  "macos": "FILL: product version and build",
  "zoom_version": "FILL",
  "discord_version": "FILL: stable/PTB/Canary and version",
  "meetily_head": "FILL: final app source SHA and artifact hash",
  "permissions": "FILL: mic/system audio/AX granted, denied, not requested",
  "route": "FILL: physical devices and per-app input/output; default route",
  "discord_call_type": "FILL: guild voice, DM, group DM; stage if applicable",
  "bundles": {"zoom": ["us.zoom.xos"], "discord": ["com.hnc.Discord"]},
  "companion_pids": []
}
```

Verify helper bundle IDs/PIDs using the raw journal and Activity Monitor; include
only verified Zoom/Discord helper identities, and list Meetily/native helper PIDs
under `companion_pids`. Preserve the initial journal and corrected mapping as
separate files. PID reuse/restart requires updating this inventory and starting a
new session. Exclusions are not a substitute for proving attribution. Unmapped
helpers are a limitation, not proof that the app has no I/O.

```sh
python3 -I scripts/macos_call_signals.py record \
  --probe /provided/macos-call-signals --manifest /private/local/manifest.json \
  --output /private/local/session-01.jsonl
# Type markers, e.g. zoom idle; zoom preview; zoom connected_muted; ...; quit
python3 -I scripts/macos_call_signals.py evaluate /private/local/session-01.jsonl \
  > /private/local/session-01-report.json
```

Markers are manually observed **actual** state, not clicks predicting a join.
The timestamp is monotonic collector receipt; `input_at_ms` retains the stdin
receipt before the probe completes. Probe duration and polling bound uncertainty;
human reaction delay remains unmeasured unless separately calibrated. These are
observed marker-to-sample delays, not sample-accurate vendor lifecycle latency.
No raw application labels, window titles, chat text or audio are needed.

### Matrix and sequence

Start from an idle marker and end with an observed leave and at least 60 s idle,
not by quitting the app. For each client/call type, repeat at least five full
join/leave cycles per machine and route. Hold each negative state for >=60 s;
hold muted call and silence >=60 s to exceed the trial end grace. Include:

1. App launch without a call; idle with Meetily off, then with its **manual**
   capture running. Start/stop the companion capture while client stays idle.
2. Audio settings, mic test, Zoom preview, Discord ringing/cancel/reject, Zoom
   waiting room; no connected call. Opening settings is not joining.
3. Actual connect already muted; talking; silence; mute/unmute; Discord
   mute/deafen; focus change, hidden/minimized windows. These do not end calls.
4. Temporary reconnect/network interruption (mark reconnect while the logical
   call remains owned); success and failure/actual leave are distinct. Do not
   force stop merely because input/output flags disappear.
5. Built-in speakers/mic, wired headset, Bluetooth, USB if actually available;
   switch selected devices while connected and while muted. Record per-app and
   system defaults separately. Missing routes are not marked passed.
6. Helper PIDs before/during/after transitions, natural helper replacement if
   observed; deliberate restart only with explicit test approval. Companion
   own I/O on/off while connected, then leave while still muted and keep client
   alive; rejoin to verify session rearming. No killing private app processes.
7. Discord guild voice, DM, group DM separately; stage calls only if applicable
   and tested. Stable/PTB/Canary versions separately if used. No coverage is
   currently claimed for any call type. Record denied/unavailable integration
   scopes; do not obtain user tokens, self-bot, bypass restrictions or assume
   partner-only RPC access. Zoom ordinary participant in another account's
   meeting must not inherit proof from a host test.

Do not issue a ground-truth stop during mute, silence or transient reconnect.
Log route changes/companion toggles/PID changes in the outcome sheet with journal
milliseconds and start a new manifest/session for inventory changes. Compare the
full raw process list, not just the boolean aggregate, at each transition.

### Evaluation and decision gate

The report includes per-state I/O signature counts, ambiguous signatures shared
by connected/noncall states, unknown counts, uncovered states, candidate false
starts/stops and truth transition delays (`null` means unmatched, not zero).
An initially connected session has no measured join delay; always begin idle.
Repeated markers with identical lifecycle truth do not create fake join/leave.
For stop grace trials, keep sampling until after grace, or report right censoring.
End-of-file is never evidence of a stop. Reports are descriptive, never a PASS.

Compare at least start confirmations 1/2/3, stop grace 5,000/15,000/45,000 ms
using CLI overrides. Report event counts and trial counts, missed starts/stops,
unknown/censored intervals, and min/median/p95/max marker delays with manual
uncertainty. Do not choose thresholds from a different machine/route or omit
false stops on mute/reconnect. The planned sample size is not a statistical
reliability guarantee. **Production numerical bounds remain TBD by these
measurements**; the 500/1,500/45,000 ms defaults are experiment configuration.

If the same I/O signature occurs in mic test/preview and connected calls, delay
alone cannot prove their distinction. Reject I/O-only automatic lifecycle if the
mandatory matrix cannot be distinguished; document counterexamples. Next candidate
is read-only AX investigation using the prior fork's bounded diagnostics, with
explicit permission and version/language/layout coverage, not blind reliance on
its existing label rules. No vendor access entitlement is currently proven.
If no available mechanism passes, report **required automation not achieved**
and escalate the dependency/architecture decision. Manual offers do not satisfy
this issue or silently revise the automatic start/stop requirement.

## AC / evidence status

- Per-client full transition journal: collector and truth vocabulary prepared;
  all real journals pending.
- Device, helper PID, own-capture checks: raw metadata and exclusions prepared;
  attribution and physical checks pending.
- Discord types/version/permission limits: explicit inventory/matrix prepared;
  no call type or RPC eligibility claimed supported.
- False transitions/delays/production bounds: replay/report tested synthetically;
  measurements and defensible production bounds pending.
- Verified mechanism or automation gap: gap explicitly retained; no selected
  mechanism, automatic recording integration or AC completion claimed.

No recordings, native binaries, private journal, models, databases or artifacts
belong in commits. Audio/ASR remain local and are not invoked by this kit. No Mac
operations, builds, installs, releases, pushes, merges or issue closure occurred.
