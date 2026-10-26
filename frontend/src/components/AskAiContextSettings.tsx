'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ASK_AI_SETTINGS_CHANGED } from '@/lib/ask-ai-context';
import type { AskAiContextSettings as Settings } from '@/lib/workspace-api';

export function AskAiContextSettings() {
  const [mode, setMode] = useState<Settings['mode']>('recent');
  const [limit, setLimit] = useState('10000');
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  useEffect(() => {
    let alive = true;
    invoke<Settings>('get_ask_ai_context_settings').then(value => {
      if (alive) { setMode(value.mode); setLimit(String(value.maxChars)); setLoaded(true); }
    }).catch(error => { if (alive) setError(String(error)); });
    return () => { alive = false; };
  }, []);
  return <form className="rounded-2xl border border-af-border bg-af-panel-2/40 p-5 space-y-3" onSubmit={async event => {
    event.preventDefault();
    const maxChars = Number(limit);
    if (!Number.isSafeInteger(maxChars) || maxChars <= 0) { setError('Введите положительное целое число символов.'); return; }
    setSaving(true); setError(null); setSaved(false);
    try {
      await invoke('set_ask_ai_context_settings', { settings: { mode, maxChars } });
      window.dispatchEvent(new Event(ASK_AI_SETTINGS_CHANGED));
      setSaved(true);
    } catch (error) { setError(String(error)); }
    finally { setSaving(false); }
  }}>
    <h3 className="text-[15px] font-semibold text-af-text">Контекст Ask AI</h3>
    <label className="block text-sm text-af-text-2">Транскрипт
      <select aria-label="Контекст транскрипта Ask AI" value={mode} disabled={!loaded || saving}
        onChange={e => { setMode(e.target.value as Settings['mode']); setSaved(false); }}
        className="mt-1 block w-full rounded-lg border border-af-border bg-af-panel p-2">
        <option value="recent">Последние N символов</option><option value="all">Весь транскрипт</option>
      </select>
    </label>
    <label className="block text-sm text-af-text-2">Количество символов
      <input aria-label="Количество символов Ask AI" type="number" min={1} step={1} value={limit}
        disabled={!loaded || saving || mode === 'all'} onChange={e => { setLimit(e.target.value); setSaved(false); }}
        className="mt-1 block w-full rounded-lg border border-af-border bg-af-panel p-2" />
    </label>
    <p className="text-xs text-af-text-3">10 000 символов — примерно 10 минут, но не гарантия длительности. Meeting notes всегда передаются целиком. Итоговая Summary использует весь транскрипт независимо от этой настройки.</p>
    {error && <p role="alert" className="text-sm text-red-500">{error}</p>}
    {saved && <p role="status" className="text-xs text-af-text-3">Сохранено</p>}
    <button type="submit" disabled={!loaded || saving} className="rounded-lg bg-af-accent px-3 py-2 text-sm text-white disabled:opacity-50">{saving ? 'Сохранение…' : 'Сохранить контекст Ask AI'}</button>
  </form>;
}
