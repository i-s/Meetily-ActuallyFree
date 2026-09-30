//! The CoreAudio callback/stream handoff, independent of macOS device APIs.

use futures_util::task::AtomicWaker;
use ringbuf::{
    traits::{Consumer, Producer},
    HeapCons, HeapProd,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::task::{Context, Poll};

#[derive(Default)]
pub(super) struct CoreAudioBufferState {
    waker: AtomicWaker,
    consecutive_drops: AtomicU32,
    should_terminate: AtomicBool,
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq)]
enum PollStage {
    BeforeRegistration,
    AfterRegistration,
}

impl CoreAudioBufferState {
    pub(super) fn push_samples(&self, producer: &mut HeapProd<f32>, data: &[f32]) {
        let pushed = producer.push_slice(data);
        if pushed < data.len() {
            let consecutive = self.consecutive_drops.fetch_add(1, Ordering::AcqRel) + 1;
            if consecutive > 10 {
                self.terminate();
                return;
            }
        } else {
            self.consecutive_drops.store(0, Ordering::Release);
        }
        if pushed > 0 {
            self.waker.wake();
        }
    }

    pub(super) fn terminate(&self) {
        // Publish completion before notification, including callbacks that push
        // no samples because the ring is full. Never leave a terminal waiter asleep.
        self.should_terminate.store(true, Ordering::Release);
        self.waker.wake();
    }

    pub(super) fn poll_sample(
        &self,
        consumer: &mut HeapCons<f32>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<f32>> {
        self.poll_sample_inner(
            consumer,
            cx,
            #[cfg(test)]
            |_| {},
        )
    }

    fn poll_sample_inner(
        &self,
        consumer: &mut HeapCons<f32>,
        cx: &mut Context<'_>,
        #[cfg(test)] mut at: impl FnMut(PollStage),
    ) -> Poll<Option<f32>> {
        if let Some(sample) = consumer.try_pop() {
            return Poll::Ready(Some(sample));
        }
        if self.should_terminate.load(Ordering::Acquire) {
            return Poll::Ready(consumer.try_pop());
        }
        #[cfg(test)]
        at(PollStage::BeforeRegistration);
        self.waker.register(cx.waker());
        #[cfg(test)]
        at(PollStage::AfterRegistration);
        // A callback can publish after the empty check but before registration.
        // Register first, then recheck both conditions; later callbacks will wake us.
        if let Some(sample) = consumer.try_pop() {
            return Poll::Ready(Some(sample));
        }
        if self.should_terminate.load(Ordering::Acquire) {
            // The acquire may observe a final push that preceded termination.
            return Poll::Ready(consumer.try_pop());
        }
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::{traits::Split, HeapRb};
    use std::sync::{atomic::AtomicUsize, Arc};
    use std::task::{Wake, Waker};

    #[derive(Default)]
    struct WakeCount(AtomicUsize);

    impl Wake for WakeCount {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn delivery_between_empty_check_and_registration_is_not_lost() {
        let (mut producer, mut consumer) = HeapRb::<f32>::new(4).split();
        let state = CoreAudioBufferState::default();
        let wakes = Arc::new(WakeCount::default());
        let waker = Waker::from(wakes.clone());
        let mut cx = Context::from_waker(&waker);
        let result = state.poll_sample_inner(&mut consumer, &mut cx, |stage| {
            if stage == PollStage::BeforeRegistration {
                state.push_samples(&mut producer, &[0.25]);
            }
        });
        assert_eq!(result, Poll::Ready(Some(0.25)));
        assert_eq!(state.poll_sample(&mut consumer, &mut cx), Poll::Pending);
    }

    #[test]
    fn delivery_after_registration_is_rechecked_before_pending() {
        let (mut producer, mut consumer) = HeapRb::<f32>::new(4).split();
        let state = CoreAudioBufferState::default();
        let wakes = Arc::new(WakeCount::default());
        let waker = Waker::from(wakes.clone());
        let mut cx = Context::from_waker(&waker);
        let result = state.poll_sample_inner(&mut consumer, &mut cx, |stage| {
            if stage == PollStage::AfterRegistration {
                state.push_samples(&mut producer, &[0.0]);
            }
        });
        assert_eq!(result, Poll::Ready(Some(0.0)));
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn delivery_after_pending_wakes_once_and_idle_does_not_spin() {
        let (mut producer, mut consumer) = HeapRb::<f32>::new(4).split();
        let state = CoreAudioBufferState::default();
        let wakes = Arc::new(WakeCount::default());
        let waker = Waker::from(wakes.clone());
        let mut cx = Context::from_waker(&waker);
        assert_eq!(state.poll_sample(&mut consumer, &mut cx), Poll::Pending);
        assert_eq!(wakes.0.load(Ordering::SeqCst), 0);
        state.push_samples(&mut producer, &[0.5, 0.75]);
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
        assert_eq!(
            state.poll_sample(&mut consumer, &mut cx),
            Poll::Ready(Some(0.5))
        );
        assert_eq!(
            state.poll_sample(&mut consumer, &mut cx),
            Poll::Ready(Some(0.75))
        );
        assert_eq!(state.poll_sample(&mut consumer, &mut cx), Poll::Pending);
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn terminal_between_empty_check_and_registration_is_not_lost() {
        let (_producer, mut consumer) = HeapRb::<f32>::new(1).split();
        let state = CoreAudioBufferState::default();
        let waker = Waker::from(Arc::new(WakeCount::default()));
        let mut cx = Context::from_waker(&waker);
        let result = state.poll_sample_inner(&mut consumer, &mut cx, |stage| {
            if stage == PollStage::BeforeRegistration {
                state.terminate();
            }
        });
        assert_eq!(result, Poll::Ready(None));
    }

    #[test]
    fn overflow_terminal_wakes_waiter_and_drains_buffer_before_end() {
        let (mut producer, mut consumer) = HeapRb::<f32>::new(1).split();
        let state = CoreAudioBufferState::default();
        let wakes = Arc::new(WakeCount::default());
        let waker = Waker::from(wakes.clone());
        let mut cx = Context::from_waker(&waker);
        assert_eq!(state.poll_sample(&mut consumer, &mut cx), Poll::Pending);
        // Publish a sample without notification to model the callback interleaving.
        assert_eq!(producer.push_slice(&[0.5]), 1);
        for _ in 0..10 {
            state.push_samples(&mut producer, &[0.75]);
        }
        assert!(!state.should_terminate.load(Ordering::Acquire));
        assert_eq!(wakes.0.load(Ordering::SeqCst), 0);
        state.push_samples(&mut producer, &[0.75]);
        assert!(state.should_terminate.load(Ordering::Acquire));
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
        assert_eq!(
            state.poll_sample(&mut consumer, &mut cx),
            Poll::Ready(Some(0.5))
        );
        assert_eq!(state.poll_sample(&mut consumer, &mut cx), Poll::Ready(None));
    }

    #[test]
    fn successful_callback_resets_consecutive_overflow_count() {
        let (mut producer, mut consumer) = HeapRb::<f32>::new(1).split();
        let state = CoreAudioBufferState::default();
        state.push_samples(&mut producer, &[0.5]);
        for _ in 0..10 {
            state.push_samples(&mut producer, &[0.75]);
        }
        assert_eq!(consumer.try_pop(), Some(0.5));
        state.push_samples(&mut producer, &[0.25]);
        for _ in 0..10 {
            state.push_samples(&mut producer, &[0.75]);
        }
        assert!(!state.should_terminate.load(Ordering::Acquire));
    }
}
