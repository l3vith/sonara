pub mod capture;
pub mod playback;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AudioSource {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub subtitle: String,
}

#[derive(Debug, Clone)]
pub struct PcmChunk {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
}

pub fn f32_to_i16(src: &[f32]) -> Vec<i16> {
    src.iter()
        .map(|s| {
            let c = s.clamp(-1.0, 1.0);
            (c * i16::MAX as f32) as i16
        })
        .collect()
}

pub fn i16_to_f32(src: &[i16]) -> Vec<f32> {
    src.iter().map(|s| *s as f32 / i16::MAX as f32).collect()
}

pub fn rms_level(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples
        .iter()
        .map(|s| {
            let x = *s as f64 / 32768.0;
            x * x
        })
        .sum();
    (sum / samples.len() as f64).sqrt() as f32
}

/// Convert interleaved PCM to 48k stereo i16. Linear resample, upmix/downmix.
pub fn to_stream_format(chunk: &PcmChunk, sample_rate: u32, _channels: u16) -> Vec<i16> {
    let stereo = to_stereo(&chunk.samples, chunk.channels);
    if chunk.sample_rate == sample_rate {
        return stereo;
    }
    resample_linear(&stereo, chunk.sample_rate, sample_rate)
}

fn to_stereo(samples: &[i16], channels: u16) -> Vec<i16> {
    match channels {
        0 | 1 => samples.iter().flat_map(|s| [*s, *s]).collect(),
        2 => samples.to_vec(),
        n => samples
            .chunks(n as usize)
            .flat_map(|frame| {
                let l = frame.first().copied().unwrap_or(0);
                let r = frame.get(1).copied().unwrap_or(l);
                [l, r]
            })
            .collect(),
    }
}

fn resample_linear(stereo: &[i16], from: u32, to: u32) -> Vec<i16> {
    if from == 0 || stereo.len() < 2 {
        return stereo.to_vec();
    }
    let in_frames = stereo.len() / 2;
    let out_frames = (in_frames as u64 * to as u64 / from as u64).max(1) as usize;
    let mut out = Vec::with_capacity(out_frames * 2);
    for i in 0..out_frames {
        let src = i as f64 * from as f64 / to as f64;
        let i0 = src.floor() as usize;
        let i1 = (i0 + 1).min(in_frames - 1);
        let t = (src - i0 as f64) as f32;
        for ch in 0..2 {
            let a = stereo[i0 * 2 + ch] as f32;
            let b = stereo[i1 * 2 + ch] as f32;
            out.push((a + (b - a) * t) as i16);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_audio_is_upmixed_to_stereo() {
        let chunk = PcmChunk {
            samples: vec![100, -200],
            sample_rate: 48_000,
            channels: 1,
        };
        assert_eq!(to_stream_format(&chunk, 48_000, 2), vec![100, 100, -200, -200]);
    }

    #[test]
    fn resampling_preserves_stereo_frame_shape() {
        let chunk = PcmChunk {
            samples: vec![100, -100, 300, -300],
            sample_rate: 24_000,
            channels: 2,
        };
        let output = to_stream_format(&chunk, 48_000, 2);
        assert_eq!(output.len(), 8);
        assert_eq!(&output[..2], &[100, -100]);
    }
}
