//! Per-window / per-app / system audio via ScreenCaptureKit (macOS 13+).
use super::{pack_f32, send_chunk, CaptureHandle};
use crate::audio::AudioSource;
use anyhow::{anyhow, Context, Result};
use screencapturekit::prelude::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::SyncSender,
    Arc,
};

pub fn list_sources() -> Result<Vec<AudioSource>> {
    let content =
        SCShareableContent::get().map_err(|e| anyhow!("Screen capture list failed: {e}"))?;
    let mut out = Vec::new();

    if let Some(display) = content.displays().first() {
        let id = display.display_id();
        out.push(AudioSource {
            id: format!("system:{id}"),
            kind: "system".into(),
            title: "This Mac".into(),
            subtitle: "Everything you hear".into(),
        });
    }

    for app in content.applications() {
        let name = app.application_name();
        if name.is_empty() || name == "Sonora" {
            continue;
        }
        let pid = app.process_id();
        let bundle = app.bundle_identifier();
        out.push(AudioSource {
            id: format!("app:{pid}"),
            kind: "app".into(),
            title: name,
            subtitle: if bundle.is_empty() {
                format!("pid {pid}")
            } else {
                bundle
            },
        });
    }

    for win in content.windows() {
        let title = win.title().unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let id = win.window_id();
        let app = win
            .owning_application()
            .map(|a| a.application_name())
            .unwrap_or_default();
        out.push(AudioSource {
            id: format!("window:{id}"),
            kind: "window".into(),
            title,
            subtitle: app,
        });
    }

    Ok(out)
}

pub fn start(source_id: &str, tx: SyncSender<crate::audio::PcmChunk>) -> Result<CaptureHandle> {
    let content = SCShareableContent::get().map_err(|e| anyhow!("{e}"))?;
    let filter = build_filter(&content, source_id)?;

    let config = SCStreamConfiguration::new()
        .with_width(64)
        .with_height(64)
        .with_captures_audio(true)
        .with_sample_rate(48_000)
        .with_channel_count(2);

    let stop = Arc::new(AtomicBool::new(false));
    let handler = AudioHandler { tx };

    let mut stream = SCStream::new(&filter, &config);
    stream
        .add_output_handler(handler, SCStreamOutputType::Audio)
        .ok_or_else(|| anyhow!("Could not attach an audio output. Grant Screen Recording to Sonora in System Settings."))?;
    stream
        .start_capture()
        .map_err(|e| anyhow!("Could not start capture: {e}. Check Screen Recording permission."))?;

    let stop_flag = stop.clone();
    let join = std::thread::spawn(move || {
        while !stop_flag.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = stream.stop_capture();
    });

    Ok(CaptureHandle {
        stop,
        join: Some(join),
    })
}

fn build_filter(content: &SCShareableContent, source_id: &str) -> Result<SCContentFilter> {
    if let Some(rest) = source_id.strip_prefix("system:") {
        let want: u32 = rest.parse().unwrap_or(0);
        let display = content
            .displays()
            .into_iter()
            .find(|d| d.display_id() == want)
            .or_else(|| content.displays().into_iter().next())
            .ok_or_else(|| anyhow!("No display available for system audio."))?;
        return Ok(SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build());
    }
    if let Some(rest) = source_id.strip_prefix("app:") {
        let pid: i32 = rest.parse().context("bad app id")?;
        let app = content
            .applications()
            .into_iter()
            .find(|a| a.process_id() == pid)
            .ok_or_else(|| anyhow!("That app is no longer running."))?;
        return Ok(SCContentFilter::create()
            .with_display(
                content
                    .displays()
                    .first()
                    .ok_or_else(|| anyhow!("No display available."))?,
            )
            .with_including_applications(&[&app], &[])
            .build());
    }
    if let Some(rest) = source_id.strip_prefix("window:") {
        let id: u32 = rest.parse().context("bad window id")?;
        let window = content
            .windows()
            .into_iter()
            .find(|w| w.window_id() == id)
            .ok_or_else(|| anyhow!("That window is no longer open."))?;
        return Ok(SCContentFilter::create().with_window(&window).build());
    }
    anyhow::bail!("Unknown source.")
}

struct AudioHandler {
    tx: SyncSender<crate::audio::PcmChunk>,
}

impl SCStreamOutputTrait for AudioHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if of_type != SCStreamOutputType::Audio {
            return;
        }
        if let Some(chunk) = extract_pcm(&sample) {
            send_chunk(&self.tx, chunk);
        }
    }
}

fn extract_pcm(sample: &CMSampleBuffer) -> Option<crate::audio::PcmChunk> {
    // ScreenCaptureKit delivers Float32 PCM. Try the crate's audio accessors.
    if let Some(list) = sample.audio_buffer_list() {
        return buffers_to_chunk(list);
    }
    None
}

fn buffers_to_chunk(list: screencapturekit::cm::AudioBufferList) -> Option<crate::audio::PcmChunk> {
    let mut interleaved: Vec<f32> = Vec::new();
    let mut channels: u16 = 0;
    for buffer in list.iter() {
        let floats: Vec<f32> = buffer
            .data()
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let buffer_channels = buffer.number_channels.max(1) as u16;
        if interleaved.is_empty() {
            interleaved = floats;
            channels = buffer_channels;
        } else if buffer_channels == 1 && channels == 1 {
            let mut merged = Vec::with_capacity(interleaved.len() * 2);
            for (a, b) in interleaved.iter().zip(floats.iter()) {
                merged.push(*a);
                merged.push(*b);
            }
            interleaved = merged;
            channels = 2;
        }
    }
    if interleaved.is_empty() {
        return None;
    }
    Some(pack_f32(&interleaved, 48_000, channels.max(1)))
}
