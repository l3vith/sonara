use crate::audio::{f32_to_i16, to_stream_format, AudioSource, PcmChunk};
use anyhow::Result;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::SyncSender,
    Arc,
};

pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl CaptureHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.join.take() {
            let _ = h.join();
        }
    }
}

pub fn list_sources() -> Result<Vec<AudioSource>> {
    #[cfg(target_os = "macos")]
    {
        return macos::list_sources();
    }
    #[cfg(target_os = "windows")]
    {
        return windows::list_sources();
    }
    #[cfg(target_os = "linux")]
    {
        return linux::list_sources();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        anyhow::bail!("Audio capture is not supported on this OS yet.");
    }
}

pub fn start(source_id: &str, tx: SyncSender<PcmChunk>) -> Result<CaptureHandle> {
    #[cfg(target_os = "macos")]
    {
        return macos::start(source_id, tx);
    }
    #[cfg(target_os = "windows")]
    {
        return windows::start(source_id, tx);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::start(source_id, tx);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (source_id, tx);
        anyhow::bail!("Audio capture is not supported on this OS yet.");
    }
}

pub fn send_chunk(tx: &SyncSender<PcmChunk>, chunk: PcmChunk) {
    let samples = to_stream_format(&chunk);
    let framed = PcmChunk {
        samples,
        sample_rate: crate::protocol::SAMPLE_RATE,
        channels: crate::protocol::CHANNELS,
    };
    let _ = tx.try_send(framed);
}

#[allow(dead_code)]
pub fn pack_f32(samples: &[f32], rate: u32, channels: u16) -> PcmChunk {
    PcmChunk {
        samples: f32_to_i16(samples),
        sample_rate: rate,
        channels,
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
