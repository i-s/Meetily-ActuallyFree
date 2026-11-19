# Issue #23 — ordinary recording sources, existing controls

Live issue body/comments read 2026-11-19: [#23](https://github.com/i-s/Meetily-ActuallyFree/issues/23), no comments. Work is local on `meetily/ux-23`, based on verified `c05a32f64d703a29083186110b72a01f6eca1449`. No installed/published result is claimed.

## Owner direction and UI

The owner rejected the demonstrated source-pill and split-start-menu variants and explicitly directed reuse of the existing mic/output icon buttons and chevrons, then requested continuation. No extra source selector, banner, upgrade dialog or control is added. `RecordingSettings` no longer offers In-person; `RecordingControls` no longer hides the output lane behind that mode.

- Before Start, the existing icon toggles persist source inclusion. Start waits for successful pending source writes, including routed home/group/search/tray/notification/automation starts through `useRecordingStart`. Initial preference-load failure leaves controls disabled rather than visually claiming both sources are on.
- During recording, existing buttons mute included streams. An excluded source stays visibly off and cannot be unmuted into capture. Its tooltip explains Stop -> enable -> next recording. This change deliberately does not implement live stream addition.
- Native recording snapshots expose `microphone_captured` / `system_audio_captured`. The context fetches native state immediately after a Started event instead of temporarily showing an unmute-all default; reload restores the same selection.
- Chevron preview meters are restricted to their own source; a disabled source starts no preview. The optional `deviceType` command argument also prevents a default-mic preview from implicitly opening default system loopback. Legacy dual-source preview requests now also fail closed when either source is excluded or app-only capture is selected.

## Preference and capture ownership

`audio/recording_sources.rs` defines `RecordingSources`, flattened as `microphone_enabled` / `system_audio_enabled` in native preferences. Both default to true for ordinary old/unconfigured stores. Legacy `in_person:true` with no explicit modern system choice migrates to microphone enabled/system disabled. Explicit modern flags win over a leftover legacy value. Invalid types, including null, fail closed.

`load_recording_preferences` durably migrates only source keys and removes the legacy preference flag; unrelated/unknown store keys survive this upgrade. A failed migration write restores the cached previous value and fails visibly. Source inclusion is no longer a parallel room mode. Existing device names, gains and app targets are preserved.

`set_recording_source_enabled` is a source-only patch under short source-admission and preference-write locks (not the inference lifecycle lock). It refuses changes once Start owns an active/stopping take and emits successful durable changes across WebViews. Full device/gain/Settings writes preserve the current source selection, even if their incoming preference snapshot is stale. Source-only failures roll back the store cache and fence frontend Start until a successful explicit retry.

Both native start commands freeze the sources before resolving devices. Disabled source devices are not resolved, and the manager filters their stream arguments and per-app configuration again before capture. Both-off rejects. Missing enabled devices, empty chosen-app sets, macOS multi-app choices, and missing app processes remain fail-closed; there is no app-to-global fallback. Zoom/Discord source validation rejects computer-audio-disabled choices rather than broadening them. Real automation qualification remains unchanged/off.

Live unmute IPC rejects excluded sources. Reconnect also refuses excluded sources and refuses chosen-app reconfiguration, avoiding the older reconnect path's implicit restart as global capture. This is a deliberate fail-closed limit, not qualification of live app-device switching. Windows output-route monitoring is not started for deliberately excluded system audio.

## Identity and archives

New microphone-only recordings carry `microphone_identity_unverified:true`, with neutral microphone identity, without marking a new session `in_person`. Existing archived `in_person` metadata remains unchanged. No recording folder, audio, transcript, migration, snapshot or selected-version repository is rewritten by preference migration.

The existing archived room policy still excludes room speaker identification. Original-file and all relevant stored version restrictions continue combining conservatively; selecting an older version cannot grant identity. Text/timestamps/manual evidence remain under their existing owners. Neutral source labels do not establish acoustic identity. New anonymous source labeling is not qualified shared-room personal identification.

## Verification performed

All data is synthetic/generated. Existing cached Linux runtimes and dependencies were used; no owner account, recording, database or Mac was accessed.

| Check | Actual result |
| --- | --- |
| `python3 frontend/scripts/test-recording-sources-portable.py` | 22 passed: production serde/preferences loader/setter, durable-write rollback, stale full Settings saves, both actual native command device-setup blocks, manager pre-stream filtering/provenance, direct unmute guards and preview-device selection. Store/Tauri/device adapters are synthetic; this is not desktop linking or physical capture. |
| `python3 frontend/scripts/test-recording-start-authority-portable.py` | 39 passed: production authorization/settling paths with synthetic capture setup. Snapshot fixture now supplies the new active-source query. |
| `python3 frontend/scripts/test-meeting-native-integration.py` | 60 passed through mounted frontend and production Rust authorization over a JSON-lines adapter; negative cases intentionally log synthetic failure messages. |
| Bun `frontend/scripts/test-isolated.mjs` | 76/76 test files passed in separate processes, including source migration display, persistent icon toggles, pending/failed source writes, remount, native Started/reload snapshot, absence of In-person Settings and scoped preview. The integration file separately ran with its native adapter as above. |
| TypeScript `--noEmit --incremental false` | Passed; no Next/production build was run. |
| `python3 frontend/scripts/test-offline-identity-portable.py` | 47 passed with production identity/version code and generated SQLite fixtures. Initial run exposed a pre-existing non-Clone `DbManager` test adapter while the production command clones its manager; adding `#[derive(Clone)]` to that adapter restored compilation. No production identity code was changed. |
| `python3 frontend/scripts/test-recording-shutdown-portable.py` | 48 passed using generated audio and real cached FFmpeg, including durable source-track proofs and shutdown/save locking. |
| `python3 frontend/scripts/test-recovery-database-portable.py` | 17 passed, 68 filtered by that runner: generated recovery/save/Notes/version transactions and retained audio references. |
| Native source parse / `git diff --check` | Passed for changed Rust syntax and whitespace; not a full native desktop type/link check. |

Reproduction requires Bun 1.3.10, installed frontend dependencies, Rust/Cargo with the repository-locked dependencies cached for offline portable runners, and FFmpeg on PATH for shutdown/recovery. The test runners create private temporary fixture directories and never use app data.

## Remaining manual milestone — NOT RUN

Only after separate coordination and a qualified build/install:
1. Upgrade a backed-up test profile with old `in_person:true`, including saved app/device choices. Output icon is off; ordinary Start captures no system/app audio. Repeat via tray/group/search/notification routes and after app restart.
2. Explicitly enable output using the existing icon and verify the exact selected app/global source; unavailable app fails without global fallback. Check mic-only, system-only, both-on and both-off behavior, pause/Stop and WebView/minibar state.
3. Verify native Tauri command registration/event delivery and physical device capture, system permission behavior and source-scoped preview on macOS (and Windows as applicable). Live excluded sources require Stop before inclusion.
4. Open an archived in-person meeting: text, retained audio playback and versions remain available. Select an older version/reprocess and verify room identity exclusion remains visible and effective.

No full native desktop build/link, model inference, physical capture/identity, update/install/downgrade, DMG, Actions, push, release or independent agent review was performed. Author self-review is weaker than independent review. PR21 and other tickets/watch state were not changed.


## R1 — six independent-review corrections (2026-11-19)

Correction base: `cc79decbe9bc64ecb93a4892d084600bf17f077a`, same isolated branch. All six findings were checked against production code before editing. Existing two-icon direction is unchanged; contextual Retry is present only after a source-load error. No agents/re-review, other-ticket changes, physical-device use, private data, build/packaging/Actions/push/PR operations occurred.

### 1. One preference read/modify/write owner

Public `load_recording_preferences` now takes `PREFERENCES_WRITE`. Its locked helper is used only by callers already holding that mutex; there is no recursive acquisition. That owner covers source migration, read-triggered macOS folder repair, store save, rollback and returned snapshot. Folder repair patches only the folder key and fails visibly on persistence failure instead of returning a pretend-successful repair. Full saves/source patches use the same owner.

Deterministic RED at the old production loader: fake store pauses a legacy read **after** copying its migration snapshot; an ordinary source setter successfully disables microphone; resuming the old reader restores microphone. GREEN serializes the setter behind the reader. A second generated schedule injects a migration save failure and verifies its rollback cannot undo the later successful disable. These are controlled schedules, not random stress/physical timing claims. The actual macOS folder-repair RMW/save/rollback body also ran on generated paths with only the bundle-containment predicate adapted; no installed Mac app bundle was used.

### 2. Chosen-app preview is not endpoint preview

No app-scoped preview is implemented. Output chevron therefore starts no endpoint meter while chosen-app mode is selected (including empty targets), on both shared Windows/macOS paths. Global -> chosen-app, changed/empty target lists and unloaded scope invalidate the frontend preview effect. Cleanup sends native Stop immediately, even while the previous Start response is pending; a later valid preview waits for its teardown acknowledgement.

Native `simple_level_monitor` rechecks persisted inclusion/app policy under `PREFERENCES_WRITE` before stream open. Scope/source/target saves stop the old preview **and await its thread/stream Drop before persisting the new choice**; unrelated gain saves keep valid preview alive. Per-session running flags, generation fences, serialized session ownership and an awaited open acknowledgement replace detached threads sharing one reusable running bit. Late queued starts cannot revive cancelled capture. CPAL opening/DSP remains in the owned thread; join runs off Tokio. Scope changes and native Stop acknowledge stream destruction, not UI hiding. A driver hanging inside CPAL open/drop can still delay that acknowledgement; no physical latency bound is claimed.

RED against the actual old monitor (`--baseline-preview`) observes one forbidden endpoint open under chosen-app policy. GREEN observes zero. Generated stream adapters count actual open calls and live handles/Drops through the production monitor lifecycle. The fixture also covers old-start cancellation while queued behind the preference lock, scope transition teardown, target changes/empty targets, source exclusion, ordinary mic/global preview and unrelated gain saves. This establishes shared native admission/teardown, **not** real Windows WASAPI or macOS CPAL operation.

### 3. Reconnect direction

Reconnect matches name **and** `devices::DeviceType::Input/Output`, derived from requested monitor direction, before stopping/reopening any streams. RED opens the wrong hardware for reversed enumeration or the sole opposite-direction device. GREEN verifies both input/output and both list orders at the final synthetic stream-open boundary; opposite-only and chosen-app reconnect open no stream. Chosen-app reconnect still fails rather than globally falling back.

### 4. Source edits do not wait for inference

`recording_source_admission.rs` owns a short mutex plus RAII starting count. Both native Start functions acquire a starting reservation before `PRIORITY.recording()` and retain it through setup/commit; source patches serialize with that admission and reject starting/active/stopping. They never acquire engine lifecycle or background priority. Frozen source snapshots therefore remain authoritative while ordinary edits do not cancel ASR or wait for background inference to drain.

RED holds engine lock and polls an ordinary setter: old code stalls. GREEN completes that edit. Production `Priority` plus both actual Start admission/setup blocks are exercised with generated background work: click completes without cancelling its token, immediate routed Start cancels via recording priority while engine is held, late source patch rejects, release completes Start with system source still excluded. Dropped/failed start reservations release their starting count. This models the specified schedule only; untested schedules are not claimed.

### 5. Recoverable source loading

Hook exposes an error and retry on the same record card, only in error state. Initial failure keeps source icons/Start disabled. Successful retry or a fresh native/local source event clears the error. Revision guards reject late read responses/errors and responses from an earlier mount. Background/tray/notification starts remain subject to native source loading and fail closed on actual native load failure; a transient UI read failure is not a permanent global Start fence. Mounted tests exercise failure -> visible Retry -> success, disappearance of Retry, fresh event versus delayed read and preservation of existing source-write fences.

### 6. Minibar capture truth

Minibar initializes both inclusion flags to false until its first native snapshot, then uses frozen `microphone_captured`/`system_audio_captured` independently of mute events. Excluded source is off, has no active meter, cannot send unmute even through its callback, and the existing button's accessible label/title explains Stop -> choose sources -> start again. Included lanes keep ordinary mute/unmute. Mounted tests cover both source directions, delayed first snapshot/reload and normal included mute/unmute.

### R1 verification on final code

- Source/preferences/native-preview/reconnect/admission fixture: **35 passed** (production preference bodies, both command setup/admission blocks, real Priority, manager reconnect/provenance, native monitor lifecycle; synthetic store/stream/device transport).
- Native start authority: **39 passed**; cross-language normal/routed start/Stop: **60 passed**.
- Full isolated frontend: **77/77 files passed**. Targeted meter: 4; source hook: 3; source Controls: 4; minibar: 2.
- Standalone TypeScript `--noEmit --incremental false`: passed. Changed Rust syntax parse and `git diff --check`: passed. No desktop build/type-link.
- Archived identity/versions SQLite **47 passed**; generated FFmpeg shutdown **48 passed**; recovery/Notes/version SQLite **17 passed**, **68 filtered by its runner**.
- Expected negative fixtures print injected storage/capture/transport errors; no failing tests remain.

The original manual milestone above is still **NOT RUN**, now additionally covering physical chosen-app preview teardown, Windows duplex-device reconnect, macOS shared-preview applicability, contextual retry and minibar source truth. No physical Windows/Mac or real model inference result is claimed. This correction is author-verified, not a new independent review.
