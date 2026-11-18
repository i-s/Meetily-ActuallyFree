import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
const calls: { name: string; args: any }[] = [];
let downloaded = '';
const noop = () => {};
Object.assign(globalThis, { window: { confirm: () => true }, document: { createElement: () => ({ href: '', download: '', click: () => { downloaded = 'journal'; } }) } });
URL.createObjectURL = () => 'blob:synthetic'; URL.revokeObjectURL = noop;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  if (!['start_zoom_observation', 'mark_zoom_observation', 'stop_zoom_observation', 'export_zoom_observation', 'clear_zoom_observation'].includes(name)) throw new Error(`Unexpected capture command ${name}`);
  calls.push({ name, args });
  if (name === 'export_zoom_observation') return '{"schema":1,"qualified":false}';
} }));
mock.module('sonner', () => ({ toast: { success: noop, error: noop } }));
const { ZoomObservationPanel } = await import('../../src/components/ZoomObservationPanel');

test('normal observation controls collect timestamp-journal metadata and truth without capture commands or qualification override', async () => {
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<ZoomObservationPanel enabled={true} />); });
  const button = (text: string) => view!.root.findAllByType('button').find(b => b.children.includes(text))!;
  try {
    expect(JSON.stringify(view!.toJSON())).toContain('observation only');
    await act(async () => {
      view!.root.findByProps({ 'aria-label': 'Zoom version' }).props.onChange({ target: { value: '6.4.0' } });
      view!.root.findByProps({ 'aria-label': 'macOS version' }).props.onChange({ target: { value: '15.1' } });
    });
    await act(async () => { await button('Start observation journal').props.onClick(); });
    expect(calls[0]).toEqual({ name: 'start_zoom_observation', args: { metadata: { machine: 'm4_pro48_gb', route: 'speakers', client_version: '6.4.0', macos_version: '15.1', ui_language: 'english', accessibility: 'unknown' } } });
    for (const truth of ['preview', 'waiting', 'joined_muted', 'left_open', 'permission_lost']) {
      await act(async () => { view!.root.findByProps({ 'aria-label': 'Observed Zoom ground truth' }).props.onChange({ target: { value: truth } }); });
      await act(async () => { await button('Mark observed state').props.onClick(); });
      expect(calls.at(-1)).toEqual({ name: 'mark_zoom_observation', args: { truth } });
    }
    await act(async () => { await button('Stop journal').props.onClick(); });
    await act(async () => { await button('Export journal JSON').props.onClick(); });
    expect(downloaded).toBe('journal');
    expect(calls.some(c => c.name.includes('recording') || c.name.includes('qualified'))).toBe(false);
    await act(async () => { view!.update(<ZoomObservationPanel enabled={false} />); });
    expect(button('Start observation journal').props.disabled).toBe(true);
  } finally { act(() => view!.unmount()); }
});
