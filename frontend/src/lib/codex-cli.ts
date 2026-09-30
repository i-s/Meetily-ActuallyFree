import { invoke } from '@tauri-apps/api/core';

export interface CodexCliStatus {
  installed: boolean;
  path?: string | null;
  version?: string | null;
  logged_in: boolean;
  error?: string | null;
}

/** Use the CLI's current default model without guessing subscription model IDs. */
export const CODEX_CLI_MODELS = ['default'];
export function getCodexCliStatus(path?: string | null): Promise<CodexCliStatus> {
  // Empty string deliberately overrides the saved path to preview auto-discovery.
  return invoke('codex_cli_get_status', { path: path ?? null });
}
export function getCodexCliPath(): Promise<string | null> {
  return invoke('codex_cli_get_path');
}
export function testCodexCliConnection(path: string, model: string): Promise<{status: string; message: string}> {
  return invoke('codex_cli_test_connection', {path, model});
}
export function isCodexCliReady(status: CodexCliStatus | null): boolean {
  return !!status?.installed && !!status.logged_in && !status.error;
}
export function codexCliBlockingReason(status: CodexCliStatus | null): string | null {
  if (!status) return 'Checking Codex CLI…';
  if (status.error) return status.error;
  if (!status.installed) return 'Codex CLI not found. Set its executable path in Model Settings.';
  if (!status.logged_in) return 'Run `codex login` in a terminal and sign in with ChatGPT, then re-check.';
  return null;
}
