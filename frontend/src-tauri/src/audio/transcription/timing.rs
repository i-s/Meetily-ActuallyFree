//! Opt-in, text-free ASR diagnostics for issue #8. Not an end-to-end UI clock.
//! The dispatcher clock starts after VAD; physical speech/UI landmarks are still required.
use std::time::Instant;

pub struct ChunkTiming {
    queued: Instant,
    started: Option<Instant>,
    chunk_id: u64,
    audio_start_seconds: f64,
    audio_duration_seconds: f64,
    source: &'static str,
    outcome: &'static str,
}

impl ChunkTiming {
    pub fn new(chunk_id: u64, start: f64, duration: f64, source: &'static str) -> Self {
        Self {
            queued: Instant::now(), started: None, chunk_id,
            audio_start_seconds: start, audio_duration_seconds: duration, source,
            outcome: "aborted",
        }
    }

    pub fn start(&mut self) { self.started = Some(Instant::now()); }
    pub fn outcome(&mut self, outcome: &'static str) { self.outcome = outcome; }

    pub fn json(&self) -> String {
        let total = self.queued.elapsed().as_secs_f64();
        let processing = self.started.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
        format!(
            "{{\"kind\":\"asr_chunk\",\"chunk_id\":{},\"capture_source\":\"{}\",\"audio_start_seconds\":{},\"audio_duration_seconds\":{},\"queue_seconds\":{},\"processing_seconds\":{},\"total_seconds\":{},\"outcome\":\"{}\"}}",
            self.chunk_id, self.source, self.audio_start_seconds, self.audio_duration_seconds,
            (total - processing).max(0.0), processing, total, self.outcome,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_and_failure_are_explicit() {
        let mut timing = ChunkTiming::new(7, 2.0, 3.0, "system");
        assert!(timing.json().contains("\"outcome\":\"aborted\""));
        timing.start();
        timing.outcome("emitted");
        let json = timing.json();
        assert!(json.contains("\"capture_source\":\"system\""));
        assert!(json.contains("\"outcome\":\"emitted\""));
        assert!(!json.contains("text"));
    }
}
