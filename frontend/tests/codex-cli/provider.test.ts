import { describe, expect, test, mock } from 'bun:test';
const calls: Array<[string, unknown]> = [];
mock.module('@tauri-apps/api/core', () => ({invoke: async (command: string, args: unknown) => { calls.push([command, args]); return {installed: true, logged_in: true}; }}));
const {getCodexCliStatus, isCodexCliReady, codexCliBlockingReason} = await import('../../src/lib/codex-cli');
describe('Codex CLI provider', () => {
 test('requires ChatGPT login and compatible CLI', () => {
  expect(isCodexCliReady(null)).toBe(false);
  expect(isCodexCliReady({installed: true, logged_in: false})).toBe(false);
  expect(isCodexCliReady({installed: true, logged_in: true})).toBe(true);
  expect(isCodexCliReady({installed: true, logged_in: true, error: 'Update Codex'})).toBe(false);
  expect(codexCliBlockingReason({installed: true, logged_in: false})).toContain('codex login');
 });
 test('empty override requests auto-discovery rather than reusing a saved path', async () => {
  await getCodexCliStatus('');
  expect(calls.at(-1)).toEqual(['codex_cli_get_status', {path: ''}]);
 });
});
