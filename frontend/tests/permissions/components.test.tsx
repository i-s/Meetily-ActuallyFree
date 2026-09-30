import React from 'react';
import { act, create } from 'react-test-renderer';
import { beforeEach, expect, mock, test } from 'bun:test';
const storage = new Map<string, string>();
Object.defineProperty(globalThis, 'window', { configurable: true, value: { sessionStorage: {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => storage.set(key, value),
}, location: { reload() {} } } });
Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { userAgent: 'Mac' } });
const unknown = { detected: false, conclusive: false, reason: 'no_audio_detected' };
const verified = { detected: true, conclusive: true, reason: 'audio_detected' };
const denied = { detected: false, conclusive: true, reason: 'permission_denied' };
let probe: unknown = unknown;
let resolveProbe: ((result: unknown) => void) | null = null;
let deferProbe = false;
let deferMeter = false;
let resolveMeter: (() => void) | null = null;
const calls: string[] = [];
const permissions = { microphone: 'authorized', systemAudio: 'unknown' };
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string) => {
  calls.push(command);
  if (command === 'start_audio_level_monitoring' && deferMeter) return new Promise<void>(resolve => { resolveMeter = resolve; });
  if (command === 'get_audio_devices') return [{ name: 'Mic', device_type: 'Input' }, { name: 'Speakers', device_type: 'Output' }];
  if (command === 'trigger_system_audio_permission_command') {
    if (deferProbe) return new Promise(resolve => { resolveProbe = resolve; });
    if (probe instanceof Error) throw probe;
    return probe;
  }
  return true;
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
mock.module('sonner', () => ({ toast: { info() {} } }));
mock.module('../../src/hooks/usePlatform', () => ({ usePlatform: () => 'macos', useIsLinux: () => false }));
mock.module('../../src/contexts/OnboardingContext', () => ({ useOnboarding: () => ({
  permissions,
  setPermissionStatus: (key: keyof typeof permissions, value: string) => { permissions[key] = value; },
  setPermissionsSkipped() {}, completeOnboarding: async () => {}, goPrevious() {},
}) }));
mock.module('../../src/components/onboarding/OnboardingContainer', () => ({ OnboardingContainer: ({ children }: any) => <div>{children}</div> }));
mock.module('../../src/components/onboarding/shared', () => ({ PermissionRow: ({ title, status, description, onAction }: any) => <button data-permission={title} data-status={status} onClick={onAction}>{description}</button> }));
mock.module('../../src/components/ui/select', () => {
  const Component = ({ children }: any) => <div>{children}</div>;
  return { Select: Component, SelectContent: Component, SelectItem: Component, SelectTrigger: Component, SelectValue: Component };
});
const { PermissionWarning } = await import('../../src/components/PermissionWarning');
const { PermissionsStep } = await import('../../src/components/onboarding/steps/PermissionsStep');
const { AudioTestStep } = await import('../../src/components/onboarding/steps/AudioTestStep');
const { MACOS_SYSTEM_AUDIO_VERIFIED_KEY } = await import('../../src/lib/system-audio-permission');
const text = (view: ReturnType<typeof create>) => JSON.stringify(view.toJSON());
const settle = () => new Promise(resolve => setTimeout(resolve, 0));
beforeEach(() => {
  storage.clear(); calls.length = 0; probe = unknown; deferProbe = false; resolveProbe = null; deferMeter = false; resolveMeter = null;
  permissions.systemAudio = 'unknown';
});

test('warning gives neutral retry advice for unknown and explicit settings advice for denied', () => {
  const view = create(<PermissionWarning hasMicrophone systemAudio="unknown" onRecheck={() => {}} />);
  expect(text(view)).toContain('Computer audio has not been verified');
  expect(text(view)).not.toContain('won’t be captured');
  expect(text(view)).not.toContain('can’t record');
  view.update(<PermissionWarning hasMicrophone systemAudio="denied" onRecheck={() => {}} />);
  expect(text(view)).toContain('permission was denied');
  view.update(<PermissionWarning hasMicrophone systemAudio="verified" onRecheck={() => {}} />);
  expect(view.toJSON()).toBeNull();
  view.unmount();
});

test('onboarding permission action handles all probe states without treating the object as true', async () => {
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<PermissionsStep />); });
  try {
    const check = async () => { await act(async () => { await view!.root.findByProps({ 'data-permission': 'System Audio' }).props.onClick(); }); };
    await check();
    expect(permissions.systemAudio).toBe('unknown');
    expect(storage.size).toBe(0);
    probe = new Error('unavailable');
    await check();
    expect(permissions.systemAudio).toBe('unknown');
    probe = verified;
    await check();
    expect(permissions.systemAudio).toBe('verified');
    probe = unknown;
    await check();
    expect(permissions.systemAudio).toBe('verified');
    probe = denied;
    await check();
    expect(permissions.systemAudio).toBe('denied');
  } finally { act(() => view!.unmount()); }
});

test('audio test retains verified evidence across silence and errors, but displays explicit denial', async () => {
  probe = verified;
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<AudioTestStep />); await settle(); });
  try {
    expect(text(view!)).toContain('Detected');
    const cached = storage.get(MACOS_SYSTEM_AUDIO_VERIFIED_KEY);
    const retest = async () => { await act(async () => {
      const button = view!.root.findAllByType('button').find(node => node.children.includes('Retest audio'))!;
      button.props.onClick(); await settle();
    }); };
    probe = unknown;
    await retest();
    expect(text(view!)).toContain('Detected');
    expect(storage.get(MACOS_SYSTEM_AUDIO_VERIFIED_KEY)).toBe(cached);
    probe = new Error('unavailable');
    await retest();
    expect(text(view!)).toContain('Detected');
    expect(storage.get(MACOS_SYSTEM_AUDIO_VERIFIED_KEY)).toBe(cached);
    probe = denied;
    await retest();
    expect(text(view!)).toContain('permission was denied');
    expect(text(view!)).not.toContain('Detected');
  } finally { await act(async () => { view!.unmount(); await settle(); }); }
});

test('audio test silence is neutral and does not cache denial', async () => {
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<AudioTestStep />); await settle(); });
  expect(text(view!)).toContain('Computer audio has not been verified');
  expect(text(view!)).not.toContain('permission was denied');
  expect(storage.size).toBe(0);
  await act(async () => { view!.unmount(); await settle(); });
});

test('unmount during native probe prevents late cache writes and meter startup', async () => {
  deferProbe = true;
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<AudioTestStep />); await settle(); });
  expect(resolveProbe).not.toBeNull();
  await act(async () => { view!.unmount(); resolveProbe!(verified); await settle(); });
  expect(storage.size).toBe(0);
  expect(calls).not.toContain('start_audio_level_monitoring');
});


test('unmount while native meter starts releases it after start resolves', async () => {
  probe = verified;
  deferMeter = true;
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<AudioTestStep />); await settle(); });
  expect(resolveMeter).not.toBeNull();
  await act(async () => { view!.unmount(); resolveMeter!(); await settle(); });
  expect(calls.filter(command => command === 'start_audio_level_monitoring')).toHaveLength(1);
  expect(calls.filter(command => command === 'stop_audio_level_monitoring')).toHaveLength(1);
});
