# macOS system audio idle correction

> Implementation uses independent agents for stream/mixer, ring-buffer wakeups,
> permissions and CI; the coordinating agent reviews and verifies integration.

**Goal:** Keep v0.2.18 recording alive without system callbacks and produce a
fork-only PR and an unsigned Apple Silicon candidate through Actions.

**Architecture:** Await the native source until data, explicit termination or
cancellation. Register/recheck the bounded capture buffer. Represent permission
probe evidence separately from silence; share that contract across IPC consumers.

**Tech stack:** Rust/Tokio/ringbuf, Tauri, React/TypeScript/Bun, GitHub Actions.

**Base:** `10f11baed5e0dbc9c1e885d22534a6dec19dbb57` (v0.2.18).
The clean checkout initially used `e091a7a99b0681553b8ef6bb6a8e4b3948f8a940`.
`fix/macos-system-audio-idle` and `local-fix-base-0.2.18` start at the requested
base; main is preserved. No `.codegraph/` is present.

## Constraints

Keep version 0.2.18, bundle com.meetily.ai, VAD, models, gain and source/You
semantics. No upstream PR, Release, updater publishing, Mac development installs,
Apple secrets or physical-device qualification claims. Preserve bounded buffers,
fatal real closure/rate mismatch and bounded five-second diagnostic probe.

## Tasks

- [x] Stream/mixer: factor a production handler accepting a synthetic stream;
  prove initial 60-second Pending and a 30-second gap fail the old timeout, then
  remove timeout. Cover zeros, cancellation/drop, actual None, rate mismatch.
  Test the partial 1024-sample block at a long gap, and only fix a demonstrated
  timing defect. Run existing pipeline continuity tests.
- [x] Capture buffer: deterministic ringbuf tests inject delivery between empty
  check and registration, after registration, and terminal transition. Preserve
  fast path and overflow limit; register and recheck before Pending; wake at end.
- [x] Permissions: introduce detected/conclusive/reason, frontend
  verified/denied/unknown and a versioned cache. Test silence, unknown exceptions,
  preserved verification, explicit denial, legacy cache, and meter cleanup.
  Update hook, warning/callers, PermissionsStep and AudioTestStep.
- [x] CI: fork pull_request/workflow_dispatch on macos-15 arm64, Node 24,
  pnpm 11.9.0, Rust 1.97.1, minimum macOS 14.2. Install native prerequisites,
  build metal llama-helper, serialize frontend build/tsc/Bun/native tests,
  build ad-hoc DMG and validate bundle/resources/architectures/smoke tests.
  Upload DMG, preserved app archive, checksums, patch and commit/tool provenance.
- [ ] Integration: review all diff, run available Cloud tests/builds, document
  actual results and limits in AUDIO_CALLBACK_CONTINUITY.md. Push fork branches,
  open PR to local-fix-base-0.2.18, attach it and inspect Actions for exact head.

## Review focus

Check cancellation without any new callback; tail samples before a long gap;
terminal wake with pending data; stale async permission responses after teardown;
verified evidence retained after a silent retry. These belong to the corresponding
production-handler and frontend lifecycle tests above.

## Verification commands

Frontend commands are separate, sequential processes: `pnpm run build`,
`pnpm exec tsc --noEmit`, isolated `pnpm dlx bun@1.3.10 test <file>`.
Native macOS: `cargo test --locked -p meetily --lib --target aarch64-apple-darwin
<FILTER>` for audio::stream, core_audio, audio::permissions, audio::pipeline.
Linux production-helper harness results do not establish macOS integration.
