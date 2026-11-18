import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
let settings: any = { enabled: true, interval_secs: 15, meeting_apps: ['zoom'], ignored_apps: [], notify: true, discord_automation: false };
const listeners = new Map<string, (event: any) => void>();
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true, window: new EventTarget(), sessionStorage: { getItem: () => null } });
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args?: any) => {
  if (name === 'get_meeting_detection_settings') return settings;
  if (name === 'set_meeting_detection_settings') { settings = args.settings; return; }
  if (name === 'get_meeting_lifecycle_snapshot') return { revision: 1, enabled: true, automation_enabled: false, sources: [] };
  if (name === 'get_discord_observation') return null;
  if (name === 'get_discord_automation_status') return { enabled: false, qualified: false, platform_supported: false, client_present: null, state: 'unknown', reason: 'disabled', session_id: null, transition: 'none' };
  throw new Error(`unexpected ${name}`);
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async (name: string, callback: any) => { listeners.set(name, callback); return () => {}; } }));
mock.module('@/hooks/useLabs', () => ({ useLabs: () => ({ labs: { meetingAutomation: false } }) }));
mock.module('@/lib/labs-features', () => ({ setLabsFeature: async () => {} }));
mock.module('@/components/ui/switch', () => ({ Switch: (props: any) => <button {...props} /> }));
const { MeetingDetectionSettings } = await import('../../src/components/MeetingDetectionSettings');

test('normal settings opt in to read-only observation and display Unknown/qualification limits', async () => {
  let view: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { view = create(<MeetingDetectionSettings />); });
    const toggle = () => view!.root.findByProps({ 'aria-label': 'Observe Discord macOS call signals' });
    expect(toggle().props.checked).toBe(false);
    await act(async () => { await toggle().props.onCheckedChange(true); });
    expect(settings.discord_automation).toBe(true);
    expect(settings.meeting_apps).toEqual(['zoom']);
    await act(async () => { listeners.get('discord-call-state')!({ payload: { enabled: true, qualified: false, state: 'unknown', reason: 'accessibility_permission_required' } }); });
    const text = JSON.stringify(view!.toJSON());
    expect(text).toContain('accessibility_permission_required');
    expect(text).toContain('blocked until the signal is qualified');
    expect(text).toContain('Guild voice, DM and group DM are each unqualified');
  } finally { await act(async () => view?.unmount()); }
}, 120_000);

test('full consumed-session history is visibly unavailable with safe recovery, not a qualification/reset override', async () => {
  const getItem = sessionStorage.getItem;
  sessionStorage.getItem = (key: string) => key === 'labsAutoStartConsumed' ? JSON.stringify(Array.from({ length: 256 }, (_, i) => `consumed-${i}`)) : null;
  let view: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { view = create(<MeetingDetectionSettings />); });
    const text = JSON.stringify(view!.toJSON());
    expect(text).toContain('256-session safety history is full');
    expect(text).toContain('fully quit and reopen Meetily');
    expect(text).toContain('Manual controls remain available');
    expect(text).toContain('blocked until the signal is qualified');
  } finally { sessionStorage.getItem = getItem; await act(async () => view?.unmount()); }
}, 120_000);
