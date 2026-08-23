use crate::audio::i16_to_f32;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    mpsc, Arc,
};

const MAX_QUEUED_SAMPLES: usize = 48_000 * 2 * 2;

pub struct Playback {
    stop: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    sender: mpsc::SyncSender<Vec<f32>>,
}

impl Playback {
    pub fn start() -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Vec<f32>>(16);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        let stop = Arc::new(AtomicBool::new(false));
        let volume = Arc::new(AtomicU32::new(f32::to_bits(1.0)));
        let stop_thread = stop.clone();
        let volume_thread = volume.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let host = cpal::default_host();
                let device = host
                    .default_output_device()
                    .ok_or_else(|| "No output device.".to_string())?;
                let config = device.default_output_config().map_err(|e| e.to_string())?;
                let out_ch = config.channels() as usize;
                let queue = Arc::new(Mutex::new(VecDeque::<f32>::new()));
                let queue_in = queue.clone();
                let volume_in = volume_thread.clone();
                let err_fn = |e| tracing::error!("playback: {e}");
                let stream = match config.sample_format() {
                    cpal::SampleFormat::F32 => device.build_output_stream(
                        &config.into(),
                        move |out: &mut [f32], _| mix_out(out, out_ch, &queue, &volume_in),
                        err_fn,
                        None,
                    ),
                    cpal::SampleFormat::I16 => device.build_output_stream(
                        &config.into(),
                        move |out: &mut [i16], _| {
                            let mut f = vec![0.0; out.len()];
                            mix_out(&mut f, out_ch, &queue, &volume_in);
                            for (o, s) in out.iter_mut().zip(f) {
                                *o = (s * i16::MAX as f32) as i16;
                            }
                        },
                        err_fn,
                        None,
                    ),
                    other => return Err(format!("Unsupported output format {other}")),
                }
                .map_err(|e| e.to_string())?;
                stream.play().map_err(|e| e.to_string())?;
                ready_tx.send(Ok(())).map_err(|e| e.to_string())?;
                while !stop_thread.load(Ordering::SeqCst) {
                    match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
                        Ok(samples) => {
                            let mut queued = queue_in.lock();
                            queued.extend(samples);
                            let overflow = queued.len().saturating_sub(MAX_QUEUED_SAMPLES);
                            if overflow > 0 {
                                queued.drain(..overflow);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                let _ = ready_tx.send(Err(error));
            }
        });
        match ready_rx.recv().map_err(|e| anyhow!(e.to_string()))? {
            Ok(()) => Ok(Self {
                stop,
                volume,
                sender,
            }),
            Err(e) => Err(anyhow!(e)),
        }
    }

    pub fn set_volume(&self, v: f32) {
        self.volume
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
    pub fn push_i16(&self, pcm: &[i16]) {
        let _ = self.sender.try_send(i16_to_f32(pcm));
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn mix_out(out: &mut [f32], out_ch: usize, queue: &Mutex<VecDeque<f32>>, volume: &AtomicU32) {
    let gain = f32::from_bits(volume.load(Ordering::Relaxed));
    let mut q = queue.lock();
    for frame in out.chunks_mut(out_ch.max(1)) {
        let l = q.pop_front().unwrap_or(0.0) * gain;
        let r = q.pop_front().unwrap_or(l) * gain;
        if frame.len() == 1 {
            frame[0] = (l + r) * 0.5;
        } else {
            frame[0] = l;
            if frame.len() > 1 {
                frame[1] = r;
            }
            for sample in frame.iter_mut().skip(2) {
                *sample = 0.0;
            }
        }
    }
}
