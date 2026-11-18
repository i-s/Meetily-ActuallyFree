'use client';

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';

interface CodexStatus { installed: boolean; path: string | null; version: string | null; logged_in: boolean; error: string | null; auth_home: string; readiness: string }
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
  return <section className="space-y-4 rounded-2xl border border-af-border bg-af-panel-2/40 p-5">
    <h3 className="text-[15px] font-semibold">Live questions · Codex CLI</h3>
    <p className="text-sm text-af-text-3">Only your installed local Codex CLI with a ChatGPT subscription is used for live questions. Text from the current recording and separately committed personal Notes is sent to OpenAI when you ask. Audio stays local. Summary-provider/API settings are never a fallback.</p>
    <label className="block text-sm">Codex executable path
      <input aria-label="Codex executable path" value={path} disabled={busy} onChange={e => { ++generation.current; setPath(e.target.value); setStatus(null); setError(null); }} placeholder="Auto-detect, or /opt/homebrew/bin/codex" className="mt-1 block w-full rounded-lg border border-af-border bg-af-panel px-3 py-2 text-sm" />
    </label>
    <p className="text-xs text-af-text-3">Save an absolute executable path so Dock launches do not depend on your shell PATH. Leave empty for discovery in standard install locations. Requires the tested Codex CLI 0.167.2. Sign in separately to the private live-questions auth home using the exact command below and ChatGPT, not an API key. Existing general Codex credentials are never copied, read or linked.</p>
    <div className="flex gap-2"><Button size="sm" disabled={busy} onClick={() => void check(true)}>Save and check</Button><Button size="sm" variant="secondary" disabled={busy} onClick={() => void check(false)}>Check path</Button></div>
    {busy && <p role="status">Checking local installation and login…</p>}
    {error && <p role="alert">{error}</p>}
    {status && <div role={status.error ? 'alert' : 'status'} className="space-y-1 text-xs">
      <p>Readiness: {status.readiness.replace('_', ' ')} · {status.logged_in ? 'ChatGPT subscription login confirmed in isolated home' : 'ChatGPT subscription login not confirmed in isolated home'}</p>
      {status.path && <p className="break-all">{status.path}</p>}{status.version && <p>{status.version}</p>}
      <p className="break-all">Private CLI auth home: {status.auth_home}</p>
      <p>One-time owner action in a terminal (not run by Meetily):</p>
      <pre aria-label="Isolated Codex sign-in command" className="whitespace-pre-wrap break-all rounded bg-af-panel p-2">{`CODEX_HOME=${quotePosixArgument(status.auth_home)} ${quotePosixArgument(status.path || path.trim() || 'codex')} login`}</pre>
      <p>Use this private directory only for this CLI login. AGENTS.md or AGENTS.override.md there blocks all requests; never copy general Codex config, skills or credentials into it.</p>
      {status.error && <p>{status.error}</p>}
      <p>This local readiness check sends no meeting text and does not test network access or remaining quota. Failed/offline requests, cancellation and the 60-second deadline do not stop recording, local ASR or Notes.</p>
    </div>}
  </section>;
}
