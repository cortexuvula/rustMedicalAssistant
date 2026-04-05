use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;

use crate::{AudioError, AudioResult};

// ──────────────────────────────────────────────────────────────────────────────
// Configuration
// ──────────────────────────────────────────────────────────────────────────────

/// Configuration for the capture pipeline.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Sample rate in Hz (default 16 000).
    pub sample_rate: u32,
    /// Number of channels (default 1 — mono).
    pub channels: u16,
    /// Ring-buffer capacity in frames (default 4 096).
    pub buffer_size: usize,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            channels: 1,
            buffer_size: 4_096,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// CaptureHandle
// ──────────────────────────────────────────────────────────────────────────────

/// A handle to an in-progress audio capture session.
///
/// Dropping the handle stops capture and joins the drain thread.
pub struct CaptureHandle {
    is_paused: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    drain_thread: Option<thread::JoinHandle<()>>,
    // Keep the cpal stream alive as long as the handle lives.
    _stream: cpal::Stream,
}

impl CaptureHandle {
    /// Pause audio capture (samples are discarded while paused).
    pub fn pause(&self) {
        self.is_paused.store(true, Ordering::SeqCst);
    }

    /// Resume audio capture after a pause.
    pub fn resume(&self) {
        self.is_paused.store(false, Ordering::SeqCst);
    }

    /// Stop capture and flush the remaining samples to the WAV file.
    ///
    /// Calling `stop()` is equivalent to dropping the handle, but gives you
    /// an explicit place to handle any panic from the drain thread.
    pub fn stop(mut self) {
        self.do_stop();
    }

    fn do_stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.drain_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.do_stop();
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Public API
// ──────────────────────────────────────────────────────────────────────────────

/// Start an audio capture session.
///
/// Returns a `(CaptureHandle, Receiver<Vec<f32>>)`.  The receiver delivers
/// downsampled waveform snapshots (~128 points, every ~50 ms) so callers can
/// draw a live VU meter without seeing every raw sample.
///
/// Samples are written to `output_path` as a 32-bit float WAV file.
pub fn start_capture(
    device: &cpal::Device,
    config: CaptureConfig,
    output_path: &Path,
) -> AudioResult<(CaptureHandle, mpsc::Receiver<Vec<f32>>)> {
    // ── Build cpal StreamConfig ───────────────────────────────────────────────
    let stream_config = cpal::StreamConfig {
        channels: config.channels,
        sample_rate: cpal::SampleRate(config.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    // ── Ring buffer (2 seconds of audio) ─────────────────────────────────────
    let ring_capacity = (config.sample_rate as usize) * (config.channels as usize) * 2;
    let rb = HeapRb::<f32>::new(ring_capacity.max(config.buffer_size * 4));
    let (mut prod, mut cons) = rb.split();

    let is_paused = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::new(AtomicBool::new(false));

    let is_paused_cb = Arc::clone(&is_paused);

    // ── cpal input stream callback ────────────────────────────────────────────
    let stream = device
        .build_input_stream(
            &stream_config,
            move |data: &[f32], _info: &cpal::InputCallbackInfo| {
                if is_paused_cb.load(Ordering::Relaxed) {
                    return;
                }
                // Push as many samples as fit; silently drop the rest when the
                // ring buffer is full (back-pressure is acceptable for audio).
                prod.push_slice(data);
            },
            |err| {
                tracing::error!("cpal input stream error: {err}");
            },
            None,
        )
        .map_err(|e| AudioError::Capture(e.to_string()))?;

    stream
        .play()
        .map_err(|e| AudioError::Capture(e.to_string()))?;

    // ── WAV writer setup ──────────────────────────────────────────────────────
    let wav_spec = hound::WavSpec {
        channels: config.channels,
        sample_rate: config.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let output_path = output_path.to_path_buf();
    let stop_flag_drain = Arc::clone(&stop_flag);

    // ── waveform channel ──────────────────────────────────────────────────────
    let (waveform_tx, waveform_rx) = mpsc::channel::<Vec<f32>>();

    // Chunk size to accumulate before computing & sending a waveform snapshot.
    // ~50 ms worth of samples.
    let waveform_chunk = (config.sample_rate / 20) as usize;

    // ── Drain thread ──────────────────────────────────────────────────────────
    let drain_handle = thread::spawn(move || {
        let mut writer = match hound::WavWriter::create(&output_path, wav_spec) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("failed to create WAV writer: {e}");
                return;
            }
        };

        let mut acc: Vec<f32> = Vec::with_capacity(waveform_chunk * 2);

        loop {
            // Drain available samples from the ring buffer.
            let batch: Vec<f32> = cons.pop_iter().collect();

            if !batch.is_empty() {
                for &s in &batch {
                    if let Err(e) = writer.write_sample(s) {
                        tracing::error!("WAV write error: {e}");
                    }
                    acc.push(s);
                }

                // Emit waveform snapshot(s).
                while acc.len() >= waveform_chunk {
                    let chunk = acc.drain(..waveform_chunk).collect::<Vec<_>>();
                    let waveform = downsample_waveform(&chunk, 128);
                    let _ = waveform_tx.send(waveform);
                }
            } else if stop_flag_drain.load(Ordering::Relaxed) {
                // Flush remaining accumulator.
                if !acc.is_empty() {
                    let waveform = downsample_waveform(&acc, 128);
                    let _ = waveform_tx.send(waveform);
                }
                break;
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }

        if let Err(e) = writer.finalize() {
            tracing::error!("WAV finalize error: {e}");
        }
    });

    let handle = CaptureHandle {
        is_paused,
        stop_flag,
        drain_thread: Some(drain_handle),
        _stream: stream,
    };

    Ok((handle, waveform_rx))
}

// ──────────────────────────────────────────────────────────────────────────────
// Waveform helper
// ──────────────────────────────────────────────────────────────────────────────

/// Downsample `samples` to `target_len` points by taking the peak absolute
/// value within each chunk.
///
/// If `samples.len() <= target_len` the original slice is returned as-is.
pub fn downsample_waveform(samples: &[f32], target_len: usize) -> Vec<f32> {
    if samples.is_empty() || target_len == 0 {
        return Vec::new();
    }
    if samples.len() <= target_len {
        return samples.to_vec();
    }
    let n = samples.len();
    (0..target_len)
        .map(|i| {
            // Map output index i to an input window [start, end).
            let start = i * n / target_len;
            let end = ((i + 1) * n / target_len).min(n);
            samples[start..end]
                .iter()
                .map(|s| s.abs())
                .fold(0.0f32, f32::max)
        })
        .collect()
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capture_config() {
        let c = CaptureConfig::default();
        assert_eq!(c.sample_rate, 16_000);
        assert_eq!(c.channels, 1);
        assert_eq!(c.buffer_size, 4_096);
    }

    #[test]
    fn downsample_reduces_length() {
        let samples: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        let result = downsample_waveform(&samples, 128);
        assert_eq!(result.len(), 128);
    }

    #[test]
    fn downsample_preserves_short() {
        let samples = vec![0.1f32, 0.5, 0.3];
        let result = downsample_waveform(&samples, 128);
        assert_eq!(result, samples);
    }

    #[test]
    fn downsample_takes_peak() {
        // One chunk: [-0.9, 0.5, 0.3] → peak abs = 0.9
        let samples = vec![-0.9f32, 0.5, 0.3, 0.1, 0.2, 0.4];
        // target_len = 2 → chunk_size = 3
        let result = downsample_waveform(&samples, 2);
        assert_eq!(result.len(), 2);
        assert!((result[0] - 0.9).abs() < 1e-6, "first peak should be 0.9, got {}", result[0]);
        assert!((result[1] - 0.4).abs() < 1e-6, "second peak should be 0.4, got {}", result[1]);
    }
}
