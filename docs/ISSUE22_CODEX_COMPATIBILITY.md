# Issue #22 — capability-based installed Codex CLI compatibility

Live issue body and empty comments read 2026-11-19. Technical stage only,
local branch `meetily/ux-22`, verified parent
`c05a32f64d703a29083186110b72a01f6eca1449` (restored SQLx migration history).
No other issue/PR, migration, UI source, account or owner Mac was changed.

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

## UI preparation and remaining manual milestone

No visible UI source changed. In particular, the old Settings copy still says
0.167.2; changing that copy and presentation is intentionally deferred with the
rest of UI until owner choice, not claimed complete.

Two functional local throwaway alternatives are prepared but NOT demonstrated
or put to an owner question while #23 owns the slot:

- A: one concise status/next action, expandable path/version and sign-in details.
- B: three separate installation / compatibility / isolated-login rows, the same
  diagnostics and actions. Both use the same five synthetic status scenarios,
  expandable details, path edit/check simulation and login instructions.

Prototype: `/tmp/meetily22-status-options.html`. No external scripts, network,
CLI invocation or credentials. After explicit slot transfer (or owner's direct
opening here), show both using `html_preview` then `html_render`, ask the concrete
choice here, and wait for the actual answer. No coordinator callback requesting
relay of that question.

Pending: UI choice and implementation/tests; actual Mac/Dock executable discovery,
private-auth-home permissions and scratch-HOME behavior; owner-controlled separate
ChatGPT login and real-subscription smoke only after coordination. Local fixtures
do not establish account access, quota, semantic quality, remote latency,
recording/ASR responsiveness on hardware, desktop IPC or a packaged installation.
No frontend production export, native desktop build, DMG, Actions, push, PR,
release, account changes or owner-Mac actions occurred. No #24 work started.
