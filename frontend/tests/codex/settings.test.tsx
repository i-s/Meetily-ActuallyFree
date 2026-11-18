import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

const calls: Array<{ command: string; args: any }> = [];
let readiness = 'signed_out';
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  calls.push({ command, args });
  if (command === 'codex_cli_get_path') return "/owned/O'Brien/codex cli";
  if (command === 'codex_cli_save_path') return;
  if (command === 'codex_cli_get_status') return {
    installed: true, path: args?.path || "/owned/O'Brien/codex cli", version: 'codex-cli 0.167.2', logged_in: readiness === 'ready',
    readiness, auth_home: "/owned/O'Brien/Meetily/codex-live-auth", error: readiness === 'ready' ? null : readiness === 'blocked' ? 'AGENTS.override.md blocks live questions; no text sent' : readiness === 'unsupported' ? 'CLI version not qualified; no text sent' : 'Separate isolated ChatGPT sign-in required',
  };
  throw new Error(`No model invocation allowed: ${command}`);
} }));
const { CodexCliSettings } = await import('../../src/components/CodexCliSettings');
test('normal Settings caller exposes isolated signed-out/unsupported/blocked status and properly quoted one-time login without transmitting text', async () => {
  let root!: ReactTestRenderer;
  await act(async () => { root = create(<CodexCliSettings />); });
  expect(JSON.stringify(root.toJSON())).toContain('signed out');
  expect(root.root.findByProps({ 'aria-label': 'Isolated Codex sign-in command' }).children.join('')).toBe("CODEX_HOME='/owned/O'\\''Brien/Meetily/codex-live-auth' '/owned/O'\\''Brien/codex cli' login");
  const input = root.root.findByProps({ 'aria-label': 'Codex executable path' });
  await act(async () => { input.props.onChange({ target: { value: '/owned/custom codex' } }); });
  readiness = 'unsupported';
  await act(async () => { root.root.findAllByType('button').find(b => b.children.includes('Save and check'))!.props.onClick(); });
  expect(calls.find(c => c.command === 'codex_cli_save_path')!.args.path).toBe('/owned/custom codex');
  expect(JSON.stringify(root.toJSON())).toContain('unsupported');
  readiness = 'blocked';
  await act(async () => { root.root.findAllByType('button').find(b => b.children.includes('Check path'))!.props.onClick(); });
  expect(JSON.stringify(root.toJSON())).toContain('AGENTS.override.md blocks');
  expect(calls.every(c => ['codex_cli_get_path', 'codex_cli_get_status', 'codex_cli_save_path'].includes(c.command))).toBe(true);
  await act(async () => { root.unmount(); });
});
