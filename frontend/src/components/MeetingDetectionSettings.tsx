"use client"

import { useEffect, useState } from "react"
import { Switch } from "./ui/switch"
import { Badge } from "./ui/badge"
import { listen } from "@tauri-apps/api/event"
import type { LifecycleSnapshot } from "@/lib/meeting-lifecycle"
import { invoke } from "@tauri-apps/api/core"
import { Radar, Workflow } from "lucide-react"
import { toast } from "sonner"
import { useLabs } from "@/hooks/useLabs"
import { DiscordObservation } from "./DiscordObservation"
import { automationUnavailableMessage, AUTOMATION_CHANGED_EVENT } from "@/lib/meeting-automation"
import type { DiscordCallStatus } from "@/lib/discord-automation"
import { ZoomObservationPanel } from './ZoomObservationPanel'
import { setLabsFeature } from "@/lib/labs-features"

interface MeetingDetectionSettings {
  enabled: boolean;
  interval_secs: number;
  meeting_apps: string[];
  ignored_apps: string[];
  notify: boolean;
  discord_automation: boolean;
  zoom_automation: boolean;
}

/**
 * Meeting Detection settings panel. Watches running processes for meeting apps
 * (Zoom / Teams / Slack / Webex / Discord / …) and prompts to start recording.
 * Fully on-device. Persisted install-locally via Rust.
 */
export function MeetingDetectionSettings() {
  const [md, setMd] = useState<MeetingDetectionSettings | null>(null);
  const [ignoredInput, setIgnoredInput] = useState('');
  const { labs } = useLabs();
  const [automationBusy, setAutomationBusy] = useState(false);
  const [automationUnavailable, setAutomationUnavailable] = useState<string | null>(null);
  useEffect(() => {
    const update = () => setAutomationUnavailable(automationUnavailableMessage());
    update();
    window.addEventListener(AUTOMATION_CHANGED_EVENT, update);
    return () => window.removeEventListener(AUTOMATION_CHANGED_EVENT, update);
  }, []);
  const [discord, setDiscord] = useState<DiscordCallStatus | null>(null);

  useEffect(() => {
    let disposed = false;
    const update = (status: DiscordCallStatus) => { if (!disposed) setDiscord(status); };
    const subscription = listen<DiscordCallStatus>('discord-call-state', (event) => update(event.payload));
    void subscription.then(() => invoke<DiscordCallStatus>('get_discord_automation_status')).then(update).catch(console.error);
    return () => { disposed = true; void subscription.then((fn) => fn()); };
  }, []);

  const [lifecycle, setLifecycle] = useState<LifecycleSnapshot | null>(null);
  useEffect(() => {
    let disposed = false;
    const update = (snapshot: LifecycleSnapshot) => {
      if (!disposed) setLifecycle(previous => !previous || snapshot.revision > previous.revision ? snapshot : previous);
    };
    const unlisten = listen<LifecycleSnapshot>('meeting-lifecycle', event => update(event.payload));
    void unlisten.then(() => invoke<LifecycleSnapshot>('get_meeting_lifecycle_snapshot')).then(update).catch(console.error);
    return () => { disposed = true; void unlisten.then(fn => fn()); };
  }, []);

  useEffect(() => {
    invoke<MeetingDetectionSettings>('get_meeting_detection_settings')
      .then((s) => {
        setMd(s);
        setIgnoredInput((s.ignored_apps || []).join(', '));
      })
      .catch((e) => console.error('Failed to load meeting detection settings:', e));
  }, []);

  const saveMd = async (next: MeetingDetectionSettings) => {
    setMd(next);
    try {
      await invoke('set_meeting_detection_settings', { settings: next });
      // Automation acts on detection's events; without detection it is off too.
      if (!next.enabled && labs.meetingAutomation) await setLabsFeature('meetingAutomation', false);
    } catch (e) {
      console.error('Failed to save meeting detection settings:', e);
    }
  };

  // Labs: turning automation on also turns detection on.
  const setAutomation = async (value: boolean) => {
    setAutomationBusy(true);
    try {
      await setLabsFeature('meetingAutomation', value);
      if (value && md && !md.enabled) setMd({ ...md, enabled: true });
    } catch (error) {
      toast.error('Could not change meeting automation', { description: error instanceof Error ? error.message : String(error) });
    } finally {
      setAutomationBusy(false);
    }
  };

  if (!md) {
    return <div className="max-w-2xl mx-auto p-6 text-af-text-3">Loading…</div>;
  }

  return (
    <div className="space-y-6">
      <div className="rounded-2xl border border-af-border bg-af-panel-2/40 p-5">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-[15px] font-semibold text-af-text mb-2 flex items-center gap-2">
              <Radar className="w-5 h-5 text-af-accent" />
              Meeting Detection
            </h3>
            <p className="text-sm text-af-text-2">
              Local meeting detection. On macOS, Zoom uses read-only Accessibility call controls,
              not process launch, audio activity, mute or silence. Other app offers are separate.
            </p>
          </div>
          <Switch checked={md.enabled} onCheckedChange={(v) => saveMd({ ...md, enabled: v })} />
        </div>

        {md.enabled && (
          <div className="mt-5 space-y-4">
            <div className="flex items-center justify-between gap-4">
              <label className="text-sm text-af-text-2">Check every</label>
              <div className="flex items-center gap-2">
                <input
                  type="number"
                  min={3}
                  max={3600}
                  value={md.interval_secs}
                  onChange={(e) => setMd({ ...md, interval_secs: Number(e.target.value) || 15 })}
                  onBlur={() => saveMd({ ...md, interval_secs: Math.min(3600, Math.max(3, md.interval_secs || 15)) })}
                  className="w-20 rounded-lg border border-af-border px-2 py-1 text-sm focus:border-af-accent/40 focus:outline-none"
                />
                <span className="text-sm text-af-text-3">seconds (legacy offers; macOS Zoom polls every 3 s)</span>
              </div>
            </div>

            <div className="flex items-center justify-between gap-4">
              <label className="text-sm text-af-text-2">Also send a system notification</label>
              <Switch checked={md.notify} onCheckedChange={(v) => saveMd({ ...md, notify: v })} />
            </div>

            <div>
              <label className="text-sm text-af-text-2 block mb-1">
                Ignore these apps (comma-separated)
              </label>
              <input
                type="text"
                value={ignoredInput}
                onChange={(e) => setIgnoredInput(e.target.value)}
                onBlur={() =>
                  saveMd({
                    ...md,
                    ignored_apps: ignoredInput
                      .split(',')
                      .map((s) => s.trim())
                      .filter(Boolean),
                  })
                }
                placeholder="e.g. obs64, teamviewer"
                className="w-full rounded-lg border border-af-border px-3 py-2 text-sm focus:border-af-accent/40 focus:outline-none"
              />
              <p className="text-xs text-af-text-3 mt-1">
                Watched apps: {md.meeting_apps.join(', ')}
              </p>
            </div>
          </div>
        )}
      </div>

      {lifecycle?.sources.map(source => (
        <div key={source.source} className="rounded-xl border border-af-border p-4 text-sm text-af-text-2" role="status">
          <strong>{source.app}: {source.presence}</strong> — {source.reason} · {source.qualified ? 'qualified' : 'observation only, capture blocked'}
          {source.session_id ? ' · call session retained' : ' · no confirmed call'}
          {source.reason === 'accessibility_permission_required' && (
            <p>Allow Meetily in macOS System Settings → Privacy &amp; Security → Accessibility. No recording is inferred from permission alone.</p>
          )}
          <p>Candidate detector, not a physical qualification pass. Identifiable Zoom microphone speech remains source-labeled; #6 acoustic identity is pending.</p>
        </div>
      ))}

      <div className="rounded-xl border border-af-border p-4 text-sm">
        <label className="flex items-center justify-between gap-4">Allow qualified Zoom automation
          <Switch checked={md.zoom_automation ?? false} disabled={!md.enabled} aria-label="Allow qualified Zoom automation"
            onCheckedChange={(value) => void saveMd({ ...md, zoom_automation: value })} />
        </label>
        <p>Separate opt-in; real Zoom signals remain unqualified. This switch never overrides qualification.</p>
      </div>
      <ZoomObservationPanel enabled={md.enabled} />
      <section className="rounded-2xl border border-af-border bg-af-panel-2/40 p-5" aria-label="Discord macOS signal adapter">
        <div className="flex items-center justify-between gap-4">
          <h3 className="text-[15px] font-semibold text-af-text">Discord macOS — qualification pending</h3>
          <Switch checked={md.discord_automation ?? false} disabled={!md.enabled}
            aria-label="Observe Discord macOS call signals"
            onCheckedChange={(value) => void saveMd({ ...md, discord_automation: value })} />
        </div>
        <p className="mt-2 text-sm text-af-text-2">
          Opt-in read-only Accessibility observation, every 3 seconds. Stable desktop client only.
          Grant Meetily Accessibility access in macOS System Settings if you choose to observe.
          No RPC scopes, user token, bot, microphone probing or audio capture is used for detection.
        </p>
        <p className="mt-2 text-sm text-af-text-2" role="status">
          Signal: {discord?.state ?? 'unknown'} · {discord?.reason ?? 'status unavailable'}.
          {' '}Automatic Discord recording is blocked until the signal is qualified on a physical Mac.
        </p>
        <p className="mt-2 text-sm text-af-text-3">
          Guild voice, DM and group DM are each unqualified; this adapter cannot identify the call type.
          Mute, silence, channel/device changes and hidden or unreadable controls do not prove leave.
          Leaving with the client open has no qualified signal. Candidate client-exit tracking has a 45-second reconnect grace;
          automatic Stop also requires integrated native ownership.
          PTB, Canary, browser Discord, Stage and Windows are unsupported here.
          Use manual Start, Pause, Resume and Stop. Manual Stop will not rearm the same detected session.
        </p>
        <DiscordObservation status={discord} automationOn={labs.meetingAutomation} />
      </section>

      {automationUnavailable && <p role="status" className="text-sm text-af-text-2">{automationUnavailable}</p>}



      <div className="flex items-start gap-4 rounded-2xl border border-af-border bg-af-panel-2/40 p-5">
        <span className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-af-accent/[0.12] text-af-accent">
          <Workflow className="h-[18px] w-[18px]" />
        </span>
        <div className="min-w-0 flex-1">
          <h3 className="flex items-center gap-2 text-[15px] font-semibold text-af-text">
            Start and stop recordings automatically
            <Badge variant="accent" size="xs">Labs</Badge>
          </h3>
          <p className="mt-0.5 text-[13px] leading-relaxed text-af-text-3">
            Zoom remains observation-only until a real client class passes physical lifecycle qualification.
            This switch cannot override that native gate, including previously saved settings.
            Unknown never starts or stops a call. A future qualified adapter uses generation-guarded Stop;
            recordings you start yourself remain under manual control. Speaker identity is a separate #6 gate.
          </p>
        </div>
        <Switch
          checked={labs.meetingAutomation}
          disabled={automationBusy}
          onCheckedChange={(value) => void setAutomation(value)}
          aria-label="Start and stop recordings automatically"
          className="mt-1 shrink-0"
        />
      </div>
    </div>
  );
}
