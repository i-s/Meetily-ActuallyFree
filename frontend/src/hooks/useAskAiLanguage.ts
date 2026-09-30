import { useEffect, useState } from 'react';
import { resolveAskAiLanguage } from '@/lib/ask-ai-language';
import { SUMMARY_LANGUAGE_CHANGED_EVENT, readPinnedSummaryLanguageDefault } from '@/lib/summary-language-preferences';

/** One language for visible presets and the IPC request, refreshed when the summary preference changes. */
export function useAskAiLanguage(meetingId: string | undefined, lines: ReadonlyArray<{ text: string }>) {
  const [selection, setSelection] = useState(() => ({
    key: '', language: meetingId ? 'en' : readPinnedSummaryLanguageDefault() ?? 'en',
  }));
  const [revision, setRevision] = useState(0);
  const selectionKey = JSON.stringify([meetingId ?? null, revision]);
  const transcriptTexts = JSON.stringify(lines.map(line => line.text));
  useEffect(() => {
    const refresh = () => setRevision(value => value + 1);
    window.addEventListener(SUMMARY_LANGUAGE_CHANGED_EVENT, refresh);
    window.addEventListener('storage', refresh);
    return () => {
      window.removeEventListener(SUMMARY_LANGUAGE_CHANGED_EVENT, refresh);
      window.removeEventListener('storage', refresh);
    };
  }, []);
  useEffect(() => {
    let cancelled = false;
    // Avoid invoking detection for each render during live transcript bursts.
    const timer = setTimeout(() => {
      void resolveAskAiLanguage(meetingId, JSON.parse(transcriptTexts)).then(next => {
        if (!cancelled) setSelection({ key: selectionKey, language: next });
      });
    }, 150);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [meetingId, transcriptTexts, selectionKey]);
  return { language: selection.language, ready: selection.key === selectionKey };
}
