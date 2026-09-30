# Codex CLI subscription provider

Settings > AI model offers **Codex CLI (ChatGPT subscription)**. Summaries,
regeneration, live Ask AI, and saved-meeting Ask AI use the same native provider.
Install and sign in to Codex yourself, then select the provider and re-check.
Meetily never installs Codex, opens a login flow, or reads its credential store.
Status checks run `--version`, `exec --help`, and `login status`; only the explicit
**Test AI call** button and requested summaries/answers consume model usage.
A successful status requires a ChatGPT login, not API-key login. Subscription
availability and usage limits remain controlled by Codex/ChatGPT.

The model picker deliberately offers `default`: Codex chooses its current default
model without a stale list of guessed model IDs. Meetily ignores personal Codex
configuration for isolation, so this is the executable's default rather than a
model override from `~/.codex/config.toml`.

## Native data flow

- `codex_cli/codex_cli.rs` discovers an executable through an explicit Settings
  path, `MEETILY_CODEX_CLI`, PATH, or common macOS/Linux install folders (including
  Homebrew, npm-global, Volta and nvm). An invalid explicit path fails instead of
  silently choosing another executable. The launcher directory is included in
  child PATH so a GUI launch can find an npm launcher's adjacent Node executable.
- `codex_cli/commands.rs` exposes status, saved path and connection-test commands.
  The Settings panel invalidates readiness when the path changes and discards
  old probe responses. Empty path explicitly previews auto-discovery; omitted
  path uses the persisted override.
- Migration `20260930000000_add_codex_cli_path.sql` adds nullable `codexCliPath`.
  Settings API save/read and both UI save entry points preserve this field.
  Switching providers does not clear the Claude path, API keys or other settings.
- `LLMProvider::CodexCli` routes through `generate_summary` before HTTP setup.
  Summary service and `live_assistant.rs` independently load the selected CLI's
  path into the shared `cli_path` argument; the provider requires no API key.
- Fresh `codex exec` processes receive the prompt on stdin, never shell arguments.
  They run in a newly created temporary directory with `--ephemeral`,
  `--ignore-user-config`, `--ignore-rules`, `--skip-git-repo-check`,
  `--sandbox read-only`, JSON events, and approval policy `never`. Meetily removes
  `OPENAI_API_KEY`/`CODEX_API_KEY` from the child environment and requires ChatGPT
  login before generation. No shell interpolates model/path/transcript text.
- Shell/unified execution, apps/plugins/hooks, search, browser, computer, image,
  memory, goals, request-user-input, code-mode-host, skill discovery/instructions
  and multi-agent features are disabled. Project instructions are limited
  to zero bytes. Parent `CODEX_THREAD_ID`/`CODEX_SESSION_ID` are removed. Prompts
  direct Codex to answer only from the meeting text.
  JSON parsing accepts a completed turn and the final agent message. Non-fatal
  `item.error` warnings and `todo_list` plan metadata are ignored, as are reasoning
  items. Command/file/MCP/collaboration/search items are rejected in all three
  lifecycle phases (`started`, `updated`, `completed`); unknown items fail with
  an unsupported-item error. Top-level `error`, failed/partial/empty output still
  fail. Raw diagnostics and meeting text are never included in these errors.
- Each subprocess has a bounded deadline (20 seconds for an individual probe,
  300 seconds for generation). Pipe input/output and child exit are awaited
  concurrently, including large prompts. Cancellation/timeout also covers open
  inherited pipes. Unix process groups terminate npm-launcher descendants; no
  detached pipe tasks retain transcripts. Scratch folders are removed on return.

## CLI contract and limits

Flags were verified against locally available `codex --help`, `codex exec --help`,
`codex login --help` and the upstream config schema/tool construction source at
[OpenAI Codex 67727e7](https://github.com/openai/codex/tree/67727e7cf114cf3e1b71db368d74b24e32f6cb12).
Older versions without the isolation flags are rejected with an update message;
there is no fallback that enables workspace writing or loads personal tools.

Codex CLI does not provide a universal `--tools none` contract. A local Responses
fixture with Codex `0.159.0-alpha.3` advertised `tools: []` using the updated
settings. This covers that executable and the fixture provider/model metadata;
other versions or account-specific model catalogs may expose different tools.
The read-only sandbox remains the filesystem protection; Meetily rejects
responses containing execution tool events, but does not claim that an internal
tool can never be attempted. Managed system Codex policy may further restrict
execution.
A real logged-in call and macOS installation still need platform qualification.
Windows `.cmd`/`.bat` shims are deliberately not executed through a shell; Windows
users must choose a native `codex.exe`. Windows descendant process cleanup is not
qualified by the Unix fixtures.

## Compatibility fix and working reference

On 2026-09-30 the user reported that Codex CLI works correctly in
[YoungTeurus/meetily PR #1](https://github.com/YoungTeurus/meetily/pull/1), while
this fork's `d9cb04f` Preview failed with `Codex attempted to use a tool`.
The reference is pinned at
[`summary/codex_cli.rs`, `4732a3ff`](https://github.com/YoungTeurus/meetily/blob/4732a3ff2331e3b33e007313c06b7748625e9304/frontend/src-tauri/src/summary/codex_cli.rs).

A local, unauthenticated Responses fixture reproduced the failure using the
actual Codex `0.159.0-alpha.3` executable. Our old `features.memory_tool=false`
setting emits an `item.completed` with item type `error` explaining that the
setting is deprecated in favor of `features.memories`. Codex then returns a
valid final answer and `turn.completed`; our parser incorrectly reported the
warning as tool use. The user's exact emitted item was not captured, but this
reproduces their error without any tool call or paid inference.

The upstream
[exec event schema](https://github.com/openai/codex/blob/67727e7cf114cf3e1b71db368d74b24e32f6cb12/codex-rs/exec/src/exec_events.rs)
defines item-level `error` as non-fatal and the top-level `error` as fatal. The
parser now respects that distinction and allows plan metadata, while retaining
execution-tool rejection and completion checks. The deprecated memory alias was
removed; `features.memories=false` remains. The working reference's additional
feature/skill disabling settings and parent-session cleanup were adopted. The
same local endpoint observed four advertised tools before these settings
(`request_user_input`, `get_goal`, `create_goal`, `update_goal`) and none after.

Unlike the reference's permissive parser, unrecognized item types still fail
closed, with an unsupported-item message rather than a false tool diagnosis.
The reference also resolves npm's native payload and uses a 600-second overall
deadline; those differences are not part of this parser/configuration fix.
The ChatGPT-only login gate, isolated working directory, stdin transport and
existing bounded I/O/cancellation contract are retained. Installed macOS and
real-account summary qualification remain separate from the fixture result.

## Verification

`cargo test -p meetily --lib codex_cli` covers the production native module, JSON
parsing/login validation, an executable fixture receiving a large stdin prompt,
isolated args/cwd cleanup, failures, cancellation/deadlines/Unix descendants,
settings preservation and backward-compatible model-config deserialization.
Fixtures never use a real account, credential, network inference or model.

Run mock-heavy frontend fixtures separately from `frontend/`:

```sh
pnpm dlx bun@1.3.10 test tests/codex-cli/provider.test.ts
pnpm dlx bun@1.3.10 test tests/codex-cli/settings.test.tsx
pnpm dlx bun@1.3.10 test tests/hooks/summary-recovery.test.tsx
```

The core module also compiles independently of Tauri using a portable Cargo test
harness that imports `codex_cli/codex_cli.rs` directly with Tokio, tokio-util,
serde, serde_json and tempfile. Its ten default tests passed on Linux, including
non-fatal warnings, plan metadata, execution items in every lifecycle phase,
unknown-item failure and session isolation. Frontend tests
passed for readiness, clearing a saved path, stale responses and both summary
preflight error transitions. Full native/installed-app qualification is separate
from these fixture results; no authenticated model inference was invoked.

An additional ignored test runs a real installed CLI against an in-process
localhost Responses endpoint, using the production generation arguments with
only provider routing replaced. It clears the child environment, uses an empty
HOME/CODEX_HOME, checks that no Authorization header was sent, requires an empty
tool catalog, and parses the synthetic final answer through the production
parser. It passed on Linux with `/opt/codex/bin/codex` version `0.159.0-alpha.3`.
Run it explicitly with the native build prerequisites or the portable harness:

```sh
MEETILY_CODEX_TEST_BINARY=/absolute/path/to/codex cargo test -p meetily --lib codex_cli_real_binary -- --ignored
```

This tests actual CLI configuration/event compatibility without an account,
private transcript, credential inspection, or a paid inference request.
