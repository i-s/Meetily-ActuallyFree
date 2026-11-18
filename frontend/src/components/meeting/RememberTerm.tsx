'use client';

import { useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';

/** Explicit confirmation, optionally after an independent correction commit.
 * Never writes transcript rows, even if vocabulary saving or refresh fails. */
export function RememberTerm({ correctedText, onDismiss }: { correctedText?: string; onDismiss: () => void }) {
  const [term, setTerm] = useState('');
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);
  const confirm = async () => {
    if (inFlight.current || saved || !term.trim()) return;
    inFlight.current = true;
    setBusy(true); setError(null);
    try {
      await invoke('api_confirm_vocabulary_term', { term: term.trim() });
      setSaved(true);
    } catch (cause) { setError(String(cause)); }
    finally { inFlight.current = false; setBusy(false); }
  };
  return <section aria-label="Запомнить термин" className="my-3 space-y-2 rounded-lg border border-af-border p-3 text-sm">
    <p>{correctedText !== undefined ? 'Исправление текста сохранено независимо от словаря. Хотите запомнить правильное написание для следующих встреч?' : 'Введите правильное написание для следующих встреч. Только явное подтверждение добавляет термин в словарь.'}</p>
    {correctedText !== undefined && <details><summary>Сохранённый текст</summary><p className="whitespace-pre-wrap">{correctedText}</p></details>}
    <label className="block">Термин, имя или аббревиатура RU/EN
      <input aria-label="Правильное написание термина" className="block w-full rounded border border-af-border bg-af-panel p-2" value={term} disabled={busy || saved} onChange={(event) => setTerm(event.target.value)} />
    </label>
    <p className="text-af-text-2">Только подтверждённое написание. Повтор сохраняет первое подтверждённое написание без дублей. Whisper использует подсказки (до 224 токенов, начало списка); Parakeet их не поддерживает. Codex получает словарь отдельно от речи и Notes. Это не обучение весов и не гарантия точности. Изменить или удалить запись можно в Settings → Transcription → Personal vocabulary.</p>
    {saved ? <p role="status">Термин сохранён в личный словарь.</p> : <Button disabled={busy || !term.trim()} onClick={() => void confirm()}>Запомнить</Button>}
    <Button variant="ghost" disabled={busy} onClick={onDismiss}>{saved ? 'Закрыть' : 'Не запоминать'}</Button>
    {error && <p role="alert" className="text-af-danger">{error}{correctedText !== undefined ? ' Исправление текста уже сохранено.' : ''}</p>}
  </section>;
}
