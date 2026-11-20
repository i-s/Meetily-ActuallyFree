// Run this mock-heavy file separately from other component tests.
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { afterEach, beforeEach, expect, mock, test } from 'bun:test';

const calls: string[] = [];
let result: any;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string) => {
  calls.push(command);
  if (command === 'get_meeting_detection_settings') return {
    enabled: true, interval_secs: 15, meeting_apps: ['zoom', 'discord'], ignored_apps: [], notify: true,
  };
  if (command === 'get_meeting_detection_diagnostics') {
    if (result instanceof Error) throw result;
    return result;
  }
  if (command === 'request_meeting_detection_accessibility') return false;
  throw new Error(`Unexpected command: ${command}`);
} }));
mock.module('@/hooks/useLabs', () => ({ useLabs: () => ({ labs: { meetingAutomation: false } }) }));
mock.module('@/lib/labs-features', () => ({ setLabsFeature: async () => {} }));
mock.module('@/components/ui/switch', () => ({ Switch: () => <input type="checkbox" /> }));
mock.module('@/components/ui/badge', () => ({ Badge: ({ children }: any) => <span>{children}</span> }));
mock.module('sonner', () => ({ toast: { error() {} } }));
const { MeetingDetectionSettings } = await import('../../src/components/MeetingDetectionSettings');
let view: ReactTestRenderer;
const rendered = () => JSON.stringify(view.toJSON());
const button = (label: string) => view.root.findAllByType('button').find(node => node.children.includes(label))!;
beforeEach(() => {
  calls.length = 0;
  result = { platform: 'macos', accessibility_required: true, accessibility_granted: false,
    apps: [{ app: 'Zoom', state: 'Unknown', reason: 'Accessibility permission is required.' }] };
});
afterEach(async () => { await act(async () => view?.unmount()); });

test('diagnostics display readable evidence and prompt for Accessibility only on explicit action', async () => {
  await act(async () => { view = create(<MeetingDetectionSettings />); });
  expect(calls).toEqual(['get_meeting_detection_settings']);
  await act(async () => { await button('Check detection').props.onClick(); });
  expect(rendered()).toContain('Unknown');
  expect(rendered()).toContain('Accessibility permission is required.');
  expect(calls).not.toContain('request_meeting_detection_accessibility');
  await act(async () => { await button('Allow Accessibility').props.onClick(); });
  expect(calls.filter(call => call === 'request_meeting_detection_accessibility')).toHaveLength(1);
  expect(calls.filter(call => call === 'get_meeting_detection_diagnostics')).toHaveLength(2);
  expect(button('Allow Accessibility')).toBeDefined();
  result = { ...result, accessibility_granted: true,
    apps: [{ app: 'Zoom', state: 'Call detected', reason: 'In-call controls found.' }] };
  await act(async () => { await button('Refresh detection').props.onClick(); });
  expect(rendered()).toContain('Call detected');
  expect(rendered()).not.toContain('Accessibility permission is required.');
  expect(button('Allow Accessibility')).toBeUndefined();
});

test('Windows diagnostics do not offer macOS permission and can recover from a failed refresh', async () => {
  result = { platform: 'windows', accessibility_required: false, accessibility_granted: false,
    apps: [{ app: 'Discord', state: 'No call', reason: 'No active audio session.' }] };
  await act(async () => { view = create(<MeetingDetectionSettings />); });
  await act(async () => { await button('Check detection').props.onClick(); });
  expect(rendered()).toContain('No call');
  expect(button('Allow Accessibility')).toBeUndefined();
  result = new Error('private process detail');
  await act(async () => { await button('Refresh detection').props.onClick(); });
  expect(rendered()).toContain('Could not check detection. Try again.');
  expect(rendered()).not.toContain('private process detail');
  expect(rendered()).not.toContain('No call');
});
