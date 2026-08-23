pub const ALPN: &[u8] = b"sonora/audio/1";
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;
pub const MAGIC: &[u8; 4] = b"SNR1";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Ctrl {
    Hello {
        name: String,
        role: String,
    },
    Room {
        host: String,
        source: String,
        rate: u32,
        channels: u16,
    },
    Peers {
        names: Vec<String>,
    },
    Bye {
        reason: String,
    },
}

pub fn encode_audio_frame(seq: u64, pcm: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + pcm.len() * 2);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

pub fn decode_audio_frame(buf: &[u8]) -> Option<(u64, Vec<i16>)> {
    if buf.len() < 16 || &buf[0..4] != MAGIC {
        return None;
    }
    let seq = u64::from_le_bytes(buf[4..12].try_into().ok()?);
    let n = u32::from_le_bytes(buf[12..16].try_into().ok()?) as usize;
    if buf.len() != 16 + n * 2 {
        return None;
    }
    let mut pcm = Vec::with_capacity(n);
    for chunk in buf[16..].chunks_exact(2) {
        pcm.push(i16::from_le_bytes([chunk[0], chunk[1]]));
    }
    Some((seq, pcm))
}

pub fn write_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_frames_round_trip() {
        let samples = [-32_768, -42, 0, 42, 32_767];
        let encoded = encode_audio_frame(12, &samples);
        assert_eq!(decode_audio_frame(&encoded), Some((12, samples.to_vec())));
    }

    #[test]
    fn malformed_audio_frames_are_rejected() {
        assert_eq!(decode_audio_frame(b"bad"), None);
        let mut encoded = encode_audio_frame(1, &[1, 2]);
        encoded.pop();
        assert_eq!(decode_audio_frame(&encoded), None);
    }
}
