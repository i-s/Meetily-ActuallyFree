'use client';

/**
 * The right-hand panel while recording: who is talking (and naming them),
 * notes typed during the call, and Ask AI about the call so far.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';
import { NotebookPen, Sparkles, UserCheck, Users } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Hint } from '@/components/ui/tooltip';
import { MeetingNotesEditor } from '@/components/meeting/MeetingNotesEditor';
import { LiveCodexChat } from '@/components/recording/LiveCodexChat';
import { useUserName } from '@/hooks/useUserName';
import { displaySpeaker, isUserSpeaker, speakerDot, speakerKey } from '@/utils/speakerUtils';
import type { QuestionLine } from '@/lib/live-codex';
import { readLiveNotes } from '@/lib/live-session';
import { currentRecordingNotesTarget } from '@/lib/notes-store';
import type { DetectedSpeaker } from '@/types';

export type LivePanelTab = 'speakers' | 'notes' | 'ask';


function SpeakersTab({
  speakers,
  onIdentify,
  onMarkMe,
}: {
  speakers: DetectedSpeaker[];
  onIdentify: (speaker: string) => void;
  onMarkMe: (speaker: string) => void;
}) {
  const userName = useUserName();
  const total = speakers.reduce((sum, speaker) => sum + speaker.segmentCount, 0);
  // One row per person: labels that all mean the user, or differ only in case, share a row.
  const sorted = useMemo(() => {
    const byPerson = new Map<string, DetectedSpeaker>();
    for (const speaker of [...speakers].sort((a, b) => b.segmentCount - a.segmentCount)) {
      const key = speaker.isUser ? speakerKey('You') : speakerKey(speaker.name);
      const kept = byPerson.get(key);
      byPerson.set(
        key,
        kept
          ? { ...kept, segmentCount: kept.segmentCount + speaker.segmentCount, lastSpokeAt: Math.max(kept.lastSpokeAt ?? 0, speaker.lastSpokeAt ?? 0) || undefined }
          : speaker,
      );
    }
    return [...byPerson.values()];
  }, [speakers]);

  if (speakers.length === 0) {
    return (
      <div className="flex flex-col items-center px-6 py-14 text-center">
        <span className="mb-3 flex h-10 w-10 items-center justify-center rounded-xl border border-af-border bg-af-panel-2 text-af-text-3">
          <Users className="h-4 w-4" />
        </span>
        <p className="text-[13px] font-medium text-af-text">No voices yet</p>
        <p className="mt-1 max-w-[15rem] text-xs leading-relaxed text-af-text-3">Each person shows up here as they start talking.</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <ul className="min-h-0 flex-1 space-y-1 overflow-y-auto p-3">
        {sorted.map((speaker) => {
          const you = speaker.isUser || isUserSpeaker(speaker.name);
          const share = total > 0 ? Math.round((speaker.segmentCount / total) * 100) : 0;
          return (
            <li key={speaker.id} className="group/speaker relative">
              <button
                type="button"
                onClick={() => onIdentify(speaker.name)}
                className="flex w-full items-center gap-3 rounded-xl px-2.5 py-2 text-left transition-colors hover:bg-af-hover"
              >
                <span className={cn('h-2.5 w-2.5 shrink-0 rounded-full', speakerDot(speaker.name, speaker.colorIndex))} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[13px] font-medium text-af-text">{displaySpeaker(speaker.name, userName)}</span>
                  <span className="mt-1 flex items-center gap-2">
                    <span className="h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-af-border">
                      <span
                        className="block h-full rounded-full bg-af-text-4 transition-[width] duration-500 ease-af"
                        style={{ width: `${Math.max(4, share)}%` }}
                      />
                    </span>
                    <span className="shrink-0 text-[11px] tabular-nums text-af-text-4">{share}%</span>
                  </span>
                </span>
              </button>
              {!you && (
                <Hint label="This is me">
                  <button
                    type="button"
                    onClick={() => onMarkMe(speaker.name)}
                    aria-label={`${speaker.name} is me`}
                    className="absolute right-2 top-1/2 flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-lg bg-af-panel text-af-text-3 opacity-0 shadow-sm ring-1 ring-af-border transition-[opacity,color] hover:text-af-accent focus-visible:opacity-100 group-hover/speaker:opacity-100"
                  >
                    <UserCheck className="h-3.5 w-3.5" />
                  </button>
                </Hint>
              )}
            </li>
          );
        })}
      </ul>
      <p className="shrink-0 border-t border-af-border px-4 py-3 text-[11px] leading-relaxed text-af-text-4">
        Click a speaker to name them from your contacts or add someone new. Names are saved as contacts when the recording ends.
      </p>
    </div>
  );
}

function NotesTab() {
  const [legacy] = useState(() => readLiveNotes());
  const [target, setTarget] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    setError(null);
    void currentRecordingNotesTarget().then(setTarget).catch(e => setError(String(e)));
  }, []);
  useEffect(load, [load]);
  return <div className="h-full overflow-y-auto px-5 py-4 pl-11">
    {error && <div role="alert">{error} <button type="button" onClick={load}>Retry</button></div>}
    {legacy?.markdown && <details className="mb-4 text-xs">
      <summary>Unassociated Notes from an older version — copy to the correct meeting</summary>
      <p>These were stored without a session identity. They have not been attached or erased.</p>
      <textarea aria-label="Unassociated legacy Notes" readOnly value={legacy.markdown} className="mt-2 w-full" />
    </details>}
    {target ? <MeetingNotesEditor key={target} target={target} /> : !error && <p>Loading Notes…</p>}
  </div>;
}

export function LivePanel({
  tab,
  onTabChange,
  speakers,
  lines,
  sessionKey,
  onIdentify,
  onMarkMe,
  onJumpTo,
}: {
  tab: LivePanelTab;
  onTabChange: (tab: LivePanelTab) => void;
  speakers: DetectedSpeaker[];
  lines: QuestionLine[];
  /** Changes per recording, so each call gets its own Ask AI thread. */
  sessionKey: string;
  onIdentify: (speaker: string) => void;
  onMarkMe: (speaker: string) => void;
  /** A cited moment was clicked. */
  onJumpTo: (lineId: string) => void;
}) {
  return (
    <Tabs value={tab} onValueChange={(value) => onTabChange(value as LivePanelTab)} className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 border-b border-af-border px-3 py-2.5">
        <TabsList className="grid w-full grid-cols-3">
          <TabsTrigger value="speakers">
            <Users />
            Speakers
            {speakers.length > 0 && <span className="text-[11px] tabular-nums text-af-text-4">{speakers.length}</span>}
          </TabsTrigger>
          <TabsTrigger value="notes">
            <NotebookPen />
            Notes
          </TabsTrigger>
          <TabsTrigger value="ask">
            <Sparkles />
            Ask AI
          </TabsTrigger>
        </TabsList>
      </div>
      {/* Notes stay mounted so switching tabs never drops a keystroke. */}
      <TabsContent value="speakers" className="mt-0 min-h-0 flex-1 data-[state=inactive]:hidden">
        <SpeakersTab speakers={speakers} onIdentify={onIdentify} onMarkMe={onMarkMe} />
      </TabsContent>
      <TabsContent value="notes" forceMount className="mt-0 min-h-0 flex-1 data-[state=inactive]:hidden">
        <NotesTab key={sessionKey} />
      </TabsContent>
      <TabsContent value="ask" forceMount className="mt-0 min-h-0 flex-1 data-[state=inactive]:hidden">
        <LiveCodexChat key={sessionKey} lines={lines} onJumpTo={onJumpTo} />
      </TabsContent>
    </Tabs>
  );
}

