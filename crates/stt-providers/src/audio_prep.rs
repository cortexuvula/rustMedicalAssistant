//! Audio preprocessing: resample to 16 kHz mono for whisper and pyannote.

use medical_core::types::AudioData;

/// Resample audio to 16 kHz mono f32.
/// Uses linear interpolation — good enough for speech.
pub fn to_16k_mono_f32(audio: &AudioData) -> Vec<f32> {
    let channels = audio.channels.max(1) as usize;
    let mono: Vec<f32> = if channels > 1 {
        audio.samples.chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        audio.samples.clone()
    };
    let src_rate = audio.sample_rate as f64;
    let dst_rate = 16_000.0_f64;
    if (src_rate - dst_rate).abs() < 1.0 {
        return mono;
    }
    let ratio = src_rate / dst_rate;
    let out_len = (mono.len() as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_idx = i as f64 * ratio;
        let idx0 = src_idx.floor() as usize;
        let idx1 = (idx0 + 1).min(mono.len().saturating_sub(1));
        let frac = (src_idx - idx0 as f64) as f32;
        out.push(mono[idx0] * (1.0 - frac) + mono[idx1] * frac);
    }
    out
}

/// Convert f32 PCM samples to i16 (for diarization APIs that expect i16 input).
pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples.iter()
        .map(|&s| (s * 32_767.0).clamp(-32_768.0, 32_767.0) as i16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use medical_core::types::AudioData;

    #[test]
    fn passthrough_16k_mono() {
        let samples: Vec<f32> = (0..160).map(|i| i as f32 / 160.0).collect();
        let audio = AudioData {
            samples: samples.clone(),
            sample_rate: 16_000,
            channels: 1,
        };
        let result = to_16k_mono_f32(&audio);
        assert_eq!(result.len(), samples.len());
        for (a, b) in result.iter().zip(samples.iter()) {
            assert!((a - b).abs() < 1e-6, "passthrough mismatch: {a} vs {b}");
        }
    }

    #[test]
    fn stereo_to_mono() {
        // L=1.0, R=0.0 interleaved: [1.0, 0.0, 1.0, 0.0, ...]
        let samples: Vec<f32> = (0..20).map(|i| if i % 2 == 0 { 1.0 } else { 0.0 }).collect();
        let audio = AudioData {
            samples,
            sample_rate: 16_000,
            channels: 2,
        };
        let result = to_16k_mono_f32(&audio);
        assert_eq!(result.len(), 10);
        for &v in &result {
            assert!((v - 0.5).abs() < 1e-6, "stereo mix expected 0.5, got {v}");
        }
    }

    #[test]
    fn downsample_44100_to_16000() {
        let num_samples = 44_100usize;
        let samples: Vec<f32> = (0..num_samples).map(|i| (i as f32).sin()).collect();
        let audio = AudioData {
            samples,
            sample_rate: 44_100,
            channels: 1,
        };
        let result = to_16k_mono_f32(&audio);
        // Expected ~16000 samples; allow ±2 for ceiling rounding
        let expected = (num_samples as f64 * 16_000.0 / 44_100.0).ceil() as usize;
        assert!(
            (result.len() as isize - expected as isize).abs() <= 2,
            "expected ~{expected} samples, got {}",
            result.len()
        );
    }

    #[test]
    fn f32_to_i16_conversion() {
        let samples = vec![1.0f32, -1.0, 0.0, 0.5];
        let result = f32_to_i16(&samples);
        assert_eq!(result[0], 32_767);
        assert_eq!(result[1], -32_767);
        assert_eq!(result[2], 0);
        // 0.5 * 32767 = 16383.5 → truncates to 16383
        assert_eq!(result[3], 16_383);
    }

    #[test]
    fn empty_audio() {
        let audio = AudioData {
            samples: vec![],
            sample_rate: 44_100,
            channels: 1,
        };
        let resampled = to_16k_mono_f32(&audio);
        assert!(resampled.is_empty(), "expected empty output for empty input");

        let converted = f32_to_i16(&[]);
        assert!(converted.is_empty(), "expected empty i16 output for empty input");
    }
}
