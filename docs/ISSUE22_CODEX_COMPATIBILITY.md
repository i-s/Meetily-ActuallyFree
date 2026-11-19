# Issue #22 — capability-based installed Codex CLI compatibility

Live issue body and empty comments read 2026-11-19. Local implementation,
branch `meetily/ux-22`, verified base
`c05a32f64d703a29083186110b72a01f6eca1449` (restored SQLx migration history).
Technical stage was committed as `08b16a1beec3c4d34f10b0c9a30f06c5ac4c0fa7`;
the owner subsequently chose compact UI variant A in this thread. No other
issue/PR, migration, account or owner Mac was changed.

## Data flow and compatibility boundary

`cli::probe` and `cli::generate` share `check_capabilities`. The version command
must succeed, but its number is diagnostic metadata, not a one-version allowlist.
Both 0.167.2 and 0.170.0 pass the real local parser checks. Synthetic tests also
accept 0.999.0 with the same capabilities, proving there is no hidden numerical
gate; that is NOT real qualification of 0.999.0 or arbitrary future binaries.

Before login status and before meeting stdin:

1. Revalidate the dedicated canonical, private, owned `CODEX_HOME`, refusing both
   global AGENTS filenames/symlinks before every subprocess.
2. Check exact help tokens for ephemeral/config/rules suppression, nonrepo cwd,
   sandbox, JSON/color, model and config overrides. A lookalike `--json-lookalike`
   does not satisfy `--json`. Missing capability names appear in safe errors.
3. Have the CLI parser accept the complete production argument vector, replacing
   stdin marker `-` with `--help`. This catches rejected argument combinations;
   it is not just a substring check. No stdin or model turn is supplied.
4. Check isolated ChatGPT login. Missing/API-key login remains `signed_out`, not
   `unsupported`. No meeting stdin reaches either incompatible or signed-out CLI.

`readiness` and the command interfaces are unchanged: `missing`, `unsupported`,
`blocked`, `error`, `signed_out`, `ready`. `ready` is local parser/login readiness,
NOT a network/quota check or cryptographic attestation. Help parsing cannot prove
that every config key is honored. Actual outgoing requests are qualified below;
future CLI behavior, managed/system policy and a malicious installed executable
are not attested by these local diagnostics. Meetily trusts the selected installed
CLI. There is no runtime model request added to status, no provider override in
product code, no hidden API/key/provider fallback and no capability cache.

`command` still clears environment and forces dedicated `CODEX_HOME`. It now also
sets `HOME` and `USERPROFILE` to the private temporary cwd instead of inheriting
the person's home. OS/locale essentials and bounded runtime PATH remain. Parent
API routing, proxy/tool credentials, session IDs and preload options are absent.
Login credentials are never read/copied from a general Codex home. Generation's
text-only flags/config, JSON tool-item refusal, 1 MiB output bounds, 60-second
whole-operation deadline and Unix process-group cancellation remain unchanged.

## Exact real binaries and independent behavioral controls

Linux x86_64 only; no credentials, authorization headers, real services or
private text. Real binary availability was checked, not inferred from mocks:

| Version | Origin | Executable SHA-256 |
| --- | --- | --- |
| `codex-cli 0.167.2` | Existing `/opt/t3-code/codex-0.167.2/bin/codex` | `a9672d0fa0dd8fcf8cad136fa0d056b914bc046f5ac3d78f4a5cf390655af56b` |
| `codex-cli 0.170.0` | Exact npm `@openai/codex@0.170.0-linux-x64`, extracted into its own new `/tmp/meetily22-real170.*` directory; no install scripts | `880819aab735adcf16f36cd628940b9f1e652a36c01d10a5d32f83cc84902971` |

The 0.170.0 archive SHA-512 was checked against registry integrity:
`6cCZJaZL17tiTAvhd1Pirt8vsV1dP8CfG94+IHygUO4m24oYLQrIInJJLyKzcMv/uUonbu1dF2JmcXhA/s5bsg==`.
This establishes matching downloaded bytes, not a separate publisher audit.
No binary/archive is committed. Mac binaries were not obtained or executed.

Five real tests passed separately for EACH exact binary:

- Dedicated home: authorized synthetic speech and separate Notes are present in
  the localhost Responses request; unrelated global/config/project/parent-project/
  default-home/skill/host-skill sentinels are absent; actual emitted `tools: []`.
- Interrupted initial SSE stream: actual CLI retry completes; same outgoing
  isolation assertions hold on both attempts.
- Shared-home positive control: bypassing the production guard ONLY in the
  synthetic fixture causes global AGENTS to appear in the outgoing request on
  BOTH versions. This original leak still exists; the dedicated-home guard is
  essential, not replaced by the new compatibility check.
- Tool positive control: enabling shell ONLY in fixture config produces a
  nonempty outgoing tools array. The empty-tools check detects a real control
  failure, not merely presence of config strings. No tool is actually requested.
- Actual `probe` reaches `signed_out` in an empty synthetic auth home, and a real
  parser rejects an unknown execution switch. Expected exact version is asserted
  when supplied via `MEETILY_CODEX_EXPECT_VERSION`. No auth.json is acquired.

Tests inspect emitted JSON, not just flags. Test-only routing goes to a local
credential-free provider; it is never an application option. The safe-request
fixture deliberately overrides HOME with another *synthetic* hostile home to
stress host-discovery suppression, independently of production's scratch HOME.

## Reproduction and network isolation

Compile real production module/command bodies using the portable harness:

```sh
CARGO_BUILD_JOBS=1 CARGO_HOME=<offline-cache> CARGO_TARGET_DIR=<own-target> \
  python3 -I frontend/scripts/test-codex-portable.py
```

Default suite: **33 passed, 0 failed, 5 ignored** (real CLI opt-ins). Covers both
versions with realistic synthetic installed CLI protocols, complete-parser
rejection, missing/lookalike option, signed-out/API-key distinction, no stdin or
login probe before compatibility success, scratch HOME, late AGENTS rejection,
source/Notes ownership, safe errors, output bounds and descendant cancellation.
Mocks do not prove real account login or CLI semantics.

Use the compiled test executable printed by Cargo (not a random matching file):

```sh
python3 -I frontend/scripts/test-codex-loopback.py \
  /absolute/portable-test-executable /absolute/codex 'codex-cli 0.167.2'
# Repeat with exact 0.170.0 executable and expected version.
```

This runner requires Linux bubblewrap, iproute2, setpriv and working unprivileged
user/network namespaces; otherwise it fails, never falls back to unsandboxed
execution. Its namespace contains only loopback, with no external interface or
route; it clears parent environment, masks real T3/root homes, uses synthetic
HOME, and drops all Linux capabilities before the test executable/CLI. Before
exec, independent guards assert that `/proc/net/dev` contains only `lo`, the IPv4
route table is empty, all five process capability sets are zero and the real T3
home mount is empty. The fixture checks absence of Authorization. No external
DNS/provider can be reached
even if routing is accidentally changed. Public npm download occurred separately,
before execution, not as a model/service request. Downloaded code was not run from
its archive directory.

Final isolated runner results: **5 passed / 0 failed for 0.167.2**, separately
**5 passed / 0 failed for 0.170.0**. The initial runner's nested bubblewrap setup
failed before CLI execution (`Creating new namespace failed: Operation not
permitted`). Boundary reproduction identified nested namespace setup; the final
runner uses a single namespace plus setpriv, and both versions then passed.
Temporary evidence: `/tmp/meetily22-final-native.log`,
`/tmp/meetily22-final-real167.log`, `/tmp/meetily22-final-real170.log`.
These are local diagnostic artifacts, not committed fixtures or a public release.

## Owner-selected status A and remaining manual milestone

After the coordinator explicitly transferred the one-question UX slot from #23,
two interactive alternatives were supplied in existing Settings → Live questions
context, with the same compatible-but-signed-out 0.170.0 scenario. The prototype
retained the existing path field and `Save and check` / `Check path` actions;
variant A collapses its three checks, variant B keeps them visible. The inline
HTML tool rejected its exposed html/path argument combination; the immediate
standalone fallback was `/tmp/meetily22-status-options-v2.html`. The owner then
explicitly answered **“Вариант A.”** No visible product UI was changed before
that answer. No waiting/UX-relay callback was sent to the coordinator.

`frontend/src/components/CodexCliSettings.tsx` now implements A:

- One concise heading and next-step/reason distinguish missing, unsupported,
  blocked isolation, check error, compatible signed-out, and ready states.
  Native incompatibility/blocked reasons remain visible, not replaced with a
  version-range guess. An inconsistent ready result fails closed in presentation.
- Native HTML disclosures keep **What was checked** (installation/compatibility/
  isolated login) and **CLI path, version and isolation details** collapsed by
  default, keyboard-accessible via summary. Labels communicate meaning without
  relying on color. The old hard-coded 0.167.2 requirement is removed.
- **Separate ChatGPT sign-in** is offered only for a compatible signed-out CLI
  with a known executable and private auth home. Its properly POSIX-quoted
  command is display-only; no button, shell execution, login request, API provider,
  model selection or new native command was added. Incompatible/blocked/error/
  ready states do not display a sign-in command.
- Existing path/save/check controls and native IPC arguments remain. Editing
  invalidates prior readiness; generation guards suppress stale results after
  editing/unmount. A queued path event also releases the busy UI so an obsolete
  result cannot leave its controls disabled. Native subprocess ownership and
  limits remain independent of the UI.
- A permanent footnote distinguishes local readiness from network/quota and
  reiterates that recording, local ASR and Notes remain independent. No model
  input is sent by the Settings component.

### Frontend verification

- Red/green mounted production-component tests: **11 passed, 0 failed**. They
  exercise all readiness categories, initially collapsed disclosure markup,
  existing actions only, no unsafe login instruction, quoted spaces/apostrophes,
  check without save, trimmed path/automatic discovery save, native check failure,
  pending control disabling, late result invalidation and unmount during path
  loading. Every mocked IPC invocation is restricted to the three existing
  local path/status/save commands; no model/service operation is permitted.
- Full isolated frontend suite: **73/73 test files passed**, with each Bun file
  in a fresh process. Expected synthetic storage/autosave failure diagnostics in
  other groups are printed while their assertions pass; they are not hidden
  product failures. This is mock/fixture coverage, not real desktop transport.
- Standalone TypeScript `tsc -p frontend/tsconfig.json --noEmit --incremental false`
  passed, separate from any Next build. Available cached dependencies were reused
  by a local ignored node_modules symlink after matching pnpm lockfiles; no
  package installation or changes to another worktree were made.
- The native **33 passed / 5 ignored** default suite and **5/5 per exact real
  binary** loopback controls described above were re-run for final qualification.
  Evidence: `/tmp/meetily22-ui-green.log`, `/tmp/meetily22-ui-all.log`,
  `/tmp/meetily22-ui-types.log`, and the native/real logs listed above.

**Manual milestone remains:** verify this selected presentation on the owner's
Mac, Dock executable discovery, private-auth-home permissions and scratch-HOME
behavior; owner-controlled separate ChatGPT login and real-subscription smoke
only after coordination. Rendered-component tests do not establish actual Mac
focus/layout/interaction, account access, quota, semantic quality, remote latency,
recording/ASR responsiveness on hardware, desktop IPC or a packaged installation.
No frontend production export, native desktop build, DMG, Actions, push, PR,
release, account changes or owner-Mac actions occurred. No #24 work started.
