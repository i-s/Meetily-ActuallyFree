import React from 'react';
import { act, create, type ReactTestInstance, type ReactTestRenderer } from 'react-test-renderer';
import { afterEach, beforeEach, expect, mock, test } from 'bun:test';

interface NativeStatus {
  installed: boolean; path: string | null; version: string | null;
  logged_in: boolean; readiness: string; auth_home: string; error: string | null;
}
const calls: Array<{ command: string; args: any }> = [];
const savedPath = "/owned/O'Brien/codex cli";
const authHome = "/owned/O'Brien/Meetily/codex-live-auth";
let status: NativeStatus;
let statusRequest: ((args: any) => Promise<NativeStatus>) | null = null;
let pathRequest: (() => Promise<string>) | null = null;
let root: ReactTestRenderer | undefined;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  calls.push({ command, args });
  if (command === 'codex_cli_get_path') return pathRequest ? pathRequest() : savedPath;
  if (command === 'codex_cli_save_path') return;
  if (command === 'codex_cli_get_status') return statusRequest ? statusRequest(args) : { ...status, path: args?.path || status.path };
  throw new Error(`No model invocation allowed: ${command}`);
} }));
const { CodexCliSettings } = await import('../../src/components/CodexCliSettings');

beforeEach(() => {
  calls.length = 0;
  statusRequest = null;
  pathRequest = null;
  status = { installed: true, path: savedPath, version: 'codex-cli 0.170.0', logged_in: false,
    readiness: 'signed_out', auth_home: authHome, error: 'Separate isolated ChatGPT sign-in required' };
});
afterEach(async () => {
  if (root) await act(async () => { root!.unmount(); });
  root = undefined;
  expect(calls.every(c => ['codex_cli_get_path', 'codex_cli_get_status', 'codex_cli_save_path'].includes(c.command))).toBe(true);
});
async function mount() {
  await act(async () => { root = create(<CodexCliSettings />); });
  return root!;
}
function text(node: ReactTestInstance | string): string {
  return typeof node === 'string' ? node : node.children.map(text).join('');
}
function disclosure(label: string) {
  return root!.root.findAllByType('details').find(node => text(node.findByType('summary')) === label)!;
}
function button(label: string) {
  return root!.root.findAllByType('button').find(node => text(node) === label)!;
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

test('selected compact status keeps only existing actions and initially collapsed checks, diagnostics and sign-in', async () => {
  const view = await mount();
  expect(text(view.root.findByType('h4'))).toBe('Separate sign-in required');
  expect(view.root.findAllByType('button').map(text)).toEqual(['Save and check', 'Check path']);
  for (const label of ['What was checked', 'Separate ChatGPT sign-in', 'CLI path, version and isolation details']) {
    expect(disclosure(label)).toBeDefined();
    expect(Boolean(disclosure(label).props.open)).toBe(false);
  }
  expect(text(disclosure('What was checked'))).toContain('CLI installationFound');
  expect(text(disclosure('What was checked'))).toContain('Meetily compatibilitySupported');
  expect(text(disclosure('What was checked'))).toContain('Isolated ChatGPT loginSign-in required');
  expect(text(disclosure('CLI path, version and isolation details'))).toContain('codex-cli 0.170.0');
  expect(text(disclosure('CLI path, version and isolation details'))).toContain(authHome);
  expect(view.root.findByProps({ 'aria-label': 'Isolated Codex sign-in command' }).children.join('')).toBe("CODEX_HOME='/owned/O'\\''Brien/Meetily/codex-live-auth' '/owned/O'\\''Brien/codex cli' login");
  expect(text(view.root)).not.toContain('Requires the tested Codex CLI 0.167.2');
  expect(text(view.root)).toContain('does not test network access or remaining quota');
});

test.each([
  ['missing', false, false, 'Codex CLI not found', 'CLI not found at the configured path', 'Not found', 'Not checked'],
  ['unsupported', true, false, 'CLI is incompatible', 'Required option --ignore-rules is missing. No meeting text was sent.', 'Found', 'Not supported'],
  ['blocked', false, false, 'Isolation blocked', 'AGENTS.override.md blocks live questions; no text sent', 'Not checked', 'Blocked'],
  ['error', true, false, 'CLI check failed', 'Codex CLI could not report its version', 'Found', 'Not checked'],
  ['ready', true, true, 'Ready for questions', null, 'Found', 'Supported'],
] as const)('compact %s status distinguishes installation, compatibility and login without offering unsafe sign-in', async (readiness, installed, loggedIn, heading, error, installation, compatibility) => {
  status = { ...status, readiness, installed, logged_in: loggedIn, error,
    path: installed ? savedPath : null, version: installed ? 'codex-cli 0.170.0' : null };
  const view = await mount();
  expect(text(view.root.findByType('h4'))).toBe(heading);
  if (error) expect(text(view.root)).toContain(error);
  const checks = text(disclosure('What was checked'));
  expect(checks).toContain(`CLI installation${installation}`);
  expect(checks).toContain(`Meetily compatibility${compatibility}`);
  expect(checks).toContain(`Isolated ChatGPT login${readiness === 'ready' ? 'Confirmed' : 'Not checked'}`);
  expect(view.root.findAllByProps({ 'aria-label': 'Isolated Codex sign-in command' })).toHaveLength(0);
});

test('inconsistent ready result is not presented as permission to send a meeting', async () => {
  status = { ...status, readiness: 'ready', logged_in: false, error: null };
  const view = await mount();
  expect(text(view.root.findByType('h4'))).toBe('CLI check failed');
  expect(text(disclosure('What was checked'))).not.toContain('Confirmed');
});

test('Check path does not save while Save and check persists the trimmed path, including automatic discovery', async () => {
  const view = await mount();
  const input = view.root.findByProps({ 'aria-label': 'Codex executable path' });
  await act(async () => { input.props.onChange({ target: { value: '  /owned/custom codex  ' } }); });
  expect(view.root.findAllByType('h4')).toHaveLength(0);
  await act(async () => { button('Check path').props.onClick(); });
  expect(calls.filter(c => c.command === 'codex_cli_save_path')).toHaveLength(0);
  expect(calls.filter(c => c.command === 'codex_cli_get_status').at(-1)!.args.path).toBe('/owned/custom codex');
  await act(async () => { button('Save and check').props.onClick(); });
  expect(calls.filter(c => c.command === 'codex_cli_save_path').at(-1)!.args.path).toBe('/owned/custom codex');
  await act(async () => { input.props.onChange({ target: { value: '   ' } }); });
  await act(async () => { button('Save and check').props.onClick(); });
  expect(calls.filter(c => c.command === 'codex_cli_save_path').at(-1)!.args.path).toBeNull();
});

test('a failed local check exposes its error without stale readiness or extra native operations', async () => {
  const view = await mount();
  statusRequest = async () => { throw new Error('Synthetic local IPC failure'); };
  await act(async () => { button('Check path').props.onClick(); });
  expect(text(view.root.findByProps({ role: 'alert' }))).toContain('Synthetic local IPC failure');
  expect(view.root.findAllByType('h4')).toHaveLength(0);
  expect(button('Check path').props.disabled).toBe(false);
});

test('pending checks disable existing controls and discard a late response after path invalidation', async () => {
  const view = await mount();
  const wait = deferred<NativeStatus>();
  statusRequest = () => wait.promise;
  await act(async () => { button('Check path').props.onClick(); });
  expect(button('Check path').props.disabled).toBe(true);
  expect(button('Save and check').props.disabled).toBe(true);
  const input = view.root.findByProps({ 'aria-label': 'Codex executable path' });
  expect(input.props.disabled).toBe(true);
  // A queued change event invalidates the result even if it was captured before disable.
  await act(async () => { input.props.onChange({ target: { value: '/different/codex' } }); });
  await act(async () => { wait.resolve({ ...status, readiness: 'ready', logged_in: true, error: null }); });
  expect(view.root.findAllByType('h4')).toHaveLength(0);
  expect(input.props.value).toBe('/different/codex');
  expect(button('Check path').props.disabled).toBe(false);
});

test('leaving Settings during path loading prevents the later status subprocess probe', async () => {
  const wait = deferred<string>();
  pathRequest = () => wait.promise;
  await mount();
  await act(async () => { root!.unmount(); });
  root = undefined;
  await act(async () => { wait.resolve(savedPath); });
  expect(calls.map(c => c.command)).toEqual(['codex_cli_get_path']);
});
