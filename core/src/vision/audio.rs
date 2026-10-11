use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

const RATE: usize = 16_000;

#[derive(Clone, Default)]
pub(crate) struct MicrophoneTap {
    inner: Arc<Mutex<State>>,
    changed: Arc<Notify>,
}

#[derive(Default)]
struct State {
    running: bool,
    next_id: u64,
    watermark: Option<Instant>,
    window: Option<Window>,
}

struct Window {
    id: u64,
    start: Instant,
    end: Option<Instant>,
    limit: usize,
    deadline: Instant,
    samples: Vec<f32>,
    spans: Vec<(Instant, Instant, usize, usize, usize)>,
    covered_start: bool,
    error: Option<String>,
}

pub(crate) struct AudioWindow {
    tap: MicrophoneTap,
    id: u64,
}

pub(crate) struct AudioReader {
    tap: MicrophoneTap,
    id: u64,
    cursor: usize,
    released_since: Option<Instant>,
}

impl MicrophoneTap {
    pub(crate) fn begin(&self, max_seconds: u32) -> Result<AudioWindow, String> {
        self.begin_at(max_seconds, Instant::now())
    }

    fn begin_at(&self, max_seconds: u32, start: Instant) -> Result<AudioWindow, String> {
        let limit = RATE
            .checked_mul(max_seconds as usize)
            .filter(|n| *n > 0)
            .ok_or("Invalid audio window limit")?;
        let deadline = start
            .checked_add(Duration::from_secs(max_seconds as u64))
            .ok_or("Invalid audio window duration")?;
        let mut state = self.inner.lock().expect("microphone tap lock");
        if !state.running {
            return Err("Microphone capture is not running".into());
        }
        if state.window.is_some() {
            return Err("An audio window is already active".into());
        }
        state.next_id = state.next_id.wrapping_add(1);
        let id = state.next_id;
        state.window = Some(Window {
            id,
            start,
            end: None,
            limit,
            deadline,
            samples: Vec::new(),
            spans: Vec::new(),
            covered_start: false,
            error: None,
        });
        Ok(AudioWindow {
            tap: self.clone(),
            id,
        })
    }

    pub(crate) fn started(&self) {
        let mut state = self.inner.lock().expect("microphone tap lock");
        state.running = true;
        state.watermark = None;
    }

    pub(crate) fn stopped(&self, reason: &str) {
        let mut state = self.inner.lock().expect("microphone tap lock");
        state.running = false;
        state.watermark = None;
        if let Some(window) = &mut state.window {
            window.error.get_or_insert_with(|| reason.to_owned());
            window.samples.clear();
        }
        self.changed.notify_one();
    }

    pub(crate) fn discontinuity(&self, reason: &str) {
        let mut state = self.inner.lock().expect("microphone tap lock");
        state.watermark = None;
        if let Some(window) = &mut state.window {
            window.error.get_or_insert_with(|| reason.to_owned());
            window.samples.clear();
        }
        self.changed.notify_one();
    }

    // The producer supplies a continuous, monotonic interval, not queue arrival time.
    pub(crate) fn push(&self, samples: &[f32], start: Instant, end: Instant) {
        let mut state = self.inner.lock().expect("microphone tap lock");
        if !state.running || end <= start || samples.is_empty() {
            return;
        }
        let discontinuous = state.watermark.is_some_and(|previous| previous != start);
        state.watermark = Some(end);
        if let Some(window) = &mut state.window {
            if discontinuous {
                window.error.get_or_insert_with(|| {
                    "Microphone audio has missing or overlapping frames".into()
                });
            }
            if window.error.is_none() && end > window.start && !window.covered_start {
                if start > window.start {
                    window.error = Some("Microphone audio did not cover the window start".into());
                } else {
                    window.covered_start = true;
                }
            }
            if window.error.is_none() {
                let stop = window.end.unwrap_or(window.deadline);
                if stop > window.deadline || (window.end.is_none() && end > window.deadline) {
                    window.error = Some("Audio window exceeded its duration limit".into());
                } else if end > window.start && start < stop {
                    let duration = end.duration_since(start).as_secs_f64();
                    let index = |time: Instant| -> usize {
                        if time <= start {
                            0
                        } else if time >= end {
                            samples.len()
                        } else {
                            // Include sample instants in [press, release).
                            ((time.duration_since(start).as_secs_f64() / duration
                                * samples.len() as f64
                                - 1e-7)
                                .ceil() as usize)
                                .min(samples.len())
                        }
                    };
                    let first = index(window.start);
                    let last = index(stop);
                    if window.samples.len() + last.saturating_sub(first) > window.limit {
                        window.error = Some("Audio window exceeded its sample limit".into());
                    } else {
                        if last > first {
                            window.spans.push((
                                start,
                                end,
                                samples.len(),
                                first,
                                window.samples.len(),
                            ));
                            let additional = last - first;
                            if window.samples.capacity() < window.samples.len() + additional {
                                window.samples.reserve_exact(additional);
                            }
                            window.samples.extend_from_slice(&samples[first..last]);
                        }
                    }
                }
            }
            if window.error.is_some() {
                window.samples.clear();
            }
        }
        self.changed.notify_one();
    }
}

impl AudioWindow {
    pub(crate) fn reader(&self) -> AudioReader {
        AudioReader {
            tap: self.tap.clone(),
            id: self.id,
            cursor: 0,
            released_since: None,
        }
    }

    pub(crate) fn released_at(&self) -> Option<Instant> {
        self.tap
            .inner
            .lock()
            .expect("microphone tap lock")
            .window
            .as_ref()
            .filter(|window| window.id == self.id)
            .and_then(|window| window.end)
    }

    pub(crate) fn release(&mut self) {
        self.release_at(Instant::now());
    }

    fn release_at(&mut self, end: Instant) {
        let mut state = self.tap.inner.lock().expect("microphone tap lock");
        if let Some(window) = state.window.as_mut().filter(|window| window.id == self.id) {
            if window.end.is_none() {
                window.end = Some(end);
                // A producer estimate can be slightly ahead of wall time. Trim
                // samples already collected past the actual release boundary.
                for &(start, stop, count, first, offset) in &window.spans {
                    if end < stop {
                        let last = if end <= start {
                            0
                        } else {
                            ((end.duration_since(start).as_secs_f64()
                                / stop.duration_since(start).as_secs_f64()
                                * count as f64
                                - 1e-7)
                                .ceil() as usize)
                                .min(count)
                        };
                        window.samples.truncate(offset + last.saturating_sub(first));
                        break;
                    }
                }
                if end > window.deadline {
                    window.error = Some("Audio window exceeded its duration limit".into());
                    window.samples.clear();
                }
            }
        }
        self.tap.changed.notify_one();
    }

    #[cfg(test)]
    pub(crate) async fn finish(&mut self) -> Result<Vec<f32>, String> {
        let result = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let notified = self.tap.changed.notified();
                {
                    let mut state = self.tap.inner.lock().expect("microphone tap lock");
                    let watermark = state.watermark;
                    let window = state
                        .window
                        .as_mut()
                        .filter(|window| window.id == self.id)
                        .ok_or("Audio window is no longer active")?;
                    if let Some(error) = &window.error {
                        return Err(error.clone());
                    }
                    let end = window
                        .end
                        .ok_or("Release the audio window before finishing")?;
                    if watermark.is_some_and(|watermark| watermark >= end) {
                        if window.samples.is_empty() {
                            return Err("No microphone audio covered the window".into());
                        }
                        return Ok(std::mem::take(&mut window.samples));
                    }
                }
                notified.await;
            }
        })
        .await
        .unwrap_or_else(|_| Err("Timed out waiting for microphone audio".into()));
        self.cleanup();
        result
    }

    fn cleanup(&self) {
        let mut state = self.tap.inner.lock().expect("microphone tap lock");
        if state
            .window
            .as_ref()
            .is_some_and(|window| window.id == self.id)
        {
            state.window = None;
        }
    }
}

impl AudioReader {
    pub(crate) fn check(&self) -> Result<(), String> {
        let state = self.tap.inner.lock().expect("microphone tap lock");
        let window = state
            .window
            .as_ref()
            .filter(|window| window.id == self.id)
            .ok_or("Audio window is no longer active")?;
        match &window.error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    pub(crate) fn released_at(&self) -> Option<Instant> {
        self.tap
            .inner
            .lock()
            .expect("microphone tap lock")
            .window
            .as_ref()
            .filter(|window| window.id == self.id)
            .and_then(|window| window.end)
    }

    /// Only emit sample instants that have passed wall time. Producer estimates
    /// can lead wall time, and transmitted PCM cannot be retracted on release.
    pub(crate) async fn next(&mut self) -> Result<Option<Vec<f32>>, String> {
        loop {
            {
                let state = self.tap.inner.lock().expect("microphone tap lock");
                let window = state
                    .window
                    .as_ref()
                    .filter(|window| window.id == self.id)
                    .ok_or("Audio window is no longer active")?;
                if let Some(error) = &window.error {
                    return Err(error.clone());
                }
                let now = Instant::now();
                if window.end.is_none() && now > window.deadline {
                    return Err("Audio window exceeded its duration limit".into());
                }
                let complete = window
                    .end
                    .is_some_and(|end| state.watermark.is_some_and(|mark| mark >= end));
                if window.end.is_some() {
                    let since = self.released_since.get_or_insert(now);
                    if !complete && since.elapsed() >= Duration::from_secs(1) {
                        return Err("Timed out waiting for microphone audio".into());
                    }
                }
                let stop = window.end.unwrap_or(now).min(now);
                let mut safe = window.samples.len();
                for &(start, end, count, first, offset) in &window.spans {
                    if stop < end {
                        let last = if stop <= start {
                            0
                        } else {
                            ((stop.duration_since(start).as_secs_f64()
                                / end.duration_since(start).as_secs_f64()
                                * count as f64
                                - 1e-7)
                                .ceil() as usize)
                                .min(count)
                        };
                        safe = (offset + last.saturating_sub(first)).min(safe);
                        break;
                    }
                }
                if safe < self.cursor {
                    return Err("Question audio crossed the release boundary".into());
                }
                if safe > self.cursor {
                    let chunk = window.samples[self.cursor..safe].to_vec();
                    self.cursor = safe;
                    return Ok(Some(chunk));
                }
                if complete {
                    return if self.cursor == 0 {
                        Err("No microphone audio covered the window".into())
                    } else {
                        Ok(None)
                    };
                }
            }
            // One reader polls a bounded window; no per-frame tasks or queues.
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

impl Drop for AudioWindow {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> (MicrophoneTap, Instant) {
        let tap = MicrophoneTap::default();
        tap.started();
        (tap, Instant::now())
    }

    #[tokio::test]
    async fn live_reader_defers_future_pcm_and_preserves_release_tail() {
        let (tap, base) = running();
        let mut window = tap.begin_at(1, base).unwrap();
        let mut reader = window.reader();
        tap.push(&[0.1; 16000], base, base + Duration::from_secs(1));
        tokio::time::sleep(Duration::from_millis(10)).await;
        let first = reader.next().await.unwrap().unwrap();
        assert!(first.len() < 16000);
        window.release();
        let expected = tap
            .inner
            .lock()
            .unwrap()
            .window
            .as_ref()
            .unwrap()
            .samples
            .len();
        let mut total = first.len();
        while let Some(chunk) = reader.next().await.unwrap() {
            total += chunk.len();
        }
        assert_eq!(total, expected);
    }

    #[tokio::test]
    async fn live_reader_waits_for_tail_and_drop_invalidates_it() {
        let (tap, base) = running();
        let mut window = tap.begin_at(1, base).unwrap();
        let mut reader = window.reader();
        tap.push(&[0.1; 16], base, base + Duration::from_millis(1));
        tokio::time::sleep(Duration::from_millis(2)).await;
        assert_eq!(reader.next().await.unwrap().unwrap().len(), 16);
        window.release_at(base + Duration::from_millis(2));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), reader.next())
                .await
                .is_err()
        );
        tap.push(
            &[0.2; 32],
            base + Duration::from_millis(1),
            base + Duration::from_millis(3),
        );
        assert_eq!(reader.next().await.unwrap().unwrap(), vec![0.2; 16]);
        assert!(reader.next().await.unwrap().is_none());
        drop(window);
        assert!(reader.next().await.is_err());
        assert!(tap.begin(1).is_ok());
    }

    #[tokio::test]
    async fn clips_both_boundaries_and_waits_for_late_release_coverage() {
        let (tap, base) = running();
        let mut window = tap.begin_at(1, base + Duration::from_millis(2)).unwrap();
        window.release_at(base + Duration::from_millis(6));
        let producer = tap.clone();
        let task = tokio::spawn(async move { window.finish().await });
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        producer.push(
            &(0..160).map(|n| n as f32).collect::<Vec<_>>(),
            base,
            base + Duration::from_millis(10),
        );
        let audio = task.await.unwrap().unwrap();
        assert_eq!(audio, (32..96).map(|n| n as f32).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn release_trims_samples_already_past_the_boundary() {
        let (tap, base) = running();
        let mut window = tap.begin_at(1, base + Duration::from_millis(2)).unwrap();
        tap.push(
            &(0..160).map(|n| n as f32).collect::<Vec<_>>(),
            base,
            base + Duration::from_millis(10),
        );
        window.release_at(base + Duration::from_millis(6));
        // Repeated release calls must preserve the first release instant.
        window.release_at(base + Duration::from_millis(9));
        assert_eq!(
            window.finish().await.unwrap(),
            (32..96).map(|n| n as f32).collect::<Vec<_>>()
        );
        assert!(tap.begin(1).is_ok());
    }

    #[tokio::test]
    async fn separate_chunks_have_distinct_boundaries() {
        let (tap, base) = running();
        let mut window = tap.begin_at(1, base + Duration::from_millis(5)).unwrap();
        tap.push(&[1.0; 160], base, base + Duration::from_millis(10));
        window.release_at(base + Duration::from_millis(15));
        tap.push(
            &[2.0; 160],
            base + Duration::from_millis(10),
            base + Duration::from_millis(20),
        );
        let audio = window.finish().await.unwrap();
        assert_eq!(audio.len(), 160);
        assert_eq!(&audio[..80], &[1.0; 80]);
        assert_eq!(&audio[80..], &[2.0; 80]);
    }

    #[test]
    fn requires_running_and_rejects_duplicate_until_drop() {
        let tap = MicrophoneTap::default();
        assert!(tap.begin(1).is_err());
        tap.started();
        assert!(tap.begin(0).is_err());
        let window = tap.begin(1).unwrap();
        assert!(tap.begin(1).is_err());
        drop(window);
        assert!(tap.begin(1).is_ok());
    }

    #[tokio::test]
    async fn stop_and_device_reset_cancel_windows() {
        for reason in ["Capture stopped", "Device changed"] {
            let (tap, base) = running();
            let mut window = tap.begin_at(1, base).unwrap();
            window.release_at(base + Duration::from_millis(1));
            tap.stopped(reason);
            tap.started();
            assert_eq!(window.finish().await.unwrap_err(), reason);
        }
    }

    #[tokio::test]
    async fn duration_and_sample_limits_fail_without_retaining_audio() {
        let (tap, base) = running();
        let mut window = tap.begin_at(1, base).unwrap();
        tap.push(&vec![0.0; RATE + 1], base, base + Duration::from_secs(1));
        window.release_at(base + Duration::from_secs(1));
        assert!(window.finish().await.is_err());
        let mut window = tap.begin_at(1, base).unwrap();
        window.release_at(base + Duration::from_secs(2));
        assert!(window.finish().await.is_err());
    }

    #[tokio::test]
    async fn missing_frames_and_empty_windows_fail() {
        let (tap, base) = running();
        let mut window = tap.begin_at(1, base).unwrap();
        tap.push(&[1.0; 16], base, base + Duration::from_millis(1));
        tap.push(
            &[1.0; 16],
            base + Duration::from_millis(2),
            base + Duration::from_millis(3),
        );
        window.release_at(base + Duration::from_millis(3));
        assert!(window.finish().await.is_err());
        let mut window = tap.begin_at(1, base + Duration::from_millis(3)).unwrap();
        window.release_at(base + Duration::from_millis(3));
        assert!(window.finish().await.is_err());
    }
}
