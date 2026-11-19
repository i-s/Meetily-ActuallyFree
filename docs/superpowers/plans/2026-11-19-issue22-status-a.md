# Issue #22 — approved status A implementation plan

> **For agentic workers:** use superpowers:executing-plans inline; the owner prohibits additional agents.

**Goal:** Implement the explicitly selected variant A in the existing CLI Settings section.

**Architecture:** Preserve the existing native path/status/save calls and generation guards. Render one concise status, native disclosure for three local checks, separate disclosure for path/version/isolation, and sign-in instructions only for a compatible signed-out CLI. No new buttons, routes, providers, accounts or model requests.

**Tech stack:** React/TypeScript, existing Button, native HTML details/summary; Bun isolated React renderer tests and existing portable Rust harness.

**Spec:** Owner chose “Вариант A” in this thread after the autonomous prototype `/tmp/meetily22-status-options-v2.html` was supplied. The inline HTML tool failed at its html/path argument contract; the standalone artifact was the agreed fallback. This choice authorizes the shown compact presentation, not #24 model selection or broader Settings changes.

## Constraints

- Work on meetily/ux-22 from accepted technical commit 08b16a1 (parent c05a32f).
- Only local edits/isolated synthetic tests. No agents, push, Actions, desktop build, login or owner Mac.
- Keep existing `Codex executable path`, `Save and check`, `Check path` controls.
- Keep English product copy to match surrounding Settings; owner communication is Russian.
- Version/path visible on request, no fixed-version requirement. Readiness does not prove network/quota.
- Existing dedicated auth home, no secret copying, no model/service calls in status UI.

## Review focus

Missing versus incompatible versus signed-out; blocked isolation must not invite login; no unverified ready label; late status after editing/navigation; quoted command containing spaces/apostrophes. Exercise these through mounted production component tests, not source strings.

## Steps

- [x] Expand `frontend/tests/codex/settings.test.tsx` for concise labels, initially collapsed disclosures, all native readiness categories, no login command outside signed_out, quoted login, exactly two existing actions, check-versus-save calls, stale response and rejection handling. Run red.
- [x] Update only `frontend/src/components/CodexCliSettings.tsx` for selected A. Keep IPC shape and no fallback; status check output never runs a login command. Run targeted tests green.
- [x] Run every frontend test file separately via test-isolated.mjs and standalone types (no Next build). Reuse available dependency cache without downloading or modifying another worktree's sources.
- [x] Re-run portable native and exact real-CLI loopback controls. Update DEVELOPMENT_MAP and ISSUE22 note with chosen UI, results and unqualified Mac/service gates. Self-review, diff check, local commit and final coordinator handoff only after completion.
