#[path = "encoder_process.rs"]
mod encoder_process;
use super::ffmpeg::find_ffmpeg_path; // Correct path to encode module
use super::AudioDevice;
use std::sync::Arc;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};
use tracing::{debug, error};

pub struct AudioInput {
    pub data: Arc<Vec<f32>>,
    pub sample_rate: u32,
    pub channels: u16,
    pub device: Arc<AudioDevice>,
}

pub fn encode_single_audio(
    data: &[u8],
    sample_rate: u32,
    channels: u16,
    output_path: &PathBuf,
) -> anyhow::Result<()> {
    debug!("Starting FFmpeg process for {} bytes of audio data", data.len());

    if data.is_empty() {
        return Err(anyhow::anyhow!("No audio data provided for encoding"));
    }

    let ffmpeg_path = find_ffmpeg_path().ok_or_else(|| {
        anyhow::anyhow!("FFmpeg not found. Please install FFmpeg to save recordings.")
    })?;
    let output_path = output_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Recording output path is not valid UTF-8"))?;

    debug!("Using FFmpeg at: {:?}", ffmpeg_path);

    let mut command = Command::new(ffmpeg_path);
    command
        .args([
            "-f",
            "f32le",
            "-ar",
            &sample_rate.to_string(),
            "-ac",
            &channels.to_string(),
            "-i",
            "pipe:0",
            "-c:a",
            "aac",
            "-b:a",
            "192k", // Increased from 64k for better audio quality (especially for speech)
            "-profile:a",
            "aac_low", // Use AAC-LC profile for better compatibility
            "-movflags",
            "+faststart", // Optimize for web streaming
            "-f",
            "mp4",
            output_path,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Hide console window on Windows to prevent CMD popup during recording
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    debug!("FFmpeg command: {:?}", command);
    log::info!("[recording-save] encoder_start bytes={}", data.len());

    let ffmpeg = command
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn FFmpeg process: {}", e))?;
    let output = encoder_process::write_audio_and_wait(ffmpeg, data)?;
    log::info!("[recording-save] encoder_complete status={}", output.status);
    let status = output.status;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    debug!("FFmpeg process exited with status: {}", status);
    debug!("FFmpeg stdout: {}", stdout);
    debug!("FFmpeg stderr: {}", stderr);

    if !status.success() {
        error!("FFmpeg process failed with status: {}", status);
        error!("FFmpeg stderr: {}", stderr);
        return Err(anyhow::anyhow!(
            "FFmpeg process failed with status: {}: {}",
            status, stderr.trim()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_thirty_five_seconds_including_final_tail() {
        let output = std::env::temp_dir().join(format!("meetily-encoder-{}-{}.mp4", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let count = 35 * 48_000;
        let pcm: Vec<u8> = (0..count).flat_map(|i| {
            let sample = 0.2 * (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin();
            sample.to_le_bytes()
        }).collect();
        encode_single_audio(&pcm, 48_000, 1, &output).unwrap();
        let decoded = Command::new(find_ffmpeg_path().expect("test needs bundled or installed FFmpeg"))
            .args(["-v", "error", "-i"]).arg(&output)
            .args(["-f", "f32le", "-ac", "1", "-ar", "48000", "pipe:1"])
            .output().unwrap();
        let _ = std::fs::remove_file(&output);
        assert!(decoded.status.success(), "{}", String::from_utf8_lossy(&decoded.stderr));
        let actual = decoded.stdout.len() / 4;
        assert!((count..=count + 1024).contains(&actual), "decoded {actual}, expected {count} plus AAC padding");
        let final_energy: f32 = decoded.stdout[(count - 4800) * 4..count * 4]
            .chunks_exact(4).map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()).powi(2)).sum();
        assert!((final_energy / 4800.0).sqrt() > 0.1, "last 100ms must retain signal");
    }
}
