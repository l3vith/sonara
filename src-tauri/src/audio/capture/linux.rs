use super::{pack_f32, send_chunk, CaptureHandle};
use crate::audio::{AudioSource, PcmChunk};
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::SyncSender,
    Arc,
};

pub fn list_sources() -> Result<Vec<AudioSource>> {
    let host = cpal::default_host();
    let mut out = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for (i, d) in devices.enumerate() {
            let name = d.name().unwrap_or_else(|_| format!("Input {i}"));
            let is_monitor = name.to_lowercase().contains("monitor")
                || name.to_lowercase().contains("loopback")
                || name.to_lowercase().contains("stereo mix");
            out.push(AudioSource {
                id: format!("in:{i}:{name}"),
                kind: if is_monitor { "system" } else { "app" }.into(),
                title: name.clone(),
                subtitle: if is_monitor {
                    "Output monitor".into()
                } else {
                    "Capture device".into()
                },
            });
        }
    }
    if out.is_empty() {
        out.push(AudioSource {
            id: "in:default".into(),
            kind: "system".into(),
            title: "Default input".into(),
            subtitle: "Use a monitor / loopback device for app audio".into(),
        });
    }
    Ok(out)
}

pub fn start(source_id: &str, tx: SyncSender<PcmChunk>) -> Result<CaptureHandle> {
    let host = cpal::default_host();
    let device = pick_device(&host, source_id)?;
    let config = device.default_input_config().map_err(|e| anyhow!("{e}"))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_t = stop.clone();

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(&device, &config.into(), tx, sample_rate, channels)?
        }
        cpal::SampleFormat::I16 => {
            let tx2 = tx;
            let stream = device.build_input_stream(
                &config.into(),
                move |data: &[i16], _| {
                    send_chunk(
                        &tx2,
                        PcmChunk {
                            samples: data.to_vec(),
                            sample_rate,
                            channels,
                        },
                    );
                },
                |e| tracing::error!("cpal: {e}"),
                None,
            )?;
            stream
        }
        other => anyhow::bail!("Unsupported sample format {other}"),
    };
    stream.play()?;

    let join = std::thread::spawn(move || {
        let _stream = stream;
        while !stop_t.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    Ok(CaptureHandle {
        stop,
        join: Some(join),
    })
}

fn pick_device(host: &cpal::Host, source_id: &str) -> Result<cpal::Device> {
    if source_id == "in:default" {
        return host
            .default_input_device()
            .ok_or_else(|| anyhow!("No input device."));
    }
    if let Some(rest) = source_id.strip_prefix("in:") {
        let name = rest.splitn(2, ':').nth(1).unwrap_or(rest);
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if d.name().ok().as_deref() == Some(name) {
                    return Ok(d);
                }
            }
        }
    }
    host.default_input_device()
        .ok_or_else(|| anyhow!("No input device."))
}

fn build_stream<T: cpal::Sample>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    tx: SyncSender<PcmChunk>,
    sample_rate: u32,
    channels: u16,
) -> Result<cpal::Stream> {
    let stream = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            send_chunk(&tx, pack_f32(data, sample_rate, channels));
        },
        |e| tracing::error!("cpal: {e}"),
        None,
    )?;
    let _ = T::EQUILIBRIUM;
    Ok(stream)
}
