"use client";

/** Native owns the durable job; the view only starts, observes or defers it. */

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { Button } from '@/components/ui/button';
import { PostCallHandoffCard } from '@/components/PostCallHandoffCard';
import type { RawModelInfo } from '@/hooks/useTranscriptionModels';
import { isVisibleParakeetModel } from '@/lib/parakeet';

interface RetranscriptionProgress {
  meeting_id: string;
  progress_percentage: number;
  message: string;
}

interface ModelChoice {
  provider: 'whisper' | 'parakeet';
  name: string;
}

interface PostCallTranscriptConfig {
  provider: 'live' | 'whisper' | 'parakeet';
  model: string;
}

async function resolveEnhancementModel(
  configuredProvider?: string,
  configuredModel?: string,
): Promise<ModelChoice> {
  const [whisperModels, parakeetModels] = await Promise.all([
    invoke<RawModelInfo[]>('whisper_get_available_models').catch(() => []),
    invoke<RawModelInfo[]>('parakeet_get_available_models').catch(() => []),
  ]);
  const available: ModelChoice[] = [
    ...parakeetModels
      .filter((model) => model.status === 'Available' && isVisibleParakeetModel(model.name))
      .map((model) => ({ provider: 'parakeet' as const, name: model.name })),
    ...whisperModels
      .filter((model) => model.status === 'Available')
      .map((model) => ({ provider: 'whisper' as const, name: model.name })),
  ];
  const normalizedProvider = configuredProvider === 'localWhisper'
    ? 'whisper'
    : configuredProvider;
  const configured = available.find(
    (model) => model.provider === normalizedProvider && model.name === configuredModel,
  );
  if (configured) return configured;
  if (normalizedProvider === 'whisper' || normalizedProvider === 'parakeet') {
    const sameProvider = available.find((model) => model.provider === normalizedProvider);
    if (sameProvider) return sameProvider;
    throw new Error(`No downloaded ${normalizedProvider} model is available for enhancement.`);
  }
  const localDefault = available.find((model) => model.provider === 'parakeet')
    ?? available.find((model) => model.provider === 'whisper')
    ?? available[0];
  if (localDefault) return localDefault;
  throw new Error('No downloaded transcription model is available for post-call enhancement.');
}

interface PostCallProcessingProps {
  enabled: boolean; meetingId: string; meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>; onComplete: () => Promise<void> | void;
}

export function PostCallProcessingDialog(props: PostCallProcessingProps) {
  return <PostCallProcessingHandoff key={props.meetingId} {...props} />;
}

function PostCallProcessingHandoff({
  enabled, meetingId, meetingFolderPath, onRefetchTranscripts, onComplete,
}: PostCallProcessingProps) {
  const { selectedLanguage, transcriptModelConfig } = useConfig();
  const [working, setWorking] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const job = useRef<string | null>(null);
  const starting = useRef(false);
  const [progress, setProgress] = useState(0);
  const [completionPending, setCompletionPending] = useState(false);
  const completing = useRef(false);
  const completionFailed = useRef(false);
  const handoffKey = `post-call-handoff:${meetingId}`;
  const complete = useCallback(async (deferred = false) => {
    if (completing.current) return;
    completing.current = true;
    try {
      if (!deferred) await onRefetchTranscripts?.();
      await onComplete();
      // Persist only a successfully consumed handoff, never an inference start.
      window.localStorage.setItem(handoffKey, deferred ? 'deferred' : 'ready');
      completionFailed.current = false;
      setCompletionPending(false); setDone(true); setWorking(false);
      if (!deferred) toast.success('Refined version saved separately', { description: 'Review it in Transcript versions. Live text and corrections stay selected.' });
    } catch (cause) {
      completionFailed.current = true;
      setCompletionPending(true); setWorking(false); setError(String(cause));
    } finally { completing.current = false; }
  }, [handoffKey, onComplete, onRefetchTranscripts]);

  // Poll persisted job state as well as listening to progress. Native ownership
  // survives navigation/reload; missed completion events cannot lose a result.
  useEffect(() => {
    if (!enabled || done) return;
    let disposed = false;
    let checking = false;
    let consumed: string | null = null;
    try { consumed = window.localStorage.getItem(handoffKey); }
    catch (cause) { setError(String(cause)); }
    if (consumed === 'ready' || consumed === 'deferred') {
      void Promise.resolve().then(onComplete).then(() => { if (!disposed) setDone(true); }).catch((cause) => { if (!disposed) { setError(String(cause)); setCompletionPending(true); completionFailed.current = true; } });
      return () => { disposed = true; };
    }
    const check = async () => {
      if (checking || starting.current) return;
      const observedJob = job.current;
      checking = true;
      try {
        const state = await invoke<{ versions: Array<{ id: string; kind: string; status: string; error?: string }> }>('list_transcript_versions', { meetingId });
        if (disposed || starting.current || observedJob !== job.current) return;
        const active = state.versions.find((v) => v.status === 'running');
        if (active) { job.current = active.id; setWorking(true); }
        else {
          const finished = job.current ? state.versions.find((v) => v.id === job.current)
            : [...state.versions].reverse().find((v) => v.kind === 'refined');
          if (finished?.status === 'ready') {
            job.current = finished.id;
            if (!completionFailed.current) await complete();
          } else if (finished) {
            setWorking(false); setError(finished.error ?? `Refinement ${finished.status}. Previous text is unchanged.`);
          }
        }
      } catch (cause) { if (!disposed) setError(String(cause)); }
      finally { checking = false; }
    };
    void check();
    const timer = setInterval(() => void check(), 2000);
    const listener = listen<RetranscriptionProgress>('retranscription-progress', ({ payload }) => {
      if (!disposed && payload.meeting_id === meetingId) { setMessage(payload.message); setProgress(payload.progress_percentage); }
    });
    return () => { disposed = true; clearInterval(timer); void listener.then((stop) => stop()).catch(() => undefined); };
  }, [enabled, meetingId, done, handoffKey, onComplete, complete]);

  const start = async () => {
    if (starting.current || completing.current || completionPending) return;
    starting.current = true;
    setError(null); setWorking(true); setProgress(0);
    try {
      if (!meetingFolderPath) throw new Error('Saved audio is unavailable. Existing text remains readable.');
      const config = await invoke<PostCallTranscriptConfig>('api_get_post_call_transcript_config');
      const live = config.provider === 'live';
      const model = await resolveEnhancementModel(live ? transcriptModelConfig?.provider : config.provider, live ? transcriptModelConfig?.model : config.model);
      const started = await invoke<{ version_id: string }>('start_retranscription_command', {
        meetingId, meetingFolderPath, model: model.name, provider: model.provider,
        language: model.provider === 'parakeet' || selectedLanguage === 'auto' ? null : selectedLanguage || null,
      });
      job.current = started.version_id;
    } catch (cause) { setError(String(cause)); setWorking(false); }
    finally { starting.current = false; }
  };
  if (!enabled || done) return null;
  return <PostCallHandoffCard centered busy={working} title="Local post-meeting refinement"
    detail="ASR and automatic speaker refinement save a separate version. Live text and manual corrections are never automatically replaced. Shared-room speaker identification is outside MVP. This step does not start a summary or send text to a provider.">
    <div className="space-y-3">
      {error && <p role="alert" className="text-sm text-af-danger">{error}</p>}
      {completionPending ? <Button onClick={() => void complete()}>Retry completion refresh</Button> : working ? <>
        <p role="status" className="text-sm">{message || 'Processing saved audio locally…'} ({progress}%)</p>
        <Button variant="ghost" onClick={() => { void invoke('cancel_retranscription_command', { meetingId, versionId: job.current }).catch((cause) => setError(String(cause))); }}>Cancel refinement</Button>
        <p className="text-xs text-af-text-2">Cancellation waits for the current model inference. You can leave this page; the result and status remain saved.</p>
      </> : <Button onClick={() => void start()} disabled={!meetingFolderPath}>{error ? 'Retry local refinement' : 'Create refined version'}</Button>}
      <Button variant="ghost" onClick={() => void complete(true)}>Keep live transcript / defer</Button>
    </div>
  </PostCallHandoffCard>;
}
