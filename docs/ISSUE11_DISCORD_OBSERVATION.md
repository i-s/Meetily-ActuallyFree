# Discord observation-only qualification protocol (#11 follow-up)

This is a procedure for the **single final combined Mac build**, not a report
that either Mac or Discord was operated here. Architecture replacement is **not
established as necessary**. The bounded native AX adapter is simply unqualified.
Its current cue shapes may be absent, unstable or insufficient; leave with the
client open is unresolved. Observation will distinguish those outcomes before
choosing whether to improve this adapter, add another authorized public signal,
or replace a component. No privileged RPC access or token workaround is an option.

## 1. Collection boundary and stable contract

The existing `discord-call-state` / `get_discord_automation_status` fields retain
their meanings. `enabled` means **observation enabled**, not app running, call
supported or recording authorized. `qualified` remains hard **false**. Additive
fields are `platform_supported` (reader compiled for this OS, not call support),
`client_present: true|false|null`, `recording_active`, finite `evidence` booleans
and `cue_transition`. Do not derive join from `client_present=true`, or supported
from `enabled=true`. `client_present=false` is observed absence of the Stable
main client, not a supported/active call; null means unavailable/not observed.
`state=active` is only a **candidate signal** and cannot capture while unqualified.
Disabled snapshots have Unknown state, no client assertion and cleared cue facts.
Windows reports reader unsupported; it does not inherit Discord support.

Only these evidence booleans survive classification/export:
`complete`, `connected`, `disconnect`, `reconnect`, `join`, `connected_pair`.
`connected_pair` requires connected/disconnect in the same window. Raw unrelated
AX labels are filtered before snapshot retention; no trees, message/participant/
channel text, screenshots, free-text notes, credentials or audio are logged. AX
attribute/error reasons are finite codes, not raw strings from Discord. A complete
scan losing a previous pair emits
`connected_controls_disappeared_unknown`, **not** confirmed leave. Cue appearance,
reconnect cue and debounced tracker `start/end/none` are recorded separately.
Permission, missing attribute, invalid type, IPC read, timeout and bounds failures
remain distinguishable Unknown reasons. An AX traversal that finds no supported
controls is Unknown, including ordinary open/idle; it is not a false “supported”.

Observation commands registered in the normal Tauri entrypoint:

- `start_discord_observation({scenario: "guild_voice"|"dm"|"group_dm"})`: macOS,
  explicit detection/observation opt-in and no running recording required. Refuses
  to overwrite an existing journal, even if stopped. Starts no audio or recorder.
- `mark_discord_observation({truth: <finite marker>})`: timestamps manually
  verified truth at native receipt on the **same monotonic clock** as the worker.
- `stop_discord_observation()`: freeze journal, preserve it for export.
- `get_discord_observation()`: local JSON snapshot, no native filesystem writes.
- `clear_discord_observation()`: explicit stopped-journal clear; UI confirms
  deletion. It is not an automation-enable override.

The worker owns a volatile, bounded 4096-entry journal, survives route/WebView
reload, and stops appending on overflow without evicting truth. Overflow sets
`truncated=true`; that run cannot establish qualification. App exit loses it.
Scan intervals are `[scan_from_ms, at_ms]`, relative to journal start; scans begun
before journal start are excluded. Manual truth events use `at_ms`. Seq numbers
and monotonic ordering are deterministic. UI has scenario/marker buttons and
export textarea; use normal select-all/copy/save to retain the JSON. It neither
uploads it nor requires clipboard, debug console, audio permissions or a helper.

Run observation with Labs automation **off** and no manual recording. The UI
blocks scenario Start while Labs is on, and native Start rejects active capture.
No observation command calls recording/capture. Discord's unqualified hard gate
also applies if Labs is enabled elsewhere. Existing manual recording and other
adapters are not disabled globally by this diagnostic tool; if another path starts
recording during a run, `recording_active=true` samples invalidate observation-only
acceptance. Stop that recording and repeat, rather than claiming no capture.
A short other recording entirely between polls is not proven absent by sampling;
keep all recording paths off during this protocol. The journal never stops or
silently discards a recording someone explicitly started.

## 2. Reproducible setup for the coordinator's one combined build

1. Integrate/review accepted changes normally; preserve the additive Discord
   namespace and do not map its Unknown into #10's media/end events. No direct
   Zoom branch integration is performed by #11. In the combined native build
   environment set `MEETILY_BUILD_SHA` to that combined source head, e.g.
   `MEETILY_BUILD_SHA="$(git rev-parse HEAD)" <the normal combined build command>`.
   This is a build stamp only, not a recording-unlock flag. Exports include native
   package version and this optional stamp. If omitted, `build_head=null`: retain
   the coordinator's exact source/build manifest; never infer it from version.
2. Record a nonprivate sidecar for each Mac/run: machine alias (`M1-Pro` or
   `M4-Pro`), macOS build, Discord Stable exact version, UI locale, combined head,
   installed app/binary checksum, permission state, route category, run ordinal.
   No names, channel IDs or call contents. Keep sidecar beside the exported JSON;
   filenames should be e.g. `M1-Pro-guild_voice-run1.json`. Run each class on each
   Mac separately; never label a DM result as guild or group-DM coverage.
3. Complete normal Meetily setup and recording notice. Settings → Meeting
   detection: master on, **Labs automatic recordings off**, **Observe Discord
   macOS call signals on**. This switch enables only observation. Check the UI's
   separate platform-reader, observation, client-presence, candidate-state/reason
   and unqualified-call-support text. No user qualification override exists.
4. Start with no Meetily recording and a fresh stopped/cleared observation
   journal. Select the true scenario manually and Start observation. Do not quit
   Meetily before Stop/export. Discord may be closed for the initial baseline.
5. Grant/revoke AX only when directed by the final human-operated test. No Mac
   permission, client, network, audio or user-data operation was executed here.

## 3. Exact manual-truth comparison sequence (repeat for each of 3 scenarios)

Markers describe the state **verified in Discord**, not a state guessed from
Meetily. Click the matching Meetily marker immediately after the action/visible
confirmation; times include human reaction delay. Record any unusual delay in
nonprivate sidecar timing notes; repeat if timing is ambiguous. Do not press
`joined` while only ringing. Do not press `left_app_open` before actually leaving.
For negative tests, require no hidden active call in another Discord window.

| Step | Human action and truth marker | Hold / expected comparison |
| --- | --- | --- |
| Baseline reset | Quit Discord (not Meetily); mark `client_quit` | Hold **60 s**; client absent, never candidate Active. This also drains the tracker's 45-s absence grace from a prior run. |
| Ordinary open/idle | Open Stable Discord without joining; mark `idle` | **20 s**; no Start. Unknown controls are acceptable unavailable evidence, not proof of support/inactivity. |
| Settings negative | Open voice settings; mark `settings`, then run mic test and mark `mic_test` | **15 s each**; neither must cause candidate join/Start. End mic test before continuing. |
| Ringing negative | Initiate or receive the chosen DM/GDM call but do not answer/connect; mark `ringing`, then cancel/reject and mark `idle` | **15 s ringing + 15 s idle**, if client permits holding ringing. For guild voice, use any corresponding not-yet-joined UI; if ringing is inapplicable record that fact in the sidecar and do not invent coverage. |
| Real join already muted | Actually join guild voice / answered DM / answered GDM while muted; verify connected, mark `joined_muted` | **15 s**; candidate pair/Start may be observed, but qualified stays false and NO recording starts. Lack of pair is a measured miss, not permission to broaden the rule. |
| Mute/silence/device/channel | Mark `unmuted`, `silence`, `muted`, `device_changed`, and `channel_changed` after each real verified action | **15 s each**. No tracker End/new session on mute/silence/device. For channel change without leaving call, preserve continuity; if the client necessarily leaves, mark actual leave/rejoin instead. Do not pretend a device/route was tested when unchanged. |
| Transient reconnect | With an actual connected call, cause a brief controlled interruption using an approved human method; mark `reconnecting` while Discord reconnects, then `reconnected` only on real return | Keep interruption **under 20 s**, then hold connected **20 s**; same session, no tracker End/second Start. No forced OS/network changes are automated by Meetily. Failed recovery is not `reconnected`. |
| Permission loss | While still in call, remove Meetily Accessibility access; mark `permission_lost`. Restore it, mark `permission_restored` | **20 s each**; lost access gives explicit Unknown, no End. Restoration must resume evidence or report Unknown; do not invent a positive sample. If an OS change requires restart, stop/export first and report that limitation as a separate run, not continuous recovery. |
| Leave with client OPEN | Actually disconnect/leave, keep Discord open; mark `left_app_open` | **65 s**; inspect candidate cue disappearance, Unknown reasons and tracker End absence. Current implementation is expected to MISS confirmed leave here; preserve that negative result. A generic Disconnect disappearing never becomes proof of leave by itself. |
| Rejoin without quitting | Rejoin same scenario muted; mark `joined_muted` | **15 s**; evaluate whether another true session is identified. Current Unknown retention may miss a new Start; report it, do not hide it by first quitting the app. |
| Real exit and grace | Quit Discord; mark `client_quit` | **60 s**; continuous observed client absence may produce tracker End after >=45 s. This is not evidence for leave-with-open-client. |
| Final idle | Reopen client without joining; mark `idle` | **20 s**; no join/recording. Stop observation, Export JSON, save it + sidecar, then Clear explicitly for next scenario. |

Repeat foreground/background/minimized windows and relevant locale/device routes
in separate identified trials; keep the same manual-truth method. At least two
cycles per class/Mac are required to reveal session rearm errors. Silence and mute
are real joined states, not absent/leave truth. `permission_lost/restored` retain
the preceding logical joined truth; `reconnecting` is still the same logical call.
These labels and hold times are test instructions, not evidence that they ran.

## 4. Deterministic offline comparison (no Mac interaction)

From the source checkout using standard Python only:

```sh
python3 -I scripts/discord_observation_compare.py /absolute/path/M1-Pro-dm-run1.json \
  > /absolute/path/M1-Pro-dm-run1.comparison.json
python3 -I -m unittest discover -s tests -p test_discord_observation.py -v
```

The comparator validates schema, finite scenario/truth, seq/time ordering and
scan bounds. It excludes samples whose scan began before the latest manual
marker plus **1500 ms** reaction allowance (change only explicitly with
`--tolerance-ms N`, retain N with report). Disabled/unsupported is counted as
unavailable, NEVER a false negative/supported call. Unknown is counted separately
for joined and not-joined truth, with exact finite reason frequencies.

It reports candidate false joins/inactivity, tracker premature End/false Start,
true join/leave edge latencies and **missing/censored** transitions. Join budget
is **10 s**, leave **55 s** (45-s grace plus polling margin); budgets are software
comparison thresholds, not measured accuracy or a promise of Discord behavior.
Short intervals are censored, not passes. Initial idle is baseline, not a demanded
End. It checks unqualified gate remains false and no sampled recording is active.
Missing markers are reported; inapplicable markers require sidecar explanation,
not guessed truth. Do not sum Unknown as “correct negative.”

CLI exit 0 means a valid finished comparison only; exit 2 indicates active export,
truncation or safety/transition violations. Missing leave, missing coverage and
Unknown can still yield exit 0: inspect the report, not its exit status alone.
Every report says `qualification=not_granted`, even a perfect synthetic result.
The native journal captures no free-text metadata. Comparison outputs/sidecars
are local acceptance artifacts, not committed private recordings/user data.

## Executed software checks (Linux, not physical qualification)

- Exact production Rust evidence/classifier/tracker core: **6 tests passed**;
  bounded native journal core: **3 tests passed**, including overflow, finite
  markers, stopped/old-scan suppression and shared relative timestamp behavior.
- Offline Python comparator: **6 tests passed**, including actual CLI invocation,
  Unknown/disabled separation, leave-open miss, permission/reconnect continuity,
  last-sample confirmed exit latency, censored intervals and safety violations.
- Isolated Bun 1.3.10 groups: observation UI **1 / 14 assertions**, normal settings
  **1 / 6 assertions**, normal recorder entrypoints **1 / 15 assertions**, Discord
  bridge **8**, generic handoff **5**; **16 frontend tests passed**. Tauri invokes
  and recording service are mocked in UI tests; native collection was not run.
- TypeScript no-emit check and `git diff --check` passed. Full Tauri/macOS FFI and
  command-wrapper compilation was not run. No physical source was qualified,
  no real audio/model tests, app install or combined build performed here.

## 5. Concrete promotion gate — code change, NOT user override

No setting, journal marker, selected scenario, environment variable or CLI output
currently grants qualification. The required evidence before promotion is:

- Exact combined head/binary identity and installed Discord/macOS/locale/route
  versions for both Macs; separate guild/DM/GDM truth and repeated-cycle exports.
- A documented read-only **positive join and positive leave-with-client-open
  contract**. It must discriminate ringing/settings/idle, muted join, background
  UI, channel/device changes, permission loss and transient reconnect. The current
  absent toolbar/Unknown pattern is NOT such a leave contract. Either validate
  a truly discriminating public AX shape or add an authorized public signal.
- Replays/regressions of the measured *allowlisted facts* through production
  classifier/tracker and ordinary recorder entrypoints, with no false joins,
  premature ends/session splits, missed validated leave/rejoin cycles, or capture
  while unqualified/unknown/disabled. Retain Unknown/negative controls. No raw
  private AX dumps or credentials should be retained just to broaden matching.
- Independent coordinator review of coverage and declared supported profile.
  If only one class is proven, enablement must have a reliable native class
  discriminator or remain disabled for all indistinguishable classes. The
  manually selected observation scenario is NOT such a discriminator.

**Actual code/config promotion step:** implement the proven positive leave rule
in `discord_automation/evidence.rs` and its bounded reader as needed; add a native
versioned qualification-policy function in `mod.rs` matching verified client/OS/
locale/class capabilities, returning false on missing/unmatched profile. Only
then replace the hard `qualified: false` production assignment with that function
and expose the exact supported classes/profile in status/UI. Extend tests so
unknown versions/classes/permissions and all existing negative fixtures stay
blocked at both bridge and recorder preflight. A blanket literal true, a changed
JSON setting or `MEETILY_BUILD_SHA` never qualifies an adapter. Normal master +
Discord opt-in + Labs switches can authorize recording **only after** that native
policy returns qualified true and the recorder's fresh active/session checks pass.

If trials show AX cannot provide that contract, choose the next authorized
mechanism with the coordinator. That is a decision supported by observations,
not a currently established need to replace Meetily's recording architecture.
