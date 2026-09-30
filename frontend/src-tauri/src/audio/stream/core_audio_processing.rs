//! Shared by the native Core Audio task and device-free regression tests.
use std::future::poll_fn;
use std::pin::Pin;
use std::sync::Arc;

use futures_util::Stream;
use log::{error, info};

use crate::audio::recording_state::{AudioError, RecordingState};

pub(in crate::audio) async fn process_core_audio_stream<S, R, P>(
    mut stream: S,
    sample_rate_of: R,
    mut process_samples: P,
    state: Arc<RecordingState>,
    device_name: String,
    sample_rate: u32,
) where
    S: Stream<Item = f32> + Unpin,
    R: Fn(&S) -> u32,
    P: FnMut(&[f32]),
{
    let mut buffer = Vec::with_capacity(1024);
    let mut terminal_error_reported = false;
    info!("Core Audio processing task started for {}", device_name);

    loop {
        // A silent native tap may remain Pending indefinitely. Flush its tail at
        // the first empty poll, while capture still timestamps those samples at
        // their original delivery time. Carrying it into the next 1024 block
        // would replay old audio at the far side of a long callback-free gap.
        let sample = match poll_fn(|cx| {
            let next = Pin::new(&mut stream).poll_next(cx);
            if next.is_pending() && !buffer.is_empty() {
                process_samples(&buffer);
                buffer.clear();
            }
            next
        })
        .await
        {
            Some(sample) => sample,
            None => break,
        };
        let current_sample_rate = sample_rate_of(&stream);
        if current_sample_rate != sample_rate {
            error!(
                "Core Audio sample rate changed during recording: {} -> {} Hz",
                sample_rate, current_sample_rate
            );
            state.report_error(AudioError::SampleRateUnsupported);
            terminal_error_reported = true;
            break;
        }
        buffer.push(sample);
        if buffer.len() >= 1024 {
            process_samples(&buffer);
            buffer.clear();
        }
    }
    if !buffer.is_empty() {
        process_samples(&buffer);
    }
    if !terminal_error_reported && state.is_recording() {
        error!("Core Audio stream ended unexpectedly for {}", device_name);
        state.report_error(AudioError::ChannelClosed);
    }
    info!("Core Audio processing task ended for {}", device_name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::task::{Context, Poll};
    use tokio::sync::mpsc;
    use tokio::task::JoinHandle;
    use tokio::time::{advance, Duration};

    struct Source {
        receiver: futures::channel::mpsc::UnboundedReceiver<(f32, u32)>,
        rate: u32,
        dropped: Arc<AtomicBool>,
    }

    impl Stream for Source {
        type Item = f32;
        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<f32>> {
            Pin::new(&mut self.receiver).poll_next(cx).map(|sample| {
                sample.map(|(value, rate)| {
                    self.rate = rate;
                    value
                })
            })
        }
    }

    impl Drop for Source {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    struct Capture {
        sender: futures::channel::mpsc::UnboundedSender<(f32, u32)>,
        chunks: mpsc::UnboundedReceiver<Vec<f32>>,
        task: JoinHandle<()>,
        state: Arc<RecordingState>,
        dropped: Arc<AtomicBool>,
        fatal_callbacks: Arc<AtomicU32>,
    }

    impl Capture {
        fn start() -> Self {
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            let (chunk_sender, chunks) = mpsc::unbounded_channel();
            let dropped = Arc::new(AtomicBool::new(false));
            let state = RecordingState::new();
            state.start_recording().unwrap();
            let fatal_callbacks = Arc::new(AtomicU32::new(0));
            let errors = fatal_callbacks.clone();
            state.set_error_callback(move |_| {
                errors.fetch_add(1, Ordering::SeqCst);
            });
            let task = tokio::spawn(process_core_audio_stream(
                Source {
                    receiver,
                    rate: 48_000,
                    dropped: dropped.clone(),
                },
                |source| source.rate,
                move |samples| {
                    chunk_sender.send(samples.to_vec()).unwrap();
                },
                state.clone(),
                "synthetic".into(),
                48_000,
            ));
            Self {
                sender,
                chunks,
                task,
                state,
                dropped,
                fatal_callbacks,
            }
        }

        fn send(&self, data: &[f32]) {
            for &sample in data {
                self.sender.unbounded_send((sample, 48_000)).unwrap();
            }
        }

        fn assert_healthy(&self) {
            assert!(
                !self.task.is_finished(),
                "idle source must keep its processing task alive"
            );
            assert!(self.state.is_recording());
            assert_eq!(self.state.get_error_count(), 0);
            assert_eq!(self.fatal_callbacks.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn initial_pending_for_sixty_seconds_keeps_recording_active() {
        let capture = Capture::start();
        tokio::task::yield_now().await;
        advance(Duration::from_secs(60)).await;
        tokio::task::yield_now().await;
        capture.assert_healthy();
        capture.task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn samples_resume_after_thirty_seconds_pending_in_same_recording() {
        let mut capture = Capture::start();
        capture.send(&vec![0.25; 1024]);
        tokio::task::yield_now().await;
        assert_eq!(capture.chunks.try_recv().unwrap(), vec![0.25; 1024]);
        advance(Duration::from_secs(30)).await;
        tokio::task::yield_now().await;
        capture.assert_healthy();
        capture.send(&vec![0.75; 1024]);
        tokio::task::yield_now().await;
        assert_eq!(capture.chunks.try_recv().unwrap(), vec![0.75; 1024]);
        capture.assert_healthy();
        capture.task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn zero_samples_are_delivered_as_data() {
        let mut capture = Capture::start();
        capture.send(&vec![0.0; 1024]);
        tokio::task::yield_now().await;
        assert_eq!(capture.chunks.try_recv().unwrap(), vec![0.0; 1024]);
        capture.assert_healthy();
        capture.task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn partial_chunk_is_delivered_before_pending_and_not_replayed_on_resume() {
        let mut capture = Capture::start();
        capture.send(&vec![0.25; 137]);
        tokio::task::yield_now().await;
        assert_eq!(
            capture
                .chunks
                .try_recv()
                .expect("partial block must precede the gap"),
            vec![0.25; 137]
        );
        advance(Duration::from_secs(30)).await;
        tokio::task::yield_now().await;
        capture.send(&vec![0.75; 1024]);
        tokio::task::yield_now().await;
        assert_eq!(capture.chunks.try_recv().unwrap(), vec![0.75; 1024]);
        assert!(capture.chunks.try_recv().is_err());
        capture.assert_healthy();
        capture.task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn abort_while_pending_releases_source_without_callback_or_error() {
        let capture = Capture::start();
        tokio::task::yield_now().await;
        capture.state.stop_recording();
        capture.task.abort();
        assert!(capture.task.await.unwrap_err().is_cancelled());
        assert!(capture.dropped.load(Ordering::SeqCst));
        assert_eq!(capture.state.get_error_count(), 0);
        assert_eq!(capture.fatal_callbacks.load(Ordering::SeqCst), 0);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test(start_paused = true)]
    async fn audio_stream_stop_while_pending_releases_source_without_callback() {
        let capture = Capture::start();
        tokio::task::yield_now().await;
        capture.state.stop_recording();
        let stream = super::super::AudioStream {
            device: Arc::new(crate::audio::devices::AudioDevice::new(
                "synthetic".into(),
                crate::audio::devices::DeviceType::Output,
            )),
            backend: super::super::StreamBackend::CoreAudio {
                task: Some(capture.task),
            },
        };
        stream.stop().unwrap();
        tokio::task::yield_now().await;
        assert!(capture.dropped.load(Ordering::SeqCst));
        assert_eq!(capture.state.get_error_count(), 0);
        assert_eq!(capture.fatal_callbacks.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn unexpected_none_remains_fatal_and_flushes_residual_samples() {
        let mut capture = Capture::start();
        capture.send(&vec![0.5; 17]);
        drop(capture.sender);
        capture.task.await.unwrap();
        assert_eq!(capture.chunks.try_recv().unwrap(), vec![0.5; 17]);
        assert!(!capture.state.is_recording());
        assert!(matches!(
            capture.state.get_last_error(),
            Some(AudioError::ChannelClosed)
        ));
        assert_eq!(capture.state.get_error_count(), 1);
        assert_eq!(capture.fatal_callbacks.load(Ordering::SeqCst), 1);
        assert!(capture.dropped.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn changed_sample_rate_remains_fatal_without_delivering_incompatible_sample() {
        let mut capture = Capture::start();
        capture.sender.unbounded_send((0.75, 44_100)).unwrap();
        capture.task.await.unwrap();
        assert!(capture.chunks.try_recv().is_err());
        assert!(!capture.state.is_recording());
        assert!(matches!(
            capture.state.get_last_error(),
            Some(AudioError::SampleRateUnsupported)
        ));
        assert_eq!(capture.state.get_error_count(), 1);
        assert_eq!(capture.fatal_callbacks.load(Ordering::SeqCst), 1);
        assert!(capture.dropped.load(Ordering::SeqCst));
    }
}
