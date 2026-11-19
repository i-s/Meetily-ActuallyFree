# Ordinary recording sources Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. No subagents are permitted.

**Goal:** Remove new-recording In-person mode while preserving explicit capture choices and archived identity restrictions.

**Architecture:** A serializable native source selection migrates legacy preferences and freezes inclusion before capture. Existing idle icon buttons persist this selection; live buttons mute only included streams. Archived provenance remains a separate authority.

**Tech Stack:** Rust/serde/Tauri, React/TypeScript, Bun, generated portable Rust test crates.

**Spec:** `docs/superpowers/specs/2026-11-19-recording-sources-design.md`

## Global Constraints
- Only #23 on `meetily/ux-23`, based on verified `c05a32f64d703a29083186110b72a01f6eca1449`.
- No agents, push, PR21 changes/watch, Actions, packaging, account or owner-Mac operations.
- No transcript, database migration or archival metadata rewriting.
- Synthetic portable native tests and isolated frontend tests only.

## Review Focus
- Legacy true plus stale saved per-app target must not open system capture (Task 1/2).
- Immediate Start after source toggle must await successful persistence (Task 3).
- Unreadable/malformed source preferences must not broaden capture (Task 1).
- Source-off session must survive frontend reload with truthful disabled live controls (Task 2/3).
- Older transcript-version selection must not relax archived room identity policy (Task 3 verification).

### Task 1: Native source preference migration
**Files:** create `frontend/src-tauri/src/audio/recording_sources.rs`; modify `audio/mod.rs`, `audio/recording_preferences.rs`; create `frontend/scripts/test-recording-sources-portable.py` and native source tests.
**Interfaces:** `RecordingSources { microphone_enabled: bool, system_audio_enabled: bool }`, serde flatten into preferences; validation and source filtering reusable by manager. Legacy flag consumed on deserialize only, never emitted as a new preference.
- [x] Add failing serialization tests for default/legacy true/false/explicit override/malformed values and all-off validation.
- [x] Run portable tests and confirm failure before implementation.
- [x] Implement normalization; source-only durable setter serializes with native lifecycle admission and broadcasts successful changes.
- [x] Run tests, check source write failures remain visible and other preferences preserved.

### Task 2: Freeze native capture inclusion
**Files:** modify `audio/recording_commands.rs`, `audio/recording_manager.rs`, `audio/recording_mode.rs`, `audio/recording_start_sources.rs`; register source setter in `lib.rs`.
**Interfaces:** manager accepts `RecordingSources`; both commands use frozen sources before resolving devices. Snapshot returns included sources. Source-excluded unmute rejects; native microphone-only provenance is neutral without marking the session in-person.
- [x] Add tests for filtered mic/system/app inputs, missing selected source, all-off, automated-source rejection and neutral mic-only provenance.
- [x] Implement selection at both start paths; prevent reconnect from opening excluded streams.
- [x] Run portable source and start-authority tests with synthetic capture adapters.

### Task 3: Reuse existing controls and verify archives
**Files:** create source hook/client and hook tests; modify `RecordingControls.tsx`, `RecordingVoiceLane.tsx`, `RecordingSettings.tsx`, `RecordingStateContext.tsx`, `recordingService.ts`, `useRecordingStart.ts`, and affected regression tests; update feature note/development map.
**Interfaces:** source client queues writes and exposes `flushRecordingSources(): Promise<void>`; hook provides persisted flags, load/save/error state and serialized toggles. Shared start waits on it; native remains authority for every entrypoint.
- [x] Add failing mounted tests for upgrade state, persisted icon toggles, pending/failed write and reload/live-excluded behavior; replace obsolete Settings mode test.
- [x] Remove In-person switch/new-recording copy; wire existing buttons and tooltips without new controls.
- [x] Run source/control/launch/state tests separately, portable identity/version regressions and TypeScript check if local dependencies are available.
- [x] Document exactly what ran and remaining physical update/playback milestone; self-review diff and make local attributed commit only.

No extra owner choice or execution-method selection is needed: the owner explicitly requested continuation and prohibited agents. This plan is executed inline.
