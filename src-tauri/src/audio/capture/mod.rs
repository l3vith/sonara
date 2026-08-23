use crate::audio::{f32_to_i16, AudioSource, PcmChunk};
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

/// Window titles are mutable, so resolve this from the current shareable
/// content instead of retaining the label selected when the room was opened.
pub fn source_label(source_id: &str) -> Result<Option<String>> {
    Ok(list_sources()?
        .into_iter()
        .find(|source| source.id == source_id)
        .map(|source| source.title))
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
    let _ = tx.try_send(chunk);
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
