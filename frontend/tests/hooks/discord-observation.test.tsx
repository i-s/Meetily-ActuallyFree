import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
import type { DiscordCallStatus } from '../../src/lib/discord-automation';
let journal: any = null;
const calls: string[] = [];
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true, window: { confirm: () => true } });
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args?: any) => {
  calls.push(command);
  if (command === 'get_discord_observation') return journal;
  if (command === 'start_discord_observation') {
    journal = { schema: 1, mode: 'observation_only', active: true, truncated: false, scenario: args.scenario, entries: [] }; return journal;
  }
  if (command === 'mark_discord_observation') {
    journal.entries.push({ truth: args.truth, at_ms: 100 }); return { entries: journal.entries.length, last_at_ms: 100 };
  }
  if (command === 'stop_discord_observation') { journal = { ...journal, active: false }; return journal; }
  if (command === 'clear_discord_observation') { journal = null; return; }
  throw new Error(`Observation attempted unrelated command ${command}`);
} }));
const { DiscordObservation } = await import('../../src/components/DiscordObservation');
const base: DiscordCallStatus = { enabled: true, qualified: false, platform_supported: true, client_present: false,
  state: 'inactive', reason: 'stable_client_not_running', transition: 'none', session_id: null };
function button(view: ReturnType<typeof create>, text: string) {
  return view.root.findAllByType('button').find((node) => node.children.join('') === text)!;
}

test('normal observation UI collects a manually chosen scenario and truth, exports only through journal commands, and never starts recording', async () => {
  let view: ReturnType<typeof create> | undefined;
  try {
    await act(async () => { view = create(<DiscordObservation status={base} automationOn={false} />); });
    let text = JSON.stringify(view!.toJSON());
    expect(text).toContain('absent (not a joined-call claim)');
    expect(text).toContain('unqualified — automatic recording blocked');
    const select = view!.root.findByProps({ 'aria-label': 'Discord observation scenario' });
    await act(async () => select.props.onChange({ target: { value: 'group_dm' } }));
    await act(async () => { await button(view!, 'Start observation scenario').props.onClick(); });
    expect(journal.scenario).toBe('group_dm');
    await act(async () => { await button(view!, 'left_app_open').props.onClick(); });
    expect(journal.entries[0].truth).toBe('left_app_open');
    await act(async () => { await button(view!, 'Stop observation').props.onClick(); });
    await act(async () => { await button(view!, 'Export observation JSON').props.onClick(); });
    const output = JSON.parse(view!.root.findByProps({ 'aria-label': 'Discord observation export' }).props.value);
    expect(output.mode).toBe('observation_only'); expect(output.active).toBe(false);
    expect(calls.every((name) => /discord_observation$/.test(name))).toBe(true);
    await act(async () => { await button(view!, 'Clear observation journal').props.onClick(); });
    expect(journal).toBeNull();

    await act(async () => view!.update(<DiscordObservation status={{ ...base, enabled: false, client_present: null }} automationOn={false} />));
    expect(button(view!, 'Start observation scenario').props.disabled).toBe(true);
    text = JSON.stringify(view!.toJSON()); expect(text).toContain('disabled'); expect(text).toContain('not observed / unknown');
    await act(async () => view!.update(<DiscordObservation status={{ ...base, platform_supported: false }} automationOn={false} />));
    expect(button(view!, 'Start observation scenario').props.disabled).toBe(true);
    expect(JSON.stringify(view!.toJSON())).toContain('unsupported on this OS');
    await act(async () => view!.update(<DiscordObservation status={base} automationOn={true} />));
    expect(button(view!, 'Start observation scenario').props.disabled).toBe(true);
  } finally { await act(async () => view?.unmount()); }
}, 120_000);
