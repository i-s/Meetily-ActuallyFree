//! Lightweight pre-recording level meters (mic + system/loopback).
//! Used by onboarding audio test and DeviceSelection meters.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use log::{error, info, warn};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

#[derive(Debug, Serialize, Clone)]
pub struct AudioLevelData {
    pub device_name: String,
    pub device_type: String, // "input" | "output"
    pub rms_level: f32,
    pub peak_level: f32,
    pub is_active: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct AudioLevelUpdate {
    pub timestamp: u64,
    pub levels: Vec<AudioLevelData>,
}

static IS_MONITORING: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static SESSION: tokio::sync::Mutex<Option<MonitorSession>> = tokio::sync::Mutex::const_new(None);
struct MonitorSession {
    running: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}
/// Latest levels keyed by device name (shared with cpal callbacks).
type LevelMap = Arc<Mutex<std::collections::HashMap<String, AudioLevelData>>>;

// Awaited acknowledgement means the old thread has dropped its streams, not
// merely that the WebView stopped rendering its meter. Never block Tokio on join.
async fn stop_session(session: &mut Option<MonitorSession>) -> Result<()> {
    if let Some(old) = session.take() {
        old.running.store(false, Ordering::SeqCst);
        tokio::task::spawn_blocking(move || old.thread.join())
            .await.map_err(|e| anyhow!("Level monitor shutdown: {e}"))?
            .map_err(|_| anyhow!("Level monitor thread panicked"))?;
    }
    IS_MONITORING.store(false, Ordering::SeqCst);
    Ok(())
}

/// CPAL endpoint preview is never a selected-app preview, on any platform.
/// Serialize policy read and final stream-open acknowledgement against changes
/// to capture preferences. A queued older request cannot revive cancelled audio.
pub async fn start_monitoring<R: Runtime>(
    app_handle: AppHandle<R>,
    mut device_names: Vec<String>,
    device_type: Option<String>,
) -> Result<()> {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if device_type.as_deref().is_some_and(|kind| kind != "input" && kind != "output") {
        return Err(anyhow!("Unknown level-monitor source"));
    }
    let _preferences = super::recording_preferences::PREFERENCES_WRITE.lock().await;
    let prefs = match super::recording_preferences::load_recording_preferences_locked(&app_handle).await {
        Ok(prefs) => prefs,
        Err(error) => {
            stop_session(&mut *SESSION.lock().await).await?;
            return Err(error);
        }
    };
    let mut session = SESSION.lock().await;
    if generation != GENERATION.load(Ordering::SeqCst) { return Ok(()); }
    stop_session(&mut session).await?;
    let input = device_type.as_deref() != Some("output");
    let output = device_type.as_deref() != Some("input");
    // Explicit single-source record-card previews obey source inclusion. Legacy
    // onboarding dual requests fail closed if either requested source is excluded.
    if (output && (prefs.per_app_recording_enabled || !prefs.sources.system_audio_enabled))
        || (input && !prefs.sources.microphone_enabled) {
        return Ok(());
    }
    if device_names.is_empty() { device_names = default_monitor_devices(device_type.as_deref()); }
    if device_names.is_empty() { return Err(anyhow!("No audio devices available to monitor")); }
    if generation != GENERATION.load(Ordering::SeqCst) { return Ok(()); }
    let levels: LevelMap = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let running = Arc::new(AtomicBool::new(true));
    let thread_running = running.clone();
    let thread_levels = levels.clone();
    let (ready, opened) = tokio::sync::oneshot::channel();
    let handle = thread::Builder::new().name("af-level-monitor".into()).spawn(move || {
        let mut streams = Vec::new();
        for name in &device_names {
            if !thread_running.load(Ordering::SeqCst) || generation != GENERATION.load(Ordering::SeqCst) { break; }
            match open_level_stream(name, thread_levels.clone(), device_type.as_deref()) {
                Ok(stream) => {
                    if generation == GENERATION.load(Ordering::SeqCst) && thread_running.load(Ordering::SeqCst) {
                        if let Err(error) = stream.play() { warn!("Level preview: {error}"); }
                        else { streams.push(stream); }
                    }
                }
                Err(error) => warn!("Level preview: {error}"),
            }
        }
        let active = !streams.is_empty();
        let _ = ready.send(active);
        while active && thread_running.load(Ordering::SeqCst) && generation == GENERATION.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(10));
        }
        for stream in streams { let _ = stream.pause(); drop(stream); }
        thread_running.store(false, Ordering::SeqCst);
    }).map_err(|e| anyhow!("Failed to spawn level monitor: {e}"))?;
    *session = Some(MonitorSession { running: running.clone(), thread: handle });
    let active = opened.await.unwrap_or(false);
    if !active || generation != GENERATION.load(Ordering::SeqCst) {
        stop_session(&mut session).await?;
        return Ok(());
    }
    IS_MONITORING.store(true, Ordering::SeqCst);
    tokio::spawn(async move {
        while running.load(Ordering::SeqCst) && generation == GENERATION.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(80)).await;
            if !running.load(Ordering::SeqCst) || generation != GENERATION.load(Ordering::SeqCst) { break; }
            let snapshot: Vec<AudioLevelData> = levels.lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect();
            if snapshot.is_empty() { continue; }
            let update = AudioLevelUpdate { timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64, levels: snapshot };
            if app_handle.emit("audio-levels", &update).is_err() { running.store(false, Ordering::SeqCst); break; }
        }
    });
    Ok(())
}

pub async fn stop_monitoring() -> Result<()> {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    stop_session(&mut *SESSION.lock().await).await
}

pub fn is_monitoring() -> bool {
    IS_MONITORING.load(Ordering::SeqCst)
}

fn default_monitor_devices(device_type: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();

    #[cfg(target_os = "windows")]
    let host = cpal::host_from_id(cpal::HostId::Wasapi).unwrap_or_else(|_| cpal::default_host());
    #[cfg(not(target_os = "windows"))]
    let host = cpal::default_host();

    if let Some(d) = host.default_input_device().filter(|_| device_type != Some("output")) {
        if let Ok(n) = d.name() {
            out.push(n);
        }
    }
    if let Some(d) = host.default_output_device().filter(|_| device_type != Some("input")) {
        if let Ok(n) = d.name() {
            if !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

/// Open a cpal input stream for levels. Tries mic input first, then WASAPI loopback on output.
fn open_level_stream(device_name: &str, levels: LevelMap, device_type: Option<&str>) -> Result<cpal::Stream> {
    #[cfg(target_os = "windows")]
    let host = cpal::host_from_id(cpal::HostId::Wasapi).unwrap_or_else(|_| cpal::default_host());
    #[cfg(not(target_os = "windows"))]
    let host = cpal::default_host();

    // 1) Input (microphone)
    if let Some(Ok(devices)) = (device_type != Some("output")).then(|| host.input_devices()) {
        for device in devices {
            if let Ok(name) = device.name() {
                if name == device_name || name.contains(device_name) || device_name.contains(&name)
                {
                    if let Ok(cfg) = device.default_input_config() {
                        return build_input_level_stream(
                            &device,
                            cfg,
                            device_name.to_string(),
                            "input",
                            levels,
                        );
                    }
                }
            }
        }
    }

    // 2) Output device via WASAPI loopback (system audio) — build_input_stream on output device
    if let Some(Ok(devices)) = (device_type != Some("input")).then(|| host.output_devices()) {
        for device in devices {
            if let Ok(name) = device.name() {
                if name == device_name || name.contains(device_name) || device_name.contains(&name)
                {
                    // Prefer an input config if the host exposes loopback that way
                    if let Ok(cfg) = device.default_input_config() {
                        return build_input_level_stream(
                            &device,
                            cfg,
                            device_name.to_string(),
                            "output",
                            levels,
                        );
                    }
                    // Fallback: use output config shape with input stream (WASAPI loopback)
                    if let Ok(cfg) = device.default_output_config() {
                        return build_input_level_stream(
                            &device,
                            cfg,
                            device_name.to_string(),
                            "output",
                            levels,
                        );
                    }
                }
            }
        }
    }

    Err(anyhow!("Device not found for level monitor: {}", device_name))
}

fn build_input_level_stream(
    device: &cpal::Device,
    config: cpal::SupportedStreamConfig,
    device_name: String,
    device_type: &'static str,
    levels: LevelMap,
) -> Result<cpal::Stream> {
    let channels = config.channels();
    let sample_format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();
    let err_fn = |e| error!("Level stream error: {}", e);

    let stream = match sample_format {
        SampleFormat::F32 => {
            let levels = levels.clone();
            let name = device_name.clone();
            let dtype = device_type.to_string();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    push_levels(data, channels, &name, &dtype, &levels);
                },
                err_fn,
                None,
            )?
        }
        SampleFormat::I16 => {
            let levels = levels.clone();
            let name = device_name.clone();
            let dtype = device_type.to_string();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let f: Vec<f32> = data.iter().map(|&s| s.to_sample::<f32>()).collect();
                    push_levels(&f, channels, &name, &dtype, &levels);
                },
                err_fn,
                None,
            )?
        }
        SampleFormat::U16 => {
            let levels = levels.clone();
            let name = device_name.clone();
            let dtype = device_type.to_string();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    let f: Vec<f32> = data.iter().map(|&s| s.to_sample::<f32>()).collect();
                    push_levels(&f, channels, &name, &dtype, &levels);
                },
                err_fn,
                None,
            )?
        }
        SampleFormat::I32 => {
            let levels = levels.clone();
            let name = device_name.clone();
            let dtype = device_type.to_string();
            device.build_input_stream(
                &stream_config,
                move |data: &[i32], _| {
                    let f: Vec<f32> = data.iter().map(|&s| s.to_sample::<f32>()).collect();
                    push_levels(&f, channels, &name, &dtype, &levels);
                },
                err_fn,
                None,
            )?
        }
        other => return Err(anyhow!("Unsupported sample format: {:?}", other)),
    };

    Ok(stream)
}

fn push_levels(
    data: &[f32],
    channels: u16,
    device_name: &str,
    device_type: &str,
    levels: &LevelMap,
) {
    if data.is_empty() {
        return;
    }

    let mono: Vec<f32> = if channels > 1 {
        let ch = channels as usize;
        let frames = data.len() / ch;
        let mut m = Vec::with_capacity(frames);
        for i in 0..frames {
            let mut s = 0.0f32;
            for c in 0..ch {
                s += data[i * ch + c];
            }
            m.push(s / ch as f32);
        }
        m
    } else {
        data.to_vec()
    };

    let rms = if mono.is_empty() {
        0.0
    } else {
        (mono.iter().map(|x| x * x).sum::<f32>() / mono.len() as f32).sqrt()
    };
    let peak = mono.iter().map(|x| x.abs()).fold(0.0f32, f32::max);

    if let Ok(mut map) = levels.try_lock() {
        map.insert(
            device_name.to_string(),
            AudioLevelData {
                device_name: device_name.to_string(),
                device_type: device_type.to_string(),
                rms_level: rms.min(1.0),
                peak_level: peak.min(1.0),
                is_active: rms > 0.001,
            },
        );
    }
}
