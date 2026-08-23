use crate::audio::{capture, playback, rms_level, AudioSource, PcmChunk};
use crate::identity;
use crate::protocol::{self, Ctrl, ALPN};
use anyhow::{Context, Result};
use bytes::Bytes;
use iroh::{endpoint::presets, Endpoint, EndpointAddr};
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoomSnapshot {
    pub mode: String,
    pub room_code: Option<String>,
    pub source_label: Option<String>,
    pub listeners: Vec<String>,
    pub level: f32,
    pub path: String,
    pub status: String,
    pub error: Option<String>,
    pub host_name: Option<String>,
    pub lossless: String,
}

impl RoomSnapshot {
    fn idle() -> Self {
        Self {
            mode: "idle".into(),
            lossless: "16-bit · 48 kHz stereo PCM".into(),
            path: "idle".into(),
            status: "idle".into(),
            ..Default::default()
        }
    }
}

pub struct AppState {
    pub snapshot: Mutex<RoomSnapshot>,
    inner: tokio::sync::Mutex<Option<LiveSession>>,
}

struct LiveSession {
    shutdown: tokio::sync::watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    capture: Option<capture::CaptureHandle>,
    playback: Option<Arc<playback::Playback>>,
    endpoint: Option<Endpoint>,
    volume: Arc<parking_lot::Mutex<f32>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            snapshot: Mutex::new(RoomSnapshot::idle()),
            inner: tokio::sync::Mutex::new(None),
        }
    }
}

fn emit(app: &AppHandle, snap: &RoomSnapshot) {
    let _ = app.emit("room-state", snap);
}

pub fn current_snapshot(state: &AppState) -> RoomSnapshot {
    state.snapshot.lock().clone()
}

pub fn list_sources() -> Result<Vec<AudioSource>, String> {
    capture::list_sources().map_err(|e| e.to_string())
}

pub async fn host_room(
    app: AppHandle,
    state: Arc<AppState>,
    display_name: String,
    source_id: String,
    source_label: String,
) -> Result<String, String> {
    stop_session(&state).await;
    let code = identity::generate_room_code();
    let secret = identity::room_secret(&code)?;

    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| format!("Could not start networking: {e}"))?;
    let _ = tokio::time::timeout(Duration::from_secs(10), endpoint.online()).await;

    let (pcm_tx, pcm_rx) = mpsc::sync_channel::<PcmChunk>(8);
    let cap = capture::start(&source_id, pcm_tx).map_err(|e| e.to_string())?;

    let (audio_tx, _) = broadcast::channel::<Bytes>(64);
    let (peers_tx, _) = broadcast::channel::<Vec<String>>(16);
    let (shutdown, _) = tokio::sync::watch::channel(false);
    let listeners = Arc::new(Mutex::new(Vec::<String>::new()));

    {
        let mut snap = state.snapshot.lock();
        *snap = RoomSnapshot {
            mode: "hosting".into(),
            room_code: Some(code.clone()),
            source_label: Some(source_label.clone()),
            listeners: vec![],
            level: 0.0,
            path: "publishing".into(),
            status: "live".into(),
            error: None,
            host_name: Some(display_name.clone()),
            lossless: "16-bit · 48 kHz stereo PCM".into(),
        };
        emit(&app, &snap);
    }

    let snap_state = state.clone();
    let app2 = app.clone();
    let audio_tx2 = audio_tx.clone();
    let sd = shutdown.subscribe();
    let pump = tokio::task::spawn_blocking(move || {
        while !*sd.borrow() {
            match pcm_rx.recv_timeout(Duration::from_millis(80)) {
                Ok(chunk) => {
                    let lvl = rms_level(&chunk.samples);
                    {
                        let mut s = snap_state.snapshot.lock();
                        s.level = lvl;
                        s.path = "live".into();
                    }
                    let _ = app2.emit("room-state", snap_state.snapshot.lock().clone());
                    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let framed = protocol::encode_audio_frame(seq, &chunk.samples);
                    let _ = audio_tx2.send(Bytes::from(framed));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    let ep_accept = endpoint.clone();
    let mut sd2 = shutdown.subscribe();
    let audio_tx3 = audio_tx.clone();
    let peers_tx3 = peers_tx.clone();
    let listeners2 = listeners.clone();
    let host_name = display_name.clone();
    let source_label2 = source_label.clone();
    let snap_state2 = state.clone();
    let app3 = app.clone();
    let accept = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = sd2.changed() => {
                    if *sd2.borrow() { break; }
                }
                incoming = ep_accept.accept() => {
                    let Some(incoming) = incoming else { break; };
                    let Ok(conn) = incoming.await else { continue; };
                    let audio_rx = audio_tx3.subscribe();
                    let peers_rx = peers_tx3.subscribe();
                    let peers_tx = peers_tx3.clone();
                    let listeners = listeners2.clone();
                    let host_name = host_name.clone();
                    let source_label = source_label2.clone();
                    let snap = snap_state2.clone();
                    let app = app3.clone();
                    tokio::spawn(async move {
                        if let Err(e) = serve_listener(conn, audio_rx, peers_rx, peers_tx, listeners, host_name, source_label, snap, app).await {
                            tracing::warn!("listener session: {e:#}");
                        }
                    });
                }
            }
        }
    });

    let mut guard = state.inner.lock().await;
    *guard = Some(LiveSession {
        shutdown,
        tasks: vec![pump, accept],
        capture: Some(cap),
        playback: None,
        endpoint: Some(endpoint),
        volume: Arc::new(parking_lot::Mutex::new(1.0)),
    });
    Ok(code)
}

async fn serve_listener(
    conn: iroh::endpoint::Connection,
    mut audio_rx: broadcast::Receiver<Bytes>,
    mut peers_rx: broadcast::Receiver<Vec<String>>,
    peers_tx: broadcast::Sender<Vec<String>>,
    listeners: Arc<Mutex<Vec<String>>>,
    host_name: String,
    source_label: String,
    state: Arc<AppState>,
    app: AppHandle,
) -> Result<()> {
    let (mut send, mut recv) = conn.accept_bi().await.context("accept control")?;
    let hello = read_ctrl(&mut recv).await?;
    let name = match hello {
        Ctrl::Hello { name, .. } => name,
        _ => "Listener".into(),
    };
    let peer_names = {
        let mut current = listeners.lock();
        current.push(name.clone());
        current.clone()
    };
    {
        let mut s = state.snapshot.lock();
        s.listeners = peer_names.clone();
        emit(&app, &s);
    }
    let _ = peers_tx.send(peer_names.clone());
    let _registration = ListenerRegistration {
        name: name.clone(),
        listeners,
        peers_tx: peers_tx.clone(),
        state,
        app,
    };
    write_ctrl(
        &mut send,
        &Ctrl::Room {
            host: host_name,
            source: source_label,
            rate: protocol::SAMPLE_RATE,
            channels: protocol::CHANNELS,
        },
    )
    .await?;
    write_ctrl(&mut send, &Ctrl::Peers { names: peer_names }).await?;

    let mut audio_send = conn.open_uni().await.context("open audio")?;
    loop {
        tokio::select! {
            audio = audio_rx.recv() => match audio {
                Ok(frame) => {
                    audio_send
                        .write_all(&protocol::write_frame(&frame))
                        .await
                        .context("audio write")?;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            peers = peers_rx.recv() => match peers {
                Ok(names) => write_ctrl(&mut send, &Ctrl::Peers { names }).await?,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    Ok(())
}

struct ListenerRegistration {
    name: String,
    listeners: Arc<Mutex<Vec<String>>>,
    peers_tx: broadcast::Sender<Vec<String>>,
    state: Arc<AppState>,
    app: AppHandle,
}

impl Drop for ListenerRegistration {
    fn drop(&mut self) {
        let peer_names = {
            let mut current = self.listeners.lock();
            if let Some(position) = current.iter().rposition(|name| name == &self.name) {
                current.remove(position);
            }
            current.clone()
        };
        let _ = self.peers_tx.send(peer_names.clone());
        let mut snapshot = self.state.snapshot.lock();
        snapshot.listeners = peer_names;
        emit(&self.app, &snapshot);
    }
}

pub async fn join_room(
    app: AppHandle,
    state: Arc<AppState>,
    display_name: String,
    room_code: String,
) -> Result<(), String> {
    stop_session(&state).await;
    let code = identity::parse_room_code(&room_code)?;
    let host = identity::host_id(&code)?;
    let play = playback::Playback::start().map_err(|e| e.to_string())?;
    let volume = Arc::new(parking_lot::Mutex::new(1.0));

    let endpoint = Endpoint::builder(presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| format!("Could not start networking: {e}"))?;
    let _ = tokio::time::timeout(Duration::from_secs(10), endpoint.online()).await;

    {
        let mut snap = state.snapshot.lock();
        *snap = RoomSnapshot {
            mode: "listening".into(),
            room_code: Some(code.clone()),
            source_label: None,
            listeners: vec![],
            level: 0.0,
            path: "connecting".into(),
            status: "connecting".into(),
            error: None,
            host_name: None,
            lossless: "16-bit · 48 kHz stereo PCM".into(),
        };
        emit(&app, &snap);
    }

    let conn = endpoint
        .connect(EndpointAddr::new(host), ALPN)
        .await
        .map_err(|e| {
            format!("Could not reach the host. Confirm they are live and the code is right. ({e})")
        })?;

    let (shutdown, _) = tokio::sync::watch::channel(false);
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("Could not open a stream: {e}"))?;
    write_ctrl(
        &mut send,
        &Ctrl::Hello {
            name: display_name,
            role: "listener".into(),
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let play = Arc::new(play);
    let play2 = play.clone();
    let state2 = state.clone();
    let app2 = app.clone();
    let mut sd = shutdown.subscribe();
    let endpoint2 = endpoint.clone();
    let remote = conn.remote_id();

    let reader = tokio::spawn(async move {
        if let Ok(ctrl) = read_ctrl(&mut recv).await {
            if let Ctrl::Room { host, source, .. } = ctrl {
                let mut s = state2.snapshot.lock();
                s.host_name = Some(host);
                s.source_label = Some(source);
                s.status = "live".into();
                s.path = connection_path(&endpoint2, remote);
                emit(&app2, &s);
            }
        }
        loop {
            tokio::select! {
                _ = sd.changed() => { if *sd.borrow() { break; } }
                ctrl = read_ctrl(&mut recv) => {
                    match ctrl {
                        Ok(Ctrl::Peers { names }) => {
                            let mut s = state2.snapshot.lock();
                            s.listeners = names;
                            emit(&app2, &s);
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            }
        }
    });

    let play_audio = play2.clone();
    let state3 = state.clone();
    let app3 = app.clone();
    let mut sd2 = shutdown.subscribe();
    let audio_task = tokio::spawn(async move {
        let mut audio = match conn.accept_uni().await {
            Ok(s) => s,
            Err(e) => {
                let mut s = state3.snapshot.lock();
                s.error = Some(format!("Audio stream failed: {e}"));
                s.status = "error".into();
                emit(&app3, &s);
                return;
            }
        };
        loop {
            tokio::select! {
                _ = sd2.changed() => { if *sd2.borrow() { break; } }
                frame = read_len_frame(&mut audio) => {
                    match frame {
                        Ok(buf) => {
                            if let Some((_seq, pcm)) = protocol::decode_audio_frame(&buf) {
                                let lvl = rms_level(&pcm);
                                {
                                    let mut s = state3.snapshot.lock();
                                    s.level = lvl;
                                    s.status = "live".into();
                                }
                                let _ = app3.emit("room-state", state3.snapshot.lock().clone());
                                play_audio.push_i16(&pcm);
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    let mut guard = state.inner.lock().await;
    *guard = Some(LiveSession {
        shutdown,
        tasks: vec![reader, audio_task],
        capture: None,
        playback: Some(play),
        endpoint: Some(endpoint),
        volume,
    });
    Ok(())
}

fn connection_path(endpoint: &Endpoint, id: iroh::EndpointId) -> String {
    // Best-effort; API may vary slightly across iroh patch versions.
    let _ = (endpoint, id);
    "connected".into()
}

pub async fn leave(state: &AppState) {
    stop_session(state).await;
    let mut s = state.snapshot.lock();
    *s = RoomSnapshot::idle();
}

pub fn set_volume(state: &AppState, v: f32) {
    if let Ok(guard) = state.inner.try_lock() {
        if let Some(sess) = guard.as_ref() {
            *sess.volume.lock() = v;
            if let Some(p) = sess.playback.as_ref() {
                p.set_volume(v);
            }
        }
    }
}

async fn stop_session(state: &AppState) {
    let mut guard = state.inner.lock().await;
    if let Some(mut sess) = guard.take() {
        let _ = sess.shutdown.send(true);
        if let Some(c) = sess.capture.take() {
            c.stop();
        }
        if let Some(p) = sess.playback.take() {
            p.stop();
        }
        for t in sess.tasks {
            t.abort();
        }
        if let Some(ep) = sess.endpoint.take() {
            ep.close().await;
        }
    }
}

async fn write_ctrl<S: AsyncWriteExt + Unpin>(send: &mut S, msg: &Ctrl) -> Result<()> {
    let bytes = serde_json::to_vec(msg)?;
    send.write_all(&protocol::write_frame(&bytes)).await?;
    Ok(())
}

async fn read_ctrl<S: AsyncReadExt + Unpin>(recv: &mut S) -> Result<Ctrl> {
    let buf = read_len_frame(recv).await?;
    Ok(serde_json::from_slice(&buf)?)
}

async fn read_len_frame<S: AsyncReadExt + Unpin>(recv: &mut S) -> Result<Vec<u8>> {
    let mut lenb = [0u8; 4];
    recv.read_exact(&mut lenb).await?;
    let len = u32::from_le_bytes(lenb) as usize;
    if len == 0 || len > 2_000_000 {
        anyhow::bail!("bad frame");
    }
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await?;
    Ok(buf)
}
