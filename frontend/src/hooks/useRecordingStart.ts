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
import { START_RECORDING_EVENT, readRecordingLaunchIntent, takeRecordingLaunchIntent, requestRecordingStop, type RecordingLaunchIntent } from '@/lib/recording-launch';
import { automatedStartState, automatedRecording, beginAutomatedRecording, endAutomatedRecording, takeAutomatedStartResult, freezeAutomatedStart, releaseAutomatedStart, reservationCurrent, type AutomatedCall } from '@/lib/meeting-automation';
import { discordMayStart, type DiscordCallStatus } from '@/lib/discord-automation';
import { loadLabsPreferences } from '@/lib/labs';
import Analytics from '@/lib/analytics';
import { showRecordingNotification } from '@/lib/recordingNotification';
import { toast } from 'sonner';

import { recoverAutomatedStart, settleAutomatedStart } from '@/lib/recording-start-recovery';
import { isRecordingLease, type RecordingLease, type StartRequest } from '@/lib/recording-start-authority';
import type { LifecycleSnapshot } from '@/lib/meeting-lifecycle';

async function callPresence(call: AutomatedCall): Promise<'active' | 'ended' | 'unknown'> {
  if (call.source === 'discord') {
    const status = await invoke<DiscordCallStatus>('get_discord_automation_status');
    if (discordMayStart(status, call.process)) return 'active';
    return status.enabled && status.qualified && ((status.transition === 'end' && status.session_id === call.sessionId)
      || (status.state === 'inactive' && !status.session_id)) ? 'ended' : 'unknown';
  }
  if (call.source !== 'zoom') return 'unknown';
  const snapshot = await invoke<LifecycleSnapshot>('get_meeting_lifecycle_snapshot');
  const source = snapshot.sources.find(s => s.source === 'zoom');
  if (!snapshot.enabled || snapshot.automation_enabled !== true || !source?.qualified) return 'unknown';
  if (source.presence === 'active' && source.session_id === call.sessionId) return 'active';
  return (source.presence === 'inactive' && source.session_id === null)
    || (source.session_id !== null && source.session_id !== call.sessionId) ? 'ended' : 'unknown';
}
function sameLease(a: RecordingLease, b: RecordingLease | null | undefined): boolean {
  return !!b && a.generation === b.generation && JSON.stringify(a.owner) === JSON.stringify(b.owner);
}

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
  const startingRef = useRef(false);
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    void recoverAutomatedStart().catch(error => console.error('Native recording start remains unresolved:', error));
    return () => { mountedRef.current = false; };
  }, []);

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

  /** One normal recorder path for explicit manual intent and both adapters. */
  const startRecording = useCallback(async (source: StartSource, intent: RecordingLaunchIntent = { kind: 'manual' }): Promise<boolean> => {
    if (startingRef.current || isRecording || intent.kind === 'absent' || intent.kind === 'rejected') return false;
    const taken = intent.kind === 'automated' ? takeAutomatedStartResult(intent.call) : null;
    if (taken && taken.kind !== 'accepted') return false;
    const call = taken?.kind === 'accepted' ? taken.call : null;
    if (!call) {
      const previous = automatedStartState();
      if (previous.kind === 'pending' || previous.kind === 'preparing' || previous.kind === 'native_unresolved') releaseAutomatedStart(previous.call);
    }
    let expected: StartRequest | null = null;
    startingRef.current = true;
    const current = () => mountedRef.current && (!call || (loadLabsPreferences().meetingAutomation && reservationCurrent(call, 'inflight')));
    try {
      if (!current()) return false;
      setStatus(RecordingStatus.STARTING, 'Preparing transcription model…');
      const sttReady = await prefetchSttModel(call ?? undefined);
      if (!current()) return false;
      if (!sttReady) {
        if (call) {
          setStatus(RecordingStatus.IDLE);
          toast.error('Automatic recording did not start', { description: 'Speech model is not ready. Check the model and start manually when ready.' });
        } else await reportModelNotReady(source);
        return false;
      }
      const startedAt = Date.now();
      const group = readPendingGroup();
      const title = automaticTitle(new Date(startedAt), group?.name, meetings.map(meeting => meeting.title));
      let lease: RecordingLease | void;
      if (call) {
        const capability = await recordingService.getRecordingStartCapability();
        if (!current()) return false;
        const presence = await callPresence(call);
        if (!current()) return false;
        if (presence !== 'active') {
          setStatus(RecordingStatus.IDLE);
          toast(`${call.app} automatic start cancelled`, { description: 'A qualified current call is no longer confirmed. Manual controls remain available.' });
          return false;
        }
        expected = freezeAutomatedStart(call, capability);
        if (!expected) return false;
        // No await between persisting this EXACT ticket and dispatch. Reload
        // recovery must settle it even if invoke's response is lost.
        lease = await recordingService.startRecordingWithDevices(
          selectedDevices?.micDevice || null, selectedDevices?.systemDevice || null, title, call.sessionId, expected);
      } else {
        lease = await recordingService.startRecordingWithDevices(
          selectedDevices?.micDevice || null, selectedDevices?.systemDevice || null, title);
      }
      const native = await recordingService.getRecordingState();
      if (!mountedRef.current || !isRecordingLease(lease) || !sameLease(lease, native.recording_lease)
        || !native.is_recording || native.is_stopping) return false;
      if (call) {
        if (!current()) return false;
        const active = automatedRecording();
        if (!(active && reservationCurrent(call, 'recording') && sameLease(lease, active.recordingLease))
          && !beginAutomatedRecording(call, lease)) return false;
      }
      setMeetingTitle(title);
      beginLiveSession(title, startedAt);
      setIsRecording(true);
      clearTranscripts();
      setIsMeetingActive(true);
      markRecordingStarted(startedAt);
      Analytics.trackButtonClick('start_recording', source);
      if (call && reservationCurrent(call, 'recording')) {
        const capturedLease = lease;
        toast(`Recording your ${call.app} call`, {
          description: 'Meeting automation started it, and stops and saves it when the qualified call ends.',
          duration: 10000,
          action: { label: "Don't stop it", onClick: () => {
            void invoke<boolean>('release_automated_recording', { expected: capturedLease }).then(released => {
              if (released) endAutomatedRecording(call);
            }).catch(error => toast.error('Could not release automatic ownership', { description: String(error) }));
          } },
        });
        if (await callPresence(call) === 'ended' && loadLabsPreferences().meetingAutomation
          && reservationCurrent(call, 'recording')) requestRecordingStop(() => {}, capturedLease);
      }
      await showRecordingNotification();
      return true;
    } catch (error) {
      // A transport error is not a native rejection. Settle only this request;
      // never fetch B's tuple or clear its state from A's late catch/finally.
      if (call && expected) {
        try { await settleAutomatedStart(call, expected); }
        catch (settlementError) { console.error('Native start remains unresolved:', settlementError); }
        const native = await recordingService.getRecordingState().catch(() => null);
        if (!native || native.is_recording || native.is_stopping) return false;
        // Valid idle settlement clears A's stage. Report its error only if no
        // replacement B has acquired while the barrier/query was awaited.
        if (automatedStartState().kind !== 'absent') return false;
      }
      if (!mountedRef.current || (call && !expected && !reservationCurrent(call, 'inflight'))) return false;
      console.error('Failed to start recording:', error);
      if (!call) setIsRecording(false);
      const runtimeMessage = transcriptionRuntimeMessage(error);
      if (runtimeMessage) {
        setStatus(RecordingStatus.ERROR, runtimeMessage);
        toast.error('Speech recognition unavailable', { description: runtimeMessage });
        return false;
      }
      setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Failed to start recording');
      const message = describeStartError(error);
      toast.error(message.title, { description: message.message, duration: 8000 });
      Analytics.trackButtonClick('start_recording_error', source);
      return false;
    } finally {
      if (call) releaseAutomatedStart(call);
      startingRef.current = false;
    }
  }, [isRecording, prefetchSttModel, reportModelNotReady, meetings, setMeetingTitle, setStatus,
    selectedDevices, setIsRecording, clearTranscripts, setIsMeetingActive, markRecordingStarted]);

  // Record button on this page.
  const handleRecordingStart = useCallback(async () => {
    await startRecording('home_page');
  }, [startRecording]);

  // Sent here with the autoStartRecording flag (e.g. "Start" on a group).
  useEffect(() => {
    if (isRecording || isAutoStarting) return;
    const intent = takeRecordingLaunchIntent();
    if (intent.kind === 'absent' || intent.kind === 'rejected') return;
    setIsAutoStarting(true);
    void startRecording('sidebar_auto', intent).finally(() => setIsAutoStarting(false));
  }, [isRecording, isAutoStarting, startRecording]);

  // Tray, detected meetings, and start buttons elsewhere on this page.
  useEffect(() => {
    const handleDirectStart = (event: Event) => {
      if (isRecording || isAutoStarting) return;
      setIsAutoStarting(true);
      void startRecording('sidebar_direct', readRecordingLaunchIntent((event as CustomEvent).detail)).finally(() => setIsAutoStarting(false));
    };
    window.addEventListener(START_RECORDING_EVENT, handleDirectStart);
    return () => window.removeEventListener(START_RECORDING_EVENT, handleDirectStart);
  }, [isRecording, isAutoStarting, startRecording]);

  return {
    handleRecordingStart,
    isAutoStarting,
  };
}
