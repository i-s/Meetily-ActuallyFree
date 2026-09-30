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
  memory and multi-agent features are disabled. Project instructions are limited
  to zero bytes. Prompts direct Codex to answer only from the meeting text.
  JSON parsing accepts a completed turn and the final agent message, rejects
  tool events, failed/partial/empty output, and does not expose raw stderr that
  could contain sensitive diagnostics.
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

Codex CLI does not provide a universal `--tools none` contract. Its current model
catalog can still advertise internal tools such as `apply_patch` despite feature
toggles. The read-only sandbox remains the filesystem protection; Meetily rejects
responses containing tool events, but does not claim that an internal tool can
never be attempted. Managed system Codex policy may further restrict execution.
A real logged-in call and macOS installation still need platform qualification.
Windows `.cmd`/`.bat` shims are deliberately not executed through a shell; Windows
users must choose a native `codex.exe`. Windows descendant process cleanup is not
qualified by the Unix fixtures.

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
serde, serde_json and tempfile. Its six tests passed on Linux. Frontend tests
passed for readiness, clearing a saved path, stale responses and both summary
preflight error transitions. Full native/installed-app qualification is separate
from these fixture results; no real Codex generation was invoked during testing.
