use crate::audio::i16_to_f32;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    mpsc, Arc,
};

const JITTER_BUFFER_MS: usize = 120;
const MAX_BUFFER_MS: usize = 2_000;

pub struct Playback {
    stop: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    sender: mpsc::SyncSender<Vec<f32>>,
    queue: Arc<Mutex<VecDeque<f32>>>,
    underruns: Arc<AtomicU64>,
    output_sample_rate: Arc<AtomicU32>,
}

impl Playback {
    pub fn start() -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Vec<f32>>(64);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        let stop = Arc::new(AtomicBool::new(false));
        let volume = Arc::new(AtomicU32::new(f32::to_bits(1.0)));
        let queue = Arc::new(Mutex::new(VecDeque::<f32>::new()));
        let underruns = Arc::new(AtomicU64::new(0));
        let output_sample_rate = Arc::new(AtomicU32::new(48_000));
        let primed = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let volume_thread = volume.clone();
        let queue_thread = queue.clone();
        let underruns_thread = underruns.clone();
        let output_sample_rate_thread = output_sample_rate.clone();
        let primed_thread = primed.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let host = cpal::default_host();
                let device = host
                    .default_output_device()
                    .ok_or_else(|| "No output device.".to_string())?;
                let config = device.default_output_config().map_err(|e| e.to_string())?;
                let device_sample_rate = config.sample_rate().0 as usize;
                output_sample_rate_thread.store(device_sample_rate as u32, Ordering::Relaxed);
                let jitter_buffer_samples = (device_sample_rate * 2 * JITTER_BUFFER_MS / 1_000).max(2);
                let max_queued_samples = (device_sample_rate * 2 * MAX_BUFFER_MS / 1_000).max(jitter_buffer_samples);
                let out_ch = config.channels() as usize;
                let queue = queue_thread;
                let queue_in = queue.clone();
                let volume_in = volume_thread.clone();
                let underruns_in = underruns_thread.clone();
                let primed_in = primed_thread.clone();
                let err_fn = |e| tracing::error!("playback: {e}");
                let stream = match config.sample_format() {
                    cpal::SampleFormat::F32 => device.build_output_stream(
                        &config.into(),
                        move |out: &mut [f32], _| mix_out(out, out_ch, &queue, &volume_in, &underruns_in, &primed_in, jitter_buffer_samples),
                        err_fn,
                        None,
                    ),
                    cpal::SampleFormat::I16 => device.build_output_stream(
                        &config.into(),
                        move |out: &mut [i16], _| {
                            let mut f = vec![0.0; out.len()];
                            mix_out(&mut f, out_ch, &queue, &volume_in, &underruns_in, &primed_in, jitter_buffer_samples);
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
                            let overflow = queued.len().saturating_sub(max_queued_samples);
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
                queue,
                underruns,
                output_sample_rate,
            }),
            Err(e) => Err(anyhow!(e)),
        }
    }

    pub fn set_volume(&self, v: f32) {
        self.volume
            .store(v.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }
    pub fn push_i16(&self, pcm: &[i16], sample_rate: u32) {
        let output_sample_rate = self.output_sample_rate.load(Ordering::Relaxed);
        let pcm = crate::audio::to_stream_format(
            &crate::audio::PcmChunk { samples: pcm.to_vec(), sample_rate, channels: 2 },
            output_sample_rate,
            2,
        );
        let _ = self.sender.try_send(i16_to_f32(&pcm));
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
    pub fn buffered_ms(&self) -> u64 {
        let samples_per_second = self.output_sample_rate.load(Ordering::Relaxed).max(1) as u64 * 2;
        (self.queue.lock().len() as u64 * 1_000) / samples_per_second
    }
    pub fn underruns(&self) -> u64 { self.underruns.load(Ordering::Relaxed) }
}

fn mix_out(
    out: &mut [f32],
    out_ch: usize,
    queue: &Mutex<VecDeque<f32>>,
    volume: &AtomicU32,
    underruns: &AtomicU64,
    primed: &AtomicBool,
    jitter_buffer_samples: usize,
) {
    let gain = f32::from_bits(volume.load(Ordering::Relaxed));
    out.fill(0.0);
    let mut q = queue.lock();
    if !primed.load(Ordering::Relaxed) {
        if q.len() < jitter_buffer_samples {
            out.fill(0.0);
            return;
        }
        primed.store(true, Ordering::Relaxed);
    }
    for frame in out.chunks_mut(out_ch.max(1)) {
        if q.len() < 2 {
            underruns.fetch_add(1, Ordering::Relaxed);
            primed.store(false, Ordering::Relaxed);
            return;
        }
        let l = q.pop_front().unwrap_or_default() * gain;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_jitter_buffer_before_playing() {
        let queue = Mutex::new(VecDeque::from(vec![0.25, -0.25, 0.5, -0.5]));
        let volume = AtomicU32::new(1.0f32.to_bits());
        let underruns = AtomicU64::new(0);
        let primed = AtomicBool::new(false);
        let mut output = [1.0; 4];

        mix_out(&mut output, 2, &queue, &volume, &underruns, &primed, 6);

        assert_eq!(output, [0.0; 4]);
        assert_eq!(queue.lock().len(), 4);
        assert!(!primed.load(Ordering::Relaxed));
        assert_eq!(underruns.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn rebuffering_counts_one_underrun_per_dropout() {
        let queue = Mutex::new(VecDeque::from(vec![0.25, -0.25, 0.5, -0.5]));
        let volume = AtomicU32::new(1.0f32.to_bits());
        let underruns = AtomicU64::new(0);
        let primed = AtomicBool::new(true);
        let mut output = [1.0; 8];

        mix_out(&mut output, 2, &queue, &volume, &underruns, &primed, 4);

        assert_eq!(&output[..4], &[0.25, -0.25, 0.5, -0.5]);
        assert_eq!(&output[4..], &[0.0; 4]);
        assert_eq!(underruns.load(Ordering::Relaxed), 1);
        assert!(!primed.load(Ordering::Relaxed));
    }
}
