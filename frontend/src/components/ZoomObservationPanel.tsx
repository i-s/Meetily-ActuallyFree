'use client';

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

const truths = ['idle', 'preview', 'waiting', 'joined', 'joined_muted', 'silence', 'reconnecting', 'device_changed', 'left_open', 'permission_lost'] as const;

/** Read-only evidence collection for the single final combined Mac build.
 * Human markers are truth annotations, never inputs to automatic recording.
 */
export function ZoomObservationPanel({ enabled }: { enabled: boolean }) {
  const [machine, setMachine] = useState('m4_pro48_gb');
  const [route, setRoute] = useState('speakers');
  const [language, setLanguage] = useState('english');
  const [permission, setPermission] = useState('unknown');
  const [clientVersion, setClientVersion] = useState('');
  const [macosVersion, setMacosVersion] = useState('');
  const [truth, setTruth] = useState<string>('idle');
  const [busy, setBusy] = useState(false);
  const action = async (name: string, args?: Record<string, unknown>) => {
    setBusy(true);
    try {
      const result = await invoke<string | void>(name, args);
      if (name === 'export_zoom_observation' && typeof result === 'string') {
        const url = URL.createObjectURL(new Blob([result], { type: 'application/json' }));
        const link = document.createElement('a');
        link.href = url; link.download = 'zoom-observation.json'; link.click();
        URL.revokeObjectURL(url);
      }
      toast.success('Zoom observation journal updated');
    } catch (error) { toast.error('Zoom observation journal', { description: String(error) }); }
    finally { setBusy(false); }
  };
  return <section className="rounded-xl border border-af-border p-4 space-y-3" aria-label="Zoom observation qualification">
    <h3 className="font-semibold text-af-text">Zoom: observation only — physical qualification required</h3>
    <p className="text-sm text-af-text-2">No real client class is qualified to start capture, even if old automation settings are enabled. Enable Meeting Detection above to observe. Use the single final combined Mac build; do not make a separate installer for this journal.</p>
    <p className="text-sm text-af-text-2">Start idle. Mark actual preview, waiting, join (also already muted), silence, reconnect, device change, leave with Zoom open, and permission loss. Mark what happened, not a predicted click. Compare timestamped truth to observations; Unknown is neither join nor exit. Repeat on both Macs. This journal does not certify qualification or identify speakers.</p>
    <div className="flex flex-wrap gap-2">
      <select aria-label="Qualification machine" value={machine} onChange={e => setMachine(e.target.value)}>
        <option value="m4_pro48_gb">M4 Pro / 48 GB</option><option value="m1_pro32_gb">M1 Pro / 32 GB</option><option value="other">Other</option>
      </select>
      <select aria-label="Qualification route" value={route} onChange={e => setRoute(e.target.value)}>
        <option value="speakers">Speakers</option><option value="headset">Headset</option><option value="other">Other</option><option value="unknown">Unknown</option>
      </select>
      <select aria-label="Zoom UI language" value={language} onChange={e => setLanguage(e.target.value)}>
        <option value="english">English</option><option value="russian">Russian</option><option value="other">Other</option>
      </select>
      <select aria-label="Zoom observation Accessibility permission" value={permission} onChange={e => setPermission(e.target.value)}>
        <option value="unknown">AX permission unknown</option><option value="granted">AX granted</option><option value="denied">AX denied</option>
      </select>
      <input aria-label="Zoom version" placeholder="Zoom version" value={clientVersion} maxLength={64} onChange={e => setClientVersion(e.target.value)} />
      <input aria-label="macOS version" placeholder="macOS version/build" value={macosVersion} maxLength={64} onChange={e => setMacosVersion(e.target.value)} />
    </div>
    <div className="flex flex-wrap gap-3 text-sm">
      <button disabled={busy || !enabled} onClick={() => void action('start_zoom_observation', { metadata: { machine, route, client_version: clientVersion, macos_version: macosVersion, ui_language: language, accessibility: permission } })}>Start observation journal</button>
      <select aria-label="Observed Zoom ground truth" value={truth} onChange={e => setTruth(e.target.value)}>{truths.map(value => <option key={value} value={value}>{value.replaceAll('_', ' ')}</option>)}</select>
      <button disabled={busy} onClick={() => void action('mark_zoom_observation', { truth })}>Mark observed state</button>
      <button disabled={busy} onClick={() => void action('stop_zoom_observation')}>Stop journal</button>
      <button disabled={busy} onClick={() => void action('export_zoom_observation')}>Export journal JSON</button>
      <button disabled={busy} onClick={() => { if (window.confirm('Discard the in-memory Zoom observation journal? Export it first to keep the evidence.')) void action('clear_zoom_observation'); }}>Clear journal</button>
    </div>
    <p className="text-xs text-af-text-3">Local, bounded in-memory metadata; export before app exit. No AX labels/window titles, audio or transcript content. Export never enables recording.</p>
  </section>;
}
