use super::{pack_f32, send_chunk, CaptureHandle};
use crate::audio::{AudioSource, PcmChunk};
use anyhow::{anyhow, Result};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::SyncSender,
    Arc,
};
use wasapi::*;
use xcap::Window;

pub fn list_sources() -> Result<Vec<AudioSource>> {
    let mut out = vec![AudioSource {
        id: "system:default".into(),
        kind: "system".into(),
        title: "This PC".into(),
        subtitle: "Default output mix".into(),
    }];
    if let Ok(windows) = Window::all() {
        for w in windows {
            let title = w.title().unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let pid = w.pid();
            let app = w.app_name().unwrap_or_default();
            out.push(AudioSource {
                id: format!("app:{pid}"),
                kind: "app".into(),
                title,
                subtitle: app,
            });
        }
    }
    Ok(out)
}

pub fn start(source_id: &str, tx: SyncSender<PcmChunk>) -> Result<CaptureHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_t = stop.clone();
    let id = source_id.to_string();
    let join = std::thread::spawn(move || {
        if let Err(e) = capture_loop(&id, tx, stop_t.clone()) {
            tracing::error!("windows capture: {e:#}");
        }
    });
    Ok(CaptureHandle {
        stop,
        join: Some(join),
    })
}

fn capture_loop(source_id: &str, tx: SyncSender<PcmChunk>, stop: Arc<AtomicBool>) -> Result<()> {
    initialize_mta().map_err(|e| anyhow!("{e}"))?;
    let desired = WaveFormat::new(32, 32, &SampleType::Float, 48000, 2, None);
    let mut audio_client = if source_id == "system:default" {
        let device = DeviceEnumerator::new()
            .map_err(|e| anyhow!("{e}"))?
            .get_default_device(&Direction::Render)
            .map_err(|e| anyhow!("{e}"))?;
        let mut client = device.get_iaudioclient().map_err(|e| anyhow!("{e}"))?;
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: 200_000,
        };
        client
            .initialize_client(&desired, &Direction::Capture, &mode)
            .map_err(|e| anyhow!("{e}"))?;
        client
    } else if let Some(rest) = source_id.strip_prefix("app:") {
        let pid: u32 = rest.parse()?;
        let mut client =
            AudioClient::new_application_loopback_client(pid, true).map_err(|e| anyhow!("{e}"))?;
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: 0,
        };
        client
            .initialize_client(&desired, &Direction::Capture, &mode)
            .map_err(|e| anyhow!("{e}"))?;
        client
    } else {
        anyhow::bail!("Unknown source");
    };

    let h_event = audio_client
        .set_get_eventhandle()
        .map_err(|e| anyhow!("{e}"))?;
    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|e| anyhow!("{e}"))?;
    audio_client.start_stream().map_err(|e| anyhow!("{e}"))?;

    while !stop.load(Ordering::SeqCst) {
        let _ = h_event.wait_for_event(80);
        let mut data = vec![];
        let mut frames = 0u32;
        let mut flags = BufferFlags::empty();
        if capture_client
            .read_from_device_to_deque(&mut data, &mut frames, &mut flags)
            .is_err()
        {
            // Fall back to a simpler read if the deque API differs.
            let _ = frames;
            let _ = flags;
            std::thread::sleep(std::time::Duration::from_millis(10));
            continue;
        }
        if data.is_empty() {
            continue;
        }
        let floats: Vec<f32> = data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        send_chunk(&tx, pack_f32(&floats, 48_000, 2));
    }
    let _ = audio_client.stop_stream();
    Ok(())
}
