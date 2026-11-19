'use client';

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';

interface CodexStatus { installed: boolean; path: string | null; version: string | null; logged_in: boolean; error: string | null; auth_home: string; readiness: string }
const STATUS_COPY: Record<string, { title: string; description: string }> = {
  missing: { title: 'Codex CLI not found', description: 'Choose an installed codex executable or leave the path empty for discovery. No meeting text was sent.' },
  unsupported: { title: 'CLI is incompatible', description: 'This CLI does not support Meetily’s required execution capabilities. Choose another installed executable or update the CLI. No meeting text was sent.' },
  blocked: { title: 'Isolation blocked', description: 'The private CLI environment could not be verified. No meeting text was sent.' },
  error: { title: 'CLI check failed', description: 'Local readiness could not be confirmed. Check the executable path and try again. No meeting text was sent.' },
  signed_out: { title: 'Separate sign-in required', description: 'CLI supports Meetily’s mode. Sign in to ChatGPT in the private Meetily home, then check again.' },
  ready: { title: 'Ready for questions', description: 'Local checks passed. Requests use your ChatGPT subscription; network availability and quota are not confirmed.' },
};
function statusPresentation(status: CodexStatus) {
  const reportedReady = status.installed && status.logged_in && !status.error;
  const readiness = status.readiness === 'ready' && !reportedReady ? 'error' : status.readiness;
  const copy = STATUS_COPY[readiness] ?? STATUS_COPY.error;
  const compatible = readiness === 'ready' || readiness === 'signed_out';
  return {
    readiness,
    title: copy.title,
    description: compatible ? copy.description : status.error || copy.description,
    canSignIn: readiness === 'signed_out' && status.installed && !!status.path && !!status.auth_home,
    checks: [
      ['CLI installation', status.installed ? 'Found' : readiness === 'missing' ? 'Not found' : 'Not checked'],
      ['Meetily compatibility', compatible ? 'Supported' : readiness === 'unsupported' ? 'Not supported' : readiness === 'blocked' ? 'Blocked' : 'Not checked'],
      ['Isolated ChatGPT login', readiness === 'ready' ? 'Confirmed' : readiness === 'signed_out' ? 'Sign-in required' : 'Not checked'],
    ],
  };
}
// Display only: Meetily never executes a shell or this login command.
export const quotePosixArgument = (value: string) => "'" + value.replace(/'/g, "'\\''") + "'";
export function CodexCliSettings() {
  const [path, setPath] = useState('');
  const [status, setStatus] = useState<CodexStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);
  useEffect(() => {
    const id = ++generation.current;
    setBusy(true);
    void (async () => {
      try {
        const saved = await invoke<string | null>('codex_cli_get_path');
        if (generation.current !== id) return;
        setPath(saved ?? '');
        const ready = await invoke<CodexStatus>('codex_cli_get_status');
        if (generation.current === id) setStatus(ready);
      } catch (e) { if (generation.current === id) setError(String(e)); }
      finally { if (generation.current === id) setBusy(false); }
    })();
    return () => { ++generation.current; };
  }, []);
  const check = async (save: boolean) => {
    const id = ++generation.current;
    setBusy(true); setError(null); setStatus(null);
    try {
      if (save) await invoke('codex_cli_save_path', { path: path.trim() || null });
      const result = await invoke<CodexStatus>('codex_cli_get_status', { path: path.trim() });
      if (generation.current === id) setStatus(result);
    } catch (e) { if (generation.current === id) setError(String(e)); }
    finally { if (generation.current === id) setBusy(false); }
  };
  const presentation = status ? statusPresentation(status) : null;
  return <section className="space-y-4 rounded-2xl border border-af-border bg-af-panel-2/40 p-5">
    <h3 className="text-[15px] font-semibold">Live questions · Codex CLI</h3>
    <p className="text-sm text-af-text-3">Only your installed local Codex CLI with a ChatGPT subscription is used for live questions. Text from the current recording and separately committed personal Notes is sent to OpenAI when you ask. Audio stays local. Summary-provider/API settings are never a fallback.</p>
    <label className="block text-sm">Codex executable path
      <input aria-label="Codex executable path" value={path} disabled={busy} onChange={e => { ++generation.current; setBusy(false); setPath(e.target.value); setStatus(null); setError(null); }} placeholder="Auto-detect, or /opt/homebrew/bin/codex" className="mt-1 block w-full rounded-lg border border-af-border bg-af-panel px-3 py-2 text-sm" />
    </label>
    <p className="text-xs text-af-text-3">Save an absolute executable path so Dock launches do not depend on your shell PATH. Leave empty for discovery in standard install locations. Compatibility is checked by required capabilities, not one version number.</p>
    <div className="flex gap-2"><Button size="sm" disabled={busy} onClick={() => void check(true)}>Save and check</Button><Button size="sm" variant="secondary" disabled={busy} onClick={() => void check(false)}>Check path</Button></div>
    {busy && <p role="status">Checking local installation and login…</p>}
    {error && <p role="alert">{error}</p>}
    {status && presentation && <div className="space-y-3 text-xs">
      <div role={presentation.readiness === 'ready' || presentation.readiness === 'signed_out' ? 'status' : 'alert'} className="space-y-1 border-y border-af-border py-3">
        <h4 className="flex items-center gap-2 text-sm font-medium">
          <span aria-hidden="true" className={`h-2 w-2 shrink-0 rounded-full ${presentation.readiness === 'ready' ? 'bg-af-success' : presentation.readiness === 'signed_out' ? 'bg-af-warning' : 'bg-af-danger'}`} />
          {presentation.title}
        </h4>
        <p className="break-words text-af-text-3">{presentation.description}</p>
      </div>
      <details>
        <summary className="cursor-pointer rounded py-1 focus-visible:outline focus-visible:outline-2 focus-visible:outline-af-accent">What was checked</summary>
        <dl className="mt-2 space-y-2">
          {presentation.checks.map(([label, result]) => <div key={label} className="flex justify-between gap-4">
            <dt className="text-af-text-3">{label}</dt><dd className="text-right">{result}</dd>
          </div>)}
        </dl>
      </details>
      {presentation.canSignIn && <details>
        <summary className="cursor-pointer rounded py-1 focus-visible:outline focus-visible:outline-2 focus-visible:outline-af-accent">Separate ChatGPT sign-in</summary>
        <p className="mt-2 text-af-text-3">Run this command once in your terminal, then use Check path above. Meetily does not run the command. Choose ChatGPT subscription login, not an API key.</p>
        <pre aria-label="Isolated Codex sign-in command" className="mt-2 whitespace-pre-wrap break-all rounded bg-af-panel p-3">{`CODEX_HOME=${quotePosixArgument(status.auth_home)} ${quotePosixArgument(status.path!)} login`}</pre>
        <p className="mt-2 text-af-text-3">Existing general Codex credentials are never copied, read or linked. Use this private home only for Meetily sign-in; never copy general config, skills, credentials or AGENTS files into it.</p>
      </details>}
      <details>
        <summary className="cursor-pointer rounded py-1 focus-visible:outline focus-visible:outline-2 focus-visible:outline-af-accent">CLI path, version and isolation details</summary>
        <dl className="mt-2 space-y-2">
          <div><dt className="text-af-text-3">Detected executable</dt><dd className="break-all">{status.path || (presentation.readiness === 'missing' ? 'Not found' : 'Not checked')}</dd></div>
          <div><dt className="text-af-text-3">Reported version</dt><dd>{status.version || 'Not reported'}</dd></div>
          <div><dt className="text-af-text-3">Private CLI auth home</dt><dd className="break-all">{status.auth_home}</dd></div>
        </dl>
        <p className="mt-2 text-af-text-3">HOME is temporary; CODEX_HOME is dedicated to Meetily. Requests are text-only, with tools disabled and no provider fallback. AGENTS.md or AGENTS.override.md in the private auth home blocks requests.</p>
      </details>
    </div>}
    <p className="text-xs text-af-text-3">This local readiness check sends no meeting text and does not test network access or remaining quota. Failed/offline requests, cancellation and the 60-second deadline do not stop recording, local ASR or Notes.</p>
  </section>;
}
