# Issue #22 technical stage — design and execution plan

Owner explicitly authorized independent local technical work from c05a32f;
visible UI and its decision remain excluded until the question slot is opened.
No agents, push, Actions, desktop packaging, accounts or real service calls.

## Design

Keep version as diagnostic metadata, not an equality gate. Before login status
or meeting stdin, require all execution switches and have the installed parser
accept the complete production argument vector with `--help` (no model request).
Use one shared preflight in status and generation. Missing switches and rejected
combinations produce specific safe errors. This is a local capability check, not
a claim that an arbitrary executable or future release is attested safe.

Preserve dedicated private CODEX_HOME and per-process AGENTS checks; replace
inherited HOME/USERPROFILE with scratch cwd so host discovery cannot see personal
configuration. Preserve text-only controls, cancellation, output bounds and no
fallback. Real credential-free localhost requests for exact 0.167.2 and 0.170.0
qualify emitted tools/context separately from mocked preflight tests. Run tests
in a network namespace with only loopback, synthetic HOME and an independent
Cargo target. Never run downloaded code from its archive directory.

Alternatives: keeping one exact-version allowlist violates #22; trusting just
three help substrings misses other switches and parser rejection. A runtime
model request is not a local diagnostic and is deliberately not added.

## Execution (inline, no agents)

- [x] Add regressions: both versions and unknown version with same capabilities;
      absent/rejected switch; signed-out and API-key login; no generation before
      capability success; HOME/USERPROFILE do not inherit parent.
- [x] Run new regressions red in portable native harness.
- [x] Implement shared capability preflight and scratch home; update existing
      fixtures to advertise and parse full execution switches.
- [x] Run portable tests green, then real exact binaries against localhost:
      safe home, recovered stream, shared-home leak positive control, missing
      switch negative control, signed-out local diagnostic.
- [x] Record actual binary availability, provenance, commands/results and limits
      in linked issue note; prepare two UI alternatives without demonstrating or
      asking the owner before the slot opens. Self-review and diff check.

## Review focus / remaining gates

Help substring false positives; supported help but rejected invocation; absence
of auth versus incompatible installation; late personalization; accidental real
network or personal HOME. Runtime diagnostics do not prove quota, network,
semantic quality or Mac behavior. Manual milestone: later UI choice, owner Mac
check with their executable and separately coordinated isolated ChatGPT login.
