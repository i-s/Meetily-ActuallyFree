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
import { AUTO_START_KEY, START_RECORDING_EVENT } from '@/lib/recording-launch';
import { beginAutomatedRecording, endAutomatedRecording, takeAutomatedStart, automatedStartInProgress, releaseAutomatedStart, reservationCurrent, type AutomatedCall } from '@/lib/meeting-automation';
import { discordMayStart, isDiscordProcess, type DiscordCallStatus } from '@/lib/discord-automation';
import { loadLabsPreferences } from '@/lib/labs';
import Analytics from '@/lib/analytics';
import { showRecordingNotification } from '@/lib/recordingNotification';
import { toast } from 'sonner';

interface UseRecordingStartReturn {
  handleRecordingStart: () => Promise<void>;
  isAutoStarting: boolean;
}

type StartSource = 'home_page' | 'sidebar_auto' | 'sidebar_direct';

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
  const prefetchSttModel = useCallback(async (expected?: AutomatedCall): Promise<boolean> => {
    const current = () => !expected || reservationCurrent(expected, 'inflight');
    const ownedInvoke = async <T,>(command: string, args?: Record<string, unknown>): Promise<T> => {
      if (!current()) throw new Error('Automation reservation revoked');
      const result = await invoke<T>(command, args);
      if (!current()) throw new Error('Automation reservation revoked');
      return result;
    };
    runtimeErrorRef.current = null;
    try {
      await ownedInvoke('check_transcription_runtime');
    } catch (error) {
      if (!current()) return false;
      const message = transcriptionRuntimeMessage(error) || 'Could not check the speech runtime. Restart Meetily and try again.';
      runtimeErrorRef.current = message;
      toast.error('Speech recognition unavailable', { description: message });
      return false;
    }
    const provider = (transcriptModelConfig?.provider || 'parakeet').toLowerCase();
    const preferParakeet = provider === 'parakeet' || provider.includes('parakeet');

    try {
      if (preferParakeet) {
        await ownedInvoke('parakeet_init');
        const hasModels = await ownedInvoke<boolean>('parakeet_has_available_models');
        if (!hasModels) return false;
        const alreadyLoaded = await ownedInvoke<boolean>('parakeet_is_model_loaded');
        if (!alreadyLoaded) {
          const models = await ownedInvoke<any[]>('parakeet_get_available_models');
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
            await ownedInvoke('parakeet_load_model', { modelName: ready.name });
          }
        }
        return true;
      }

      // Whisper path (only if user explicitly chose localWhisper)
      await ownedInvoke('whisper_init');
      const hasModels = await ownedInvoke<boolean>('whisper_has_available_models');
      if (!hasModels) return false;
      const alreadyLoaded = await ownedInvoke<boolean>('whisper_is_model_loaded');
      if (!alreadyLoaded) {
        const modelName = transcriptModelConfig?.model || 'large-v3';
        await ownedInvoke('whisper_load_model', { modelName });
      }
      return true;
    } catch (error) {
      if (!current()) return false;
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
  const startRecording = useCallback(async (source: StartSource): Promise<boolean> => {
    // A detected call asked for this start (Labs meeting automation). A start
    // from the record button is the user's own and is never stopped for them.
    const requested = automatedStartInProgress();
    const automated = source === 'home_page' ? null : takeAutomatedStart();
    if (source === 'home_page') endAutomatedRecording();
    const call = automated;
    if (source !== 'home_page' && requested && (!call || !reservationCurrent(call, 'inflight'))) {
      return false; // Expired/replaced automation is never a manual capture.
    }
    // Readying the model can take several seconds (it loads a large model
    // into memory), so say so from the first step.
    setStatus(RecordingStatus.STARTING, 'Preparing transcription model…');
    const sttReady = await prefetchSttModel(call ?? undefined);
    if (call && !reservationCurrent(call, 'inflight')) { releaseAutomatedStart(call); return false; }
    if (!sttReady) {
      if (call) {
        setStatus(RecordingStatus.IDLE);
        toast.error('Automatic recording did not start', { description: 'Speech model is not ready. Check the model and start manually when ready.' });
        releaseAutomatedStart(call);
      } else await reportModelNotReady(source);
      return false;
    }

    try {
      if (call && !reservationCurrent(call, 'inflight')) return false;
      if (call && call.source === 'discord') {
        // Navigation/model loading may outlive the observed join. Revalidate
        // immediately before normal capture; Unknown is never permission to start.
        const status = await invoke<DiscordCallStatus>('get_discord_automation_status');
        if (!reservationCurrent(call, 'inflight')) return false;
        if (!loadLabsPreferences().meetingAutomation || !discordMayStart(status, call.process)) {
          setStatus(RecordingStatus.IDLE);
          toast('Discord automatic start cancelled', { description: 'A connected call is no longer confirmed. You can still start recording manually.' });
          return false;
        }
      }
      const startedAt = Date.now();
      const group = readPendingGroup();
      const title = automaticTitle(new Date(startedAt), group?.name, meetings.map((meeting) => meeting.title));
      setMeetingTitle(title);
      setStatus(RecordingStatus.STARTING, 'Starting audio capture…');

      await recordingService.startRecordingWithDevices(
        selectedDevices?.micDevice || null,
        selectedDevices?.systemDevice || null,
        title
      );

      // Never let a delayed completion label a replacement take as this call.
      // Final native admission/generation validation is owned by integrated #10.
      if (call && !reservationCurrent(call, 'inflight')) return false;
      // RECORDING status itself arrives from Rust via RecordingStateContext.
      beginLiveSession(title, startedAt);
      setIsRecording(true);
      clearTranscripts();
      setIsMeetingActive(true);
      markRecordingStarted(startedAt);
      Analytics.trackButtonClick('start_recording', source);
      if (call && beginAutomatedRecording(call)) {
        toast(`Recording your ${call.app} call`, {
          description: isDiscordProcess(call.process)
            ? 'Experimental Discord evidence. Automatic Stop requires integrated native owner/lease validation; stop manually when needed.'
            : 'Meeting automation started it, and stops and saves it when the call ends.',
          duration: 10000,
          action: { label: "Don't stop it", onClick: () => endAutomatedRecording(call) },
        });
      }
      await showRecordingNotification();
      return true;
    } catch (error) {
      if (call && !reservationCurrent(call, 'inflight')) return false;
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
      if (call) releaseAutomatedStart(call);
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

  // Sent here with the autoStartRecording flag (e.g. "Start" on a group).
  useEffect(() => {
    let shouldAutoStart = false;
    try {
      shouldAutoStart = sessionStorage.getItem(AUTO_START_KEY) === 'true';
    } catch {
      shouldAutoStart = false;
    }
    if (!shouldAutoStart || isRecording || isAutoStarting) return;
    sessionStorage.removeItem(AUTO_START_KEY);
    setIsAutoStarting(true);
    void startRecording('sidebar_auto').finally(() => setIsAutoStarting(false));
  }, [isRecording, isAutoStarting, startRecording]);

  // Tray, detected meetings, and start buttons elsewhere on this page.
  useEffect(() => {
    const handleDirectStart = () => {
      if (isRecording || isAutoStarting) return;
      setIsAutoStarting(true);
      void startRecording('sidebar_direct').finally(() => setIsAutoStarting(false));
    };
    window.addEventListener(START_RECORDING_EVENT, handleDirectStart);
    return () => window.removeEventListener(START_RECORDING_EVENT, handleDirectStart);
  }, [isRecording, isAutoStarting, startRecording]);

  return {
    handleRecordingStart,
    isAutoStarting,
  };
}
