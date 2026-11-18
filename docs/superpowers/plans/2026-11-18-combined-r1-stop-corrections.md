# Combined R1 Stop corrections implementation plan

> **For agentic workers:** Use superpowers:executing-plans; implement locally without delegates.

**Goal:** Fix the two reviewed P2s on `f6a9525` without changing manual recording or qualification gates.

**Architecture/design:** Native source publications and permission revocations update owner-bound Stop evidence under the recording authority mutex. Exact-lease claim requires a confirmed End for that owner and an unrevoked take; source/Labs disable or qualification loss permanently revokes automatic Stop for the take, while Unknown only removes exit evidence. A competing session cannot lend its End to an older owner. Controls retain a validated routed Stop through initial state synchronization and forward it once without substituting the current lease or manual intent.

**Tech Stack:** Rust/Tokio/Tauri, React/TypeScript/Bun, existing portable Rust/JSON-lines bridge.

**Spec:** User's Combined R1 request and `docs/ISSUE10_11_INTEGRATION.md`.

## Global constraints

- Same worktree; no delegates, heavy builds, DMG, Actions, push or Mac operations.
- Explicit manual Stop stays unguarded; malformed automation never becomes manual.
- Real Zoom/Discord remain unqualified; generated fixtures do not establish physical/IPC/audio qualification.
- Preserve source-specific settings, journals, native lease and existing save path.

## Approaches

A frontend-only recheck cannot serialize revocation with native claim. Reading adapter locks from inside authority would invert current lock ordering. Use bounded source evidence published into the existing native authority, not a second recorder or JS authority. Keep routed requests durable until dispatch rather than rebinding to a later native lease.

## Review focus

- Disable/re-enable and qualification loss cannot restore old take's Stop authority.
- End A followed by Unknown/active B cannot stop A using stale exit evidence.
- Rejected queued Stops have zero save/capture/success/manual-callback effects.
- Initial false Controls state and remount preserve the same routed request.
- Event/storage collision, malformed/missing lease and explicit manual intent remain distinct.

## Task 1: Native Stop-specific authority

Files: `recording_ownership.rs`, source lifecycle/status publishers, `meeting_detection.rs`, native fixture generator/RPC and Stop tests.

- [x] Add queued bridge reproductions for both source disables with Labs enabled; run red against existing production claim.
- [x] Add source observation/permission publication to the same authority mutex as `claim_stop`; bind confirmed exit and sticky revocation to current take. Publish real tracker End identity, not active-call start validation or an inferred absence.
- [x] Serialize settings publication with revocation; exercise disable/re-enable, qualification loss, Unknown, competing sessions, keep-recording, stale generation and explicit manual Stop.
- [x] Run portable authority/Stop/lifecycle suites and cross-language regressions; all assertions pass with no rejected-request effects.

## Task 2: Routed Stop ownership through Controls sync

Files: `RecordingControls.tsx`, `recording-launch.ts`, combined bridge and focused Controls tests.

- [x] Reproduce route A from `/library`, mount real Controls false, synchronize true; assert exact request dispatched once (red first).
- [x] Retain storage until ready; subscribe/claim event and storage through one validated dispatcher. Deduplicate identical request and never overwrite a guarded route with an unrelated event/manual fallback.
- [x] Cover stale A/current B, malformed or missing expected lease, explicit manual, collision and remount/missed sync.
- [x] Run focused then isolated frontend suites and standalone TypeScript, without a Next build.

## Completion

- [x] Update integration note/map with interfaces, red/green evidence and limits.
- [x] Verify diff and prepare scoped correction commit with required attribution.
- [x] Prepare disposition, red/green logs and limits; send with final SHA directly to coordinator immediately after commit, then end turn.

## Execution record

- Task 1: native queued repros red (18 pass/12 fail combined), then 28 pass/2 remaining Controls failures. Real settings publication was included in the fixture, not mocked in JS.
- Task 2: extended route red (28 pass/5 fail), then 33 pass; initial expanded combined 39 pass/402 assertions. Standalone TypeScript and all 43 isolated frontend files pass.
- End-withdrawal edge: portable Stop red 13 pass/1 fail, then 14 pass. Start/source/settings suite 34 pass; lifecycle/provenance 38 pass.
- Ruling: first valid pending Stop wins an event/storage collision; explicit manual button remains independent. This avoids silently replacing a guarded route; a dropped colliding command may require another explicit action.
- Ruling: withdraw an already confirmed End irreversibly for this take. Safety beats reviving stale automatic authority; manual Stop remains necessary after revocation.
- End arriving during admitted capture before lease publication: red 39 pass/2 fail; preserve the exact qualified owner End through lease publication, without reviving any previously revoked take. Expanded bridge green: 41 pass/414 assertions.
- Manual retry found during self-review: focused Controls red 2 pass/1 fail, then 3 pass after releasing failed manual deduplication claim.
- Final review: author self-review only; user explicitly prohibited subagents. No heavy build or physical/macOS/audio qualification was attempted.
