'use client';
import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { completeSummaryMarkdown, parseSummaryData } from '@/lib/summary-markdown';
import { listActionItems, onWorkspaceChange, type ActionItem } from '@/lib/workspace-api';

interface LiveSummary { meetingId: string | null; markdown: string; items: ActionItem[]; error: string | null; }
export function useLiveMeetingSummary(meetingId: string | null) {
  const [summary, setSummary] = useState<LiveSummary>({ meetingId, markdown: '', items: [], error: null });
  const request = useRef(0);
  const refresh = useCallback(async () => {
    if (!meetingId) return;
    const version = ++request.current;
    try {
      const [response, items] = await Promise.all([
        invoke<{ data?: unknown }>('api_get_summary', { meetingId }),
        listActionItems({ meetingId }),
      ]);
      if (version !== request.current) return;
      setSummary({ meetingId, markdown: completeSummaryMarkdown(parseSummaryData(response.data)), items, error: null });
    } catch (error) {
      if (version === request.current) setSummary(previous => ({ ...previous, error: String(error) }));
    }
  }, [meetingId]);
  useEffect(() => {
    void refresh();
    const timer = setInterval(() => { void refresh(); }, 5000);
    const unlisten = listen('summary-progress', () => { void refresh(); });
    const unsubscribe = onWorkspaceChange(['actions', 'meetings'], () => { void refresh(); });
    return () => { request.current++; clearInterval(timer); unsubscribe(); void unlisten.then(stop => stop()).catch(() => {}); };
  }, [refresh]);
  return { ...(summary.meetingId === meetingId ? summary : { meetingId, markdown: '', items: [], error: null }), refresh };
}
export function LiveMeetingSummary({ markdown, items, error, refresh }: ReturnType<typeof useLiveMeetingSummary>) {
  return <div className="h-full overflow-y-auto px-5 py-4">
    {error && <p role="alert" className="mb-3 text-xs text-af-danger">Could not refresh summary. <button type="button" className="underline" onClick={() => void refresh()}>Retry</button></p>}
    <div className="prose prose-sm max-w-none"><ReactMarkdown remarkPlugins={[remarkGfm]}>{markdown}</ReactMarkdown></div>
    {items.length > 0 && <section className="mt-6"><h3 className="mb-2 text-sm font-medium">Tasks</h3><ul className="space-y-2 text-sm">{items.map(item => <li key={item.id} className={item.done ? 'text-af-text-3 line-through' : ''}>
      <span aria-label={item.done ? 'Completed' : 'Open'}>{item.done ? '✓' : '○'}</span> {item.text}{item.ownerLabel && <span className="text-xs text-af-text-3"> · {item.ownerLabel}</span>}
    </li>)}</ul></section>}
  </div>;
}
