'use client';
import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { ChatThread, type ChatMessage } from './ChatThread';
import { getMeetingChat, sendMeetingChat } from '@/lib/workspace-api';
import { flushMeetingNotes } from '@/lib/meeting-notes-store';
import { askAiCopy } from '@/lib/ask-ai-language';

export function MeetingChat(props: { meetingId: string; language: string; disabled?: boolean; compact?: boolean; onSeek?: (seconds: number) => void }) {
  // A distinct mount also isolates in-flight sends and errors from navigation.
  return <MeetingChatSession key={props.meetingId} {...props} />;
}
function MeetingChatSession({ meetingId, language, disabled, compact, onSeek }: { meetingId: string; language: string; disabled?: boolean; compact?: boolean; onSeek?: (seconds: number) => void }) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [sending, setSending] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const lastQuestion = useRef('');
  const active = useRef(true);
  const sendingRef = useRef(false);
  const request = useRef(0);
  const refresh = useCallback(async () => {
    const version = ++request.current;
    try {
      const turns = await getMeetingChat(meetingId);
      if (!active.current || version !== request.current) return;
      setMessages(turns.map(turn => ({ id: turn.id, question: turn.question, answer: turn.answer ?? turn.error ?? undefined, status: turn.status === 'completed' ? 'done' : turn.status === 'pending' ? 'pending' : 'error', context: turn.context })));
      setLoaded(true); setLoadError(null);
    } catch (cause) {
      if (active.current && version === request.current) setLoadError(String(cause));
    }
  }, [meetingId]);
  useEffect(() => {
    active.current = true;
    void refresh();
    const unlisten = listen<{ meetingId: string }>('meeting-chat-updated', event => { if (event.payload.meetingId === meetingId) void refresh(); });
    // Polling recovers events missed while the WebView was suspended.
    const timer = setInterval(() => { void refresh(); }, 3000);
    return () => { active.current = false; clearInterval(timer); void unlisten.then(stop => stop()).catch(() => {}); };
  }, [meetingId, refresh]);
  const send = async (question: string) => {
    if (sendingRef.current) return false;
    sendingRef.current = true; setSending(true); setSendError(null); lastQuestion.current = question;
    try {
      await flushMeetingNotes(meetingId);
      await sendMeetingChat(meetingId, question, language);
      await refresh();
      return true;
    } catch (cause) { if (active.current) setSendError(String(cause)); return false; }
    finally { sendingRef.current = false; if (active.current) setSending(false); }
  };
  const copy = askAiCopy(language);
  return <div className="flex h-full min-h-0 flex-col">
    {loadError && <div role="alert" className="shrink-0 p-3 text-xs text-af-danger">{loadError} <button type="button" onClick={() => void refresh()} className="underline">Retry loading chat</button></div>}
    {sendError && <div role="alert" className="shrink-0 p-3 text-xs text-af-danger">{sendError} <button type="button" disabled={sending} onClick={() => void send(lastQuestion.current)} className="underline">Retry question</button></div>}
    <ChatThread historyKey={`meeting:${meetingId}`} messages={messages} onSend={send} language={language}
      disabled={disabled || !loaded || sending} compact={compact} onSeek={onSeek}
      {...copy.live} />
  </div>;
}
