import React, { useEffect, useState, useCallback } from 'react';
import { applySystemAudioProbe, SystemAudioProbeResult } from '@/lib/system-audio-permission';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Mic, Volume2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { PermissionRow } from '../shared';
import { useOnboarding } from '@/contexts/OnboardingContext';

export function PermissionsStep() {
  const { setPermissionStatus, setPermissionsSkipped, permissions, completeOnboarding } = useOnboarding();
  const [isPending, setIsPending] = useState(false);

  // Check permissions - only logs current state, doesn't auto-authorize
  // Actual permission checks are done via explicit user actions (clicking Enable)
  const checkPermissions = useCallback(async () => {
    console.log('[PermissionsStep] Current permission states:');
    console.log(`  - Microphone: ${permissions.microphone}`);
    console.log(`  - System Audio: ${permissions.systemAudio}`);
    // Don't auto-set permissions based on device availability
    // Permissions should only be set after explicit user action via Enable button
  }, [permissions.microphone, permissions.systemAudio]);

  // Check permissions on mount
  useEffect(() => {
    checkPermissions();
  }, [checkPermissions]);

  // Request microphone permission
  const handleMicrophoneAction = async () => {
    if (permissions.microphone === 'denied') {
      // Try to open system settings
      try {
        await invoke('open_system_settings');
      } catch {
        toast.info('Allow microphone access', {
          description: 'Open System Settings → Privacy & Security → Microphone and turn on Meetily.',
        });
      }
      return;
    }

    setIsPending(true);
    try {
      console.log('[PermissionsStep] Triggering microphone permission...');
      const granted = await invoke<boolean>('trigger_microphone_permission');
      console.log('[PermissionsStep] Microphone permission result:', granted);

      if (granted) {
        setPermissionStatus('microphone', 'authorized');
      } else {
        // Permission was denied or dialog was dismissed
        setPermissionStatus('microphone', 'denied');
      }
    } catch (err) {
      console.error('[PermissionsStep] Failed to request microphone permission:', err);
      setPermissionStatus('microphone', 'denied');
    } finally {
      setIsPending(false);
    }
  };

  // Request system audio permission
  const handleSystemAudioAction = async () => {
    if (permissions.systemAudio === 'denied') {
      // Try to open system settings
      try {
        await invoke('open_system_settings');
      } catch {
        toast.info('Allow Audio Capture', {
          description: 'Open System Settings → Privacy & Security → Audio Capture and turn on Meetily.',
        });
      }
      return;
    }

    setIsPending(true);
    try {
      console.log('[PermissionsStep] Triggering Audio Capture permission...');
      const result = await invoke<SystemAudioProbeResult>('trigger_system_audio_permission_command');
      const previous = permissions.systemAudio === 'verified' || permissions.systemAudio === 'authorized'
        ? 'verified' : undefined;
      const status = applySystemAudioProbe(result, previous);
      setPermissionStatus('systemAudio', status);
      if (status === 'unknown') {
        toast.info('Computer audio has not been verified', {
          description: 'Play some audio and check again. A quiet check does not mean permission was denied.',
        });
      }
    } catch (err) {
      console.error('[PermissionsStep] Could not verify system audio:', err);
      // An IPC or initialization failure supplies no new authorization evidence.
      if (permissions.systemAudio !== 'verified' && permissions.systemAudio !== 'authorized') {
        setPermissionStatus('systemAudio', 'unknown');
      }
      toast.info('Could not verify computer audio', { description: 'Play some audio and check again.' });
    } finally {
      setIsPending(false);
    }
  };

  const handleFinish = async () => {
    try {
      await completeOnboarding();
      window.location.reload();
    } catch (error) {
      console.error('Failed to complete onboarding:', error);
    }
  };

  const handleSkip = async () => {
    setPermissionsSkipped(true);
    await handleFinish();
  };

  const allPermissionsGranted =
    permissions.microphone === 'authorized' &&
    (permissions.systemAudio === 'verified' || permissions.systemAudio === 'authorized');

  return (
    <OnboardingContainer
      title="Grant Permissions"
      description="Meetily needs access to your microphone and system audio to record meetings"
      step={4}
      hideProgress={true}
      showNavigation={allPermissionsGranted}
      canGoNext={allPermissionsGranted}
    >
      <div className="max-w-lg mx-auto space-y-6">
        {/* Permission Rows */}
        <div className="space-y-4">
          {/* Microphone */}
          <PermissionRow
            icon={<Mic className="w-5 h-5" />}
            title="Microphone"
            description="Required to capture your voice during meetings"
            status={permissions.microphone}
            isPending={isPending}
            onAction={handleMicrophoneAction}
          />

          {/* System Audio */}
          <PermissionRow
            icon={<Volume2 className="w-5 h-5" />}
            title="System Audio"
            description="Play some computer audio, then check Audio Capture"
            status={permissions.systemAudio}
            isPending={isPending}
            onAction={handleSystemAudioAction}
          />
        </div>

        {/* Action Buttons */}
        <div className="flex flex-col gap-3 pt-4">
          <Button onClick={handleFinish} disabled={!allPermissionsGranted} className="w-full h-11">
            Finish Setup
          </Button>

          <button
            onClick={handleSkip}
            className="text-sm text-af-text-3 hover:text-af-text-2 transition-colors"
          >
            I'll do this later
          </button>

          {!allPermissionsGranted && (
            <p className="text-xs text-center text-muted-foreground">
              You can verify computer audio later by playing audio and checking again.
            </p>
          )}
        </div>
      </div>
    </OnboardingContainer>
  );
}
