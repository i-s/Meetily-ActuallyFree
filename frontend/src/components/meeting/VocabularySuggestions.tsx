'use client';

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { RememberTerm } from '@/components/meeting/RememberTerm';
import { suggestVocabulary, type SuggestionSegment, type VocabularySuggestion } from '@/lib/vocabulary-suggestions';

type Context = { version_id: string; revision: number; segments: SuggestionSegment[]; truncated: boolean };
type Result = { context: Context; candidates: VocabularySuggestion[]; truncated: boolean };
type Props = { meetingId: string; versionId: string; revision: number };

export function VocabularySuggestions(props: Props) {
  return <MeetingVocabularySuggestions key={props.meetingId} {...props} />;
}

function MeetingVocabularySuggestions({ meetingId, versionId, revision }: Props) {
  const [result, setResult] = useState<Result | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [manual, setManual] = useState(false);
  // Memory-only decisions: no rejection journal or automatic resurfacing.
  const suppressed = useRef(new Set<string>());
  const generation = useRef(0);
  const inFlight = useRef(false);
  useEffect(() => {
    generation.current++;
    inFlight.current = false;
    setResult(null); setBusy(false); setError(null); setSaved(false);
    return () => { generation.current++; };
  }, [versionId, revision]);
  const active = result?.context.version_id === versionId && result.context.revision === revision ? result : null;
  const scan = async () => {
    if (inFlight.current || !versionId) return;
    const ticket = ++generation.current;
    inFlight.current = true; setBusy(true); setResult(null); setError(null); setSaved(false);
    try {
      const [context, vocabulary] = await Promise.all([
        invoke<Context>('api_get_vocabulary_suggestion_context', { meetingId }),
        invoke<{ global: string; meeting: string }>('api_get_whisper_vocabulary', { meetingId }),
      ]);
      if (ticket !== generation.current) return;
      if (context.version_id !== versionId || context.revision !== revision) {
        throw new Error('Выбранный текст изменился. Обновите версии и повторите поиск терминов.');
      }
      const excluded = new Set([...suppressed.current,
        ...`${vocabulary.global}\n${vocabulary.meeting}`.split(/[,\n\r]/).map(t => t.trim().toLowerCase()).filter(Boolean)]);
      const selected = suggestVocabulary(context.segments, excluded);
      setResult({ context, candidates: selected.candidates, truncated: context.truncated || selected.truncated });
    } catch (cause) { if (ticket === generation.current) setError(String(cause)); }
    finally { if (ticket === generation.current) { inFlight.current = false; setBusy(false); } }
  };
  const remove = (candidate: VocabularySuggestion) => {
    suppressed.current.add(candidate.term.toLowerCase());
    setResult(current => current ? { ...current, candidates: current.candidates.filter(c => c.term !== candidate.term) } : null);
  };
  const confirm = async (candidate: VocabularySuggestion) => {
    if (inFlight.current || !active) return;
    const ticket = ++generation.current;
    inFlight.current = true; setBusy(true); setError(null); setSaved(false);
    try {
      await invoke('api_confirm_vocabulary_suggestion', { meetingId, versionId: active.context.version_id,
        revision: active.context.revision, transcriptId: candidate.transcriptId, term: candidate.term });
      if (ticket !== generation.current) return;
      remove(candidate); setSaved(true);
    } catch (cause) { if (ticket === generation.current) setError(String(cause)); }
    finally { if (ticket === generation.current) { inFlight.current = false; setBusy(false); } }
  };
  const close = () => {
    active?.candidates.forEach(c => suppressed.current.add(c.term.toLowerCase()));
    setResult(null); setError(null); setSaved(false);
  };
  return <section aria-label="Предложения для словаря" className="my-3 space-y-2 rounded-lg border border-af-border p-3 text-sm">
    <p>Предложения для словаря из выбранного сохранённого текста. Локальный поиск аббревиатур, CamelCase и обозначений с цифрами RU/EN — без ИИ и отправки данных. Написание из расшифровки может быть ошибочным: проверьте его перед подтверждением.</p>
    <p className="text-af-text-2">Из действий с предложением только «Запомнить написание» меняет словарь. Сканирование, отклонение и закрытие ничего не сохраняют. Нет уведомлений или автоматического поиска; отклонённые слова скрыты до ухода из этой встречи. Обычные имена и другие пропущенные термины можно добавить вручную.</p>
    <Button disabled={busy || !versionId} onClick={() => void scan()}>Предложить термины</Button>
    <Button variant="ghost" onClick={() => setManual(true)}>Добавить вручную</Button>
    {busy && <p role="status">Обработка запроса…</p>}
    {active && <>
      <p className="text-af-text-2">Не более 5 предложений; проверяются первые 2 000 строк / 100 000 символов Unicode выбранной расшифровки (дополнительный локальный лимит — 100 000 UTF-16 единиц).{active.truncated ? ' Показана ограниченная часть текста или предложений.' : ''}</p>
      <ul className="space-y-3">{active.candidates.map(candidate => <li key={candidate.term} className="rounded border border-af-border p-2">
        <strong>{candidate.term}</strong>
        <p className="text-xs text-af-text-2">Реплика {candidate.transcriptId}{candidate.audioStartTime != null ? ` · ${candidate.audioStartTime.toFixed(1)}s` : ''}</p>
        <blockquote className="my-1 whitespace-pre-wrap">{candidate.excerpt}</blockquote>
        <Button disabled={busy} onClick={() => void confirm(candidate)}>Запомнить написание</Button>
        <Button variant="ghost" disabled={busy} onClick={() => remove(candidate)}>Отклонить</Button>
      </li>)}</ul>
      {!active.candidates.length && <p role="status">Новых предложений нет. Можно добавить правильное написание вручную.</p>}
      <Button variant="ghost" disabled={busy} onClick={close}>Закрыть предложения</Button>
    </>}
    {saved && <p role="status">Подтверждение сохранено в личном словаре. Ранее подтверждённое написание сохраняет приоритет.</p>}
    {error && <p role="alert" className="text-af-danger">{error} Ручное добавление остаётся доступным; повторите поиск или подтверждение явно.</p>}
    {manual && <RememberTerm onDismiss={() => setManual(false)} />}
  </section>;
}
