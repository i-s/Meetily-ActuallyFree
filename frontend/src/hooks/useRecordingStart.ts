import { useState, useEffect, useCallback, useRef } from 'react';
import { transcriptionRuntimeMessage } from '@/lib/transcription-runtime';
import { invoke } from '@tauri-apps/api/core';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';
import { recordingService } from '@/services/recordingService';
import { readPendingGroup } from '@/lib/groups';
import { automaticTitle, beginLiveSession } from '@/lib/live-session';
import { AUTO_START_KEY, START_RECORDING_EVENT, requestRecordingStop } from '@/lib/recording-launch';
import { beginAutomatedRecording, endAutomatedRecording, takeAutomatedStart, releaseAutomatedStart, reservationCurrent, type AutomatedCall } from '@/lib/meeting-automation';
import { lifecycleCallCanStart, lifecycleCallEnded } from '@/lib/meeting-lifecycle';
import { loadLabsPreferences } from '@/lib/labs';
import Analytics from '@/lib/analytics';
import { showRecordingNotification } from '@/lib/recordingNotification';
import { toast } from 'sonner';

interface UseRecordingStartReturn {
  handleRecordingStart: () => Promise<void>;
  isAutoStarting: boolean;
}

type StartSource = 'home_page' | 'sidebar_auto' | 'sidebar_direct';

// Consumer-only validation until #11's shared read/takeRecordingLaunchIntent
// lands. Missing/corrupt handoff is no action, never evidence of manual intent.
function startIntent(payload: unknown): { kind: 'manual' } | { kind: 'automated'; call: AutomatedCall } | null {
  try {
    const value = typeof payload === 'string' ? JSON.parse(payload) : payload;
    if (!value || typeof value !== 'object') return null;
    if (value.kind === 'manual') return { kind: 'manual' };
    const call = value.call;
    if (value.kind !== 'automated' || !call || typeof call !== 'object') return null;
    if (![call.app, call.process, call.source, call.sessionId, call.token].every(part => typeof part === 'string' && part.trim())) return null;
    return { kind: 'automated', call };
  } catch { return null; }
}

/** A readable reason for a failed start, from the backend's error text. */
export function describeStartError(error: unknown): { title: string; message: string } {
  const text = (error instanceof Error ? error.message : String(error)).toLowerCase();
  if (text.includes('one app at a time')) {
    return {
      title: 'Choose one app on macOS',
      message: 'Remove extra selected apps in Recording settings before starting. No broader audio source was substituted.',
    };
  }
  if (text.includes('target application')) {
    return {
      title: 'Chosen apps are not available',
      message: 'Select an app and open every chosen app before recording, or remove unavailable apps. No broader audio source was substituted.',
    };
  }
  if (text.includes('microphone') || text.includes('mic') || text.includes('input')) {
    return {
      title: 'Microphone not available',
      message: 'Check that the microphone is connected, that Meetily may use it, and that no other app has taken it.',
    };
  }
  if (text.includes('system audio') || text.includes('speaker') || text.includes('output')) {
    return {
      title: 'System audio not available',
      message:
        'On macOS, allow Meetily Audio Capture in Privacy & Security, play some audio, and try again. Elsewhere, check the selected playback device.',
    };
  }
  if (text.includes('permission')) {
    return {
      title: 'Permission needed',
      message: 'Allow microphone access (and Audio Capture on macOS) in your system settings, then restart Meetily.',
    };
  }
  return { title: 'Could not start recording', message: 'Check your audio devices in the recording card and try again.' };
}

/**
 * Starts recordings, from the record button, from the sidebar or a group
 * (auto-start after navigating here), and from the tray or a detected
 * meeting (the `start-recording-from-sidebar` event).
 *
 * The meeting gets its automatic title here: the group's name and date when
 * one is chosen, otherwise the day and time slot.
 */
export function useRecordingStart(
  isRecording: boolean,
  setIsRecording: (value: boolean) => void,
  showModal?: (name: 'modelSelector', message?: string) => void
): UseRecordingStartReturn {
  const [isAutoStarting, setIsAutoStarting] = useState(false);
  const runtimeErrorRef = useRef<string | null>(null);

  const { clearTranscripts, setMeetingTitle } = useTranscripts();
  const { setIsMeetingActive, meetings } = useSidebar();
  const { selectedDevices, transcriptModelConfig } = useConfig();
  const { setStatus } = useRecordingState();

  const markRecordingStarted = useCallback((requestedAt: number) => {
    try {
      sessionStorage.setItem('recording_started_at', String(Date.now()));
      sessionStorage.setItem('recording_start_fallback_at', String(requestedAt));
    } catch {
      /* ignore */
    }
  }, []);

  // Prefer Parakeet for live (fast). Whisper is for post-call enhance/retranscribe.
  // Prefetch the configured provider so Start Recording feels snappy after idle unload.
  const prefetchSttModel = useCallback(async (): Promise<boolean> => {
    runtimeErrorRef.current = null;
    try {
      await invoke('check_transcription_runtime');
    } catch (error) {
      const message = transcriptionRuntimeMessage(error) || 'Could not check the speech runtime. Restart Meetily and try again.';
      runtimeErrorRef.current = message;
      toast.error('Speech recognition unavailable', { description: message });
      return false;
    }
    const provider = (transcriptModelConfig?.provider || 'parakeet').toLowerCase();
    const preferParakeet = provider === 'parakeet' || provider.includes('parakeet');

    try {
      if (preferParakeet) {
        await invoke('parakeet_init');
        const hasModels = await invoke<boolean>('parakeet_has_available_models');
        if (!hasModels) return false;
        const alreadyLoaded = await invoke<boolean>('parakeet_is_model_loaded');
        if (!alreadyLoaded) {
          const models = await invoke<any[]>('parakeet_get_available_models');
          const configured = transcriptModelConfig?.model;
          const ready =
            (configured && models.find((m: any) => m?.name === configured)) ||
            models.find(
              (m: any) =>
                m?.status === 'Available' ||
                (typeof m?.status === 'object' && m.status && 'Available' in m.status),
            ) ||
            models[0];
          if (ready?.name) {
            await invoke('parakeet_load_model', { modelName: ready.name });
          }
        }
        return true;
      }

      // Whisper path (only if user explicitly chose localWhisper)
      await invoke('whisper_init');
      const hasModels = await invoke<boolean>('whisper_has_available_models');
      if (!hasModels) return false;
      const alreadyLoaded = await invoke<boolean>('whisper_is_model_loaded');
      if (!alreadyLoaded) {
        const modelName = transcriptModelConfig?.model || 'large-v3';
        await invoke('whisper_load_model', { modelName });
      }
      return true;
    } catch (error) {
      console.error('Failed to prefetch STT model:', error);
      return false;
    }
  }, [transcriptModelConfig]);

  // Check if any model is currently downloading
  const checkIfModelDownloading = useCallback(async (): Promise<boolean> => {
    try {
      const models = await invoke<any[]>('parakeet_get_available_models');
      return models.some(m =>
        m.status && (
          typeof m.status === 'object'
            ? 'Downloading' in m.status
            : m.status === 'Downloading'
        )
      );
    } catch (error) {
      console.error('Failed to check model download status:', error);
      return false;
    }
  }, []);

  /** Says why recording can't start yet, and offers the model setup when that's the reason. */
  const reportModelNotReady = useCallback(async (source: StartSource) => {
    if (runtimeErrorRef.current) {
      setStatus(RecordingStatus.ERROR, runtimeErrorRef.current);
      return;
    }
    if (await checkIfModelDownloading()) {
      toast.info('Model download in progress', {
        description: 'Recording can start once the transcription model finishes downloading.',
        duration: 5000,
      });
      Analytics.trackButtonClick('start_recording_blocked_downloading', source);
    } else {
      toast.error('Transcription model not ready', {
        description: 'Download a transcription model (Parakeet is best for live) before recording.',
        duration: 5000,
      });
      showModal?.('modelSelector', 'Transcription model setup required');
      Analytics.trackButtonClick('start_recording_blocked_missing', source);
    }
    setStatus(RecordingStatus.IDLE);
  }, [checkIfModelDownloading, setStatus, showModal]);

  /** The one start sequence. Returns false when recording did not start. */
  const startRecording = useCallback(async (source: StartSource, expected?: AutomatedCall): Promise<boolean> => {
    // Only an explicit automated intent may consume a reservation. Validate
    // its token BEFORE transfer so a late A event cannot steal pending B.
    if (expected && !reservationCurrent(expected, 'pending')) return false;
    const automated = expected ? takeAutomatedStart() : null;
    if (expected && (!automated || automated.token !== expected.token
      || automated.source !== expected.source || automated.sessionId !== expected.sessionId)) return false;
    const call = automated;
    // Readying the model can take several seconds (it loads a large model
    // into memory), so say so from the first step.
    setStatus(RecordingStatus.STARTING, 'Preparing transcription model…');
    const sttReady = await prefetchSttModel();
    if (!sttReady) {
      if (automated) releaseAutomatedStart(automated);
      await reportModelNotReady(source);
      return false;
    }

    try {
      if (call && (!loadLabsPreferences().meetingAutomation || !reservationCurrent(call, 'inflight') || !lifecycleCallCanStart(call))) {
        setStatus(RecordingStatus.IDLE);
        return false;
      }
      const startedAt = Date.now();
      const group = readPendingGroup();
      const title = automaticTitle(new Date(startedAt), group?.name, meetings.map((meeting) => meeting.title));
      setMeetingTitle(title);
      setStatus(RecordingStatus.STARTING, 'Starting audio capture…');

      const lease = await recordingService.startRecordingWithDevices(
        selectedDevices?.micDevice || null,
        selectedDevices?.systemDevice || null,
        title,
        call?.sessionId
      );

      const native = await recordingService.getRecordingState();
      // Return values identify THIS start, not whichever take is current after
      // an awaited notification or a WebView reload. Never attach A to B.
      if (!lease || native.recording_lease?.generation !== lease.generation) return false;

      // RECORDING status itself arrives from Rust via RecordingStateContext.
      beginLiveSession(title, startedAt);
      setIsRecording(true);
      clearTranscripts();
      setIsMeetingActive(true);
      markRecordingStarted(startedAt);
      Analytics.trackButtonClick('start_recording', source);
      if (call) {
        call.recordingLease = lease;
        beginAutomatedRecording(call);
        // A leave can race native stream startup. Finish using the normal Stop
        // path, not a parallel backend shutdown that bypasses saving.
        if (call.sessionId && lifecycleCallEnded(call) && lease) {
          requestRecordingStop(() => {}, lease);
          return true;
        }
        toast(`Recording your ${call.app} call`, {
          description: 'Meeting automation started it, and stops and saves it when the call ends.',
          duration: 10000,
          action: { label: "Don't stop it", onClick: () => {
            if (lease) void invoke<boolean>('release_automated_recording', { expected: lease }).then(released => {
              if (released) endAutomatedRecording();
            });
          } },
        });
      }
      await showRecordingNotification();
      return true;
    } catch (error) {
      console.error('Failed to start recording:', error);
      setIsRecording(false);
      const runtimeMessage = transcriptionRuntimeMessage(error);
      if (runtimeMessage) {
        setStatus(RecordingStatus.ERROR, runtimeMessage);
        toast.error('Speech recognition unavailable', { description: runtimeMessage });
        return false;
      }
      setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Failed to start recording');
      const { title, message } = describeStartError(error);
      toast.error(title, { description: message, duration: 8000 });
      Analytics.trackButtonClick('start_recording_error', source);
      return false;
    } finally {
      if (automated) releaseAutomatedStart(automated);
    }
  }, [
    prefetchSttModel,
    reportModelNotReady,
    meetings,
    setMeetingTitle,
    setStatus,
    selectedDevices,
    setIsRecording,
    clearTranscripts,
    setIsMeetingActive,
    markRecordingStarted,
  ]);

  // Record button on this page.
  const handleRecordingStart = useCallback(async () => {
    await startRecording('home_page');
  }, [startRecording]);

  // The route payload carries intent independently of reservation parse success.
  useEffect(() => {
    if (isRecording || isAutoStarting) return;
    let payload: string | null;
    try {
      payload = sessionStorage.getItem(AUTO_START_KEY);
      sessionStorage.removeItem(AUTO_START_KEY);
    } catch { return; }
    const intent = startIntent(payload);
    if (!intent) return;
    setIsAutoStarting(true);
    void startRecording('sidebar_auto', intent.kind === 'automated' ? intent.call : undefined)
      .finally(() => setIsAutoStarting(false));
  }, [isRecording, isAutoStarting, startRecording]);

  useEffect(() => {
    const handleDirectStart = (event: Event) => {
      if (isRecording || isAutoStarting) return;
      const intent = startIntent((event as CustomEvent).detail);
      if (!intent) return;
      setIsAutoStarting(true);
      void startRecording('sidebar_direct', intent.kind === 'automated' ? intent.call : undefined)
        .finally(() => setIsAutoStarting(false));
    };
    window.addEventListener(START_RECORDING_EVENT, handleDirectStart);
    return () => window.removeEventListener(START_RECORDING_EVENT, handleDirectStart);
  }, [isRecording, isAutoStarting, startRecording]);

  return {
    handleRecordingStart,
    isAutoStarting,
  };
}
