'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { DiscordCallStatus } from '@/lib/discord-automation';

const MARKERS = ['idle', 'settings', 'mic_test', 'ringing', 'joined', 'joined_muted', 'muted', 'unmuted', 'silence', 'channel_changed', 'device_changed', 'reconnecting', 'reconnected', 'left_app_open', 'client_quit', 'permission_lost', 'permission_restored'];
interface JournalReport { scenario: string; active: boolean; truncated: boolean; entries: unknown[] }

/** Observation never calls the recorder. Only finite truth labels cross to native. */
export function DiscordObservation({ status, automationOn }: { status: DiscordCallStatus | null; automationOn: boolean }) {
  const [scenario, setScenario] = useState('guild_voice');
  const [journal, setJournal] = useState<JournalReport | null>(null);
  const [count, setCount] = useState(0);
  const [lastMark, setLastMark] = useState('');
  const [output, setOutput] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const apply = (report: JournalReport | null) => { setJournal(report); setCount(report?.entries.length ?? 0); };
  useEffect(() => {
    let disposed = false;
    void invoke<JournalReport | null>('get_discord_observation').then((report) => { if (!disposed) apply(report); })
      .catch((e) => { if (!disposed) setError(String(e)); });
    return () => { disposed = true; };
  }, []);
  async function command(action: () => Promise<void>) {
    setBusy(true); setError('');
    try { await action(); } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  const button = 'rounded-lg border border-af-border px-2 py-1 text-sm disabled:opacity-40';
  return <div className="mt-4 space-y-3 border-t border-af-border pt-4" aria-label="Discord observation journal">
    <p className="text-sm text-af-text-2">
      Platform reader: {status?.platform_supported === true ? 'available on macOS (not call qualification)' : status?.platform_supported === false ? 'unsupported on this OS' : 'unknown'}.
      {' '}Observation: {status?.enabled ? 'enabled' : 'disabled'}.
      {' '}Stable client: {status?.client_present === true ? 'present' : status?.client_present === false ? 'absent (not a joined-call claim)' : 'not observed / unknown'}.
      {' '}Call support: {status?.qualified ? 'qualified by adapter policy' : 'unqualified — automatic recording blocked'}.
      {' '}Candidate signal: {status?.state ?? 'unknown'}; cue change: {status?.cue_transition ?? 'none'}.
    </p>
    <p className="text-sm text-af-text-3">
      Observation only: turn Labs automation off and stop any recording first. This journal never starts audio capture.
      Choose a scenario manually; no call type is inferred. Native memory holds at most 4096 entries, with no raw AX labels,
      chat content or audio. It survives page changes, not app quit. Stop and export before clearing or quitting Meetily.
      Each marker timestamps manual truth at native receipt; mark immediately after verifying the Discord state.
    </p>
    <label className="block text-sm">Manual scenario{' '}
      <select aria-label="Discord observation scenario" className={button} value={scenario} disabled={busy || !!journal}
        onChange={(e) => setScenario(e.target.value)}>
        <option value="guild_voice">Guild voice</option><option value="dm">DM</option><option value="group_dm">Group DM</option>
      </select>
    </label>
    <div className="flex flex-wrap gap-2">
      <button className={button} disabled={busy || !!journal || automationOn || !status?.enabled || !status?.platform_supported}
        onClick={() => void command(async () => { apply(await invoke<JournalReport>('start_discord_observation', { scenario })); setOutput(''); setLastMark(''); })}>Start observation scenario</button>
      <button className={button} disabled={busy || !journal?.active}
        onClick={() => void command(async () => apply(await invoke<JournalReport>('stop_discord_observation')))}>Stop observation</button>
      <button className={button} disabled={busy || !journal}
        onClick={() => void command(async () => {
          const report = await invoke<JournalReport | null>('get_discord_observation'); apply(report);
          setOutput(JSON.stringify(report, null, 2));
        })}>Export observation JSON</button>
      <button className={button} disabled={busy || !journal || journal.active}
        onClick={() => { if (window.confirm('Clear this observation journal from memory? Export it first if needed.')) void command(async () => {
          await invoke('clear_discord_observation'); apply(null); setOutput(''); setLastMark('');
        }); }}>Clear observation journal</button>
    </div>
    {journal && <>
      <p className="text-sm" role="status">Scenario {journal.scenario}: {journal.active ? 'collecting in native memory' : 'stopped'};
        {' '}last retrieved count {count}. {journal.truncated ? 'FULL / TRUNCATED — not a qualification pass.' : ''} {lastMark}</p>
      <div className="flex flex-wrap gap-2">{MARKERS.map((truth) => <button key={truth} className={button}
        disabled={busy || !journal.active} onClick={() => void command(async () => {
          const result = await invoke<{ entries: number; last_at_ms: number }>('mark_discord_observation', { truth });
          setCount(result.entries); setLastMark(`Manual truth ${truth} @ ${result.last_at_ms} ms`);
        })}>{truth}</button>)}</div>
    </>}
    {status?.recording_active && <p role="alert">Recording is active. Do not use this run as observation-only qualification evidence.</p>}
    {error && <p role="alert">{error}</p>}
    {output && <label className="block text-sm">Local observation JSON — select all, copy and save as UTF-8 .json
      <textarea aria-label="Discord observation export" readOnly value={output} rows={10} className="mt-1 w-full rounded-lg border border-af-border bg-af-panel-2 p-2 font-mono text-xs" />
    </label>}
  </div>;
}
