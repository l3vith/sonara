import { CSSProperties, ReactNode, useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

type Mode = 'idle' | 'hosting' | 'listening';
type Source = { id: string; kind: string; title: string; subtitle: string };
type Snapshot = { mode: Mode; roomCode?: string; sourceLabel?: string; listeners: string[]; level: number; path: string; status: string; error?: string; hostName?: string; lossless: string; streamQuality: string };
type Notice = { id: number; name: string; action: 'joined' | 'left' };
type NowPlaying = { title: string; artist?: string; album?: string; artwork?: string };
type Palette = { base: string; deep: string; glow: string };

const idle: Snapshot = { mode: 'idle', listeners: [], level: 0, path: 'idle', status: 'idle', lossless: '16-bit · 48 kHz stereo PCM', streamQuality: 'Auto' };
const tauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const call = async <T,>(cmd: string, args?: Record<string, unknown>) => tauri ? invoke<T>(cmd, args) : Promise.reject(new Error('Run Sonora as a desktop app to connect audio.'));

function Icon({ name, size = 20 }: { name: 'settings' | 'minus' | 'close' | 'wave' | 'copy' | 'arrow' | 'users' | 'audio' | 'refresh'; size?: number }) {
  const paths: Record<typeof name, ReactNode> = {
    settings: <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.83 2.83-.06-.06A1.7 1.7 0 0 0 15 19.4a1.7 1.7 0 0 0-1 .6 1.7 1.7 0 0 0-.4 1.1V21h-4v-.09A1.7 1.7 0 0 0 8.6 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06-2.83-2.83.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-.6-1 1.7 1.7 0 0 0-1.1-.4H3v-4h.09A1.7 1.7 0 0 0 4.6 8.6a1.7 1.7 0 0 0-.34-1.88l-.06-.06 2.83-2.83.06.06A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1-.6 1.7 1.7 0 0 0 .4-1.1V3h4v.09A1.7 1.7 0 0 0 15.4 4.6a1.7 1.7 0 0 0 1.88-.34l.06-.06 2.83 2.83-.06.06A1.7 1.7 0 0 0 19.4 9c.13.38.35.73.66 1 .3.27.7.4 1.1.4H21v4h-.09a1.7 1.7 0 0 0-1.51.6Z" /></>,
    minus: <path d="M5 12h14" />,
    close: <path d="m7 7 10 10M17 7 7 17" />,
    wave: <path d="M3 12h2l2.2-5.2L10 17l3-12 2.8 9H21" />,
    copy: <><rect x="8" y="8" width="11" height="11" rx="2" /><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" /></>,
    arrow: <path d="M5 12h14m-5-5 5 5-5 5" />,
    users: <><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75" /></>,
    audio: <><path d="M9 18V5l10-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="16" cy="16" r="3" /></>,
    refresh: <><path d="M20 6v5h-5" /><path d="M19 11a7 7 0 1 0 .1 5" /></>,
  };
  return <svg className="icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}

function hashPalette(text: string): Palette {
  let hash = 0;
  for (const char of text) hash = char.charCodeAt(0) + ((hash << 5) - hash);
  const hue = Math.abs(hash) % 360;
  return { base: `hsl(${hue} 42% 42%)`, deep: `hsl(${(hue + 18) % 360} 48% 12%)`, glow: `hsl(${(hue + 330) % 360} 58% 64%)` };
}

function useArtworkPalette(track?: NowPlaying) {
  const [palette, setPalette] = useState<Palette>(() => hashPalette('sonora'));
  useEffect(() => {
    const fallback = hashPalette(`${track?.title ?? 'sonora'}${track?.artist ?? ''}`);
    if (!track?.artwork) { setPalette(fallback); return; }
    const image = new Image();
    image.onload = () => {
      try {
        const canvas = document.createElement('canvas');
        canvas.width = 24; canvas.height = 24;
        const context = canvas.getContext('2d', { willReadFrequently: true });
        if (!context) { setPalette(fallback); return; }
        context.drawImage(image, 0, 0, 24, 24);
        const data = context.getImageData(0, 0, 24, 24).data;
        let r = 0, g = 0, b = 0, count = 0;
        for (let index = 0; index < data.length; index += 16) {
          if (data[index + 3] < 180) continue;
          r += data[index]; g += data[index + 1]; b += data[index + 2]; count += 1;
        }
        if (!count) { setPalette(fallback); return; }
        r = Math.round(r / count); g = Math.round(g / count); b = Math.round(b / count);
        setPalette({ base: `rgb(${r} ${g} ${b})`, deep: `rgb(${Math.round(r * .2)} ${Math.round(g * .2)} ${Math.round(b * .2)})`, glow: `rgb(${Math.min(255, Math.round(r * 1.35 + 32))} ${Math.min(255, Math.round(g * 1.35 + 32))} ${Math.min(255, Math.round(b * 1.35 + 32))})` });
      } catch { setPalette(fallback); }
    };
    image.onerror = () => setPalette(fallback);
    image.crossOrigin = 'anonymous';
    image.src = track.artwork;
  }, [track?.artwork, track?.artist, track?.title]);
  return palette;
}

function Artwork({ track, compact = false }: { track?: NowPlaying; compact?: boolean }) {
  const label = track?.album || track?.title || 'Sonora';
  return <div className={`artwork ${compact ? 'compact' : ''}`}>
    {track?.artwork ? <img src={track.artwork} alt={`${label} artwork`} /> : <div className="artwork-fallback"><Icon name="wave" size={compact ? 24 : 42} /><span>{label.slice(0, 1).toUpperCase()}</span></div>}
  </div>;
}

function WindowBar({ onSettings }: { onSettings: () => void }) {
  const windowAction = (action: 'minimize' | 'close') => { if (tauri) void getCurrentWindow()[action](); };
  const startDragging = (event: React.MouseEvent<HTMLElement>) => {
    if (!tauri || event.button !== 0 || (event.target as HTMLElement).closest('button')) return;
    event.preventDefault();
    void getCurrentWindow().startDragging();
  };
  return <header className="window-bar" onMouseDown={startDragging}>
    <div className="brand"><span className="brand-mark"><Icon name="wave" size={15} /></span><span>sonora</span></div>
    <div className="window-actions">
      <button className="round-control" aria-label="Open settings" title="Settings" onClick={onSettings}><Icon name="settings" size={16} /></button>
      <button className="round-control" aria-label="Minimize window" onClick={() => windowAction('minimize')}><Icon name="minus" size={16} /></button>
      <button className="round-control close-control" aria-label="Close window" onClick={() => windowAction('close')}><Icon name="close" size={16} /></button>
    </div>
  </header>;
}

function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(idle);
  const [nowPlaying, setNowPlaying] = useState<NowPlaying>();
  const [sources, setSources] = useState<Source[]>([]);
  const [name, setName] = useState('');
  const [roomCode, setRoomCode] = useState('');
  const [sourceId, setSourceId] = useState('');
  const [quality, setQuality] = useState('auto');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [volume, setVolume] = useState(1);
  const [notices, setNotices] = useState<Notice[]>([]);
  const [diagnosticsPath, setDiagnosticsPath] = useState<string | null>(null);
  const palette = useArtworkPalette(nowPlaying);
  const theme = { '--music-base': palette.base, '--music-deep': palette.deep, '--music-glow': palette.glow, '--artwork': nowPlaying?.artwork ? `url("${nowPlaying.artwork}")` : 'none' } as CSSProperties;

  useEffect(() => {
    if (!tauri) return;
    let active = true;
    const refresh = () => void call<NowPlaying | null>('current_now_playing').then(track => { if (active) setNowPlaying(track || undefined); }).catch(() => {});
    refresh();
    const timer = window.setInterval(refresh, 3_000);
    return () => { active = false; window.clearInterval(timer); };
  }, []);
  useEffect(() => {
    if (!tauri) return;
    let active = true;
    void call<Snapshot>('room_snapshot').then(value => { if (active) setSnapshot(value); }).catch(value => setError(String(value)));
    const unlisten = listen<Snapshot>('room-state', event => { if (active) setSnapshot(event.payload); });
    return () => { active = false; void unlisten.then(stop => stop()); };
  }, []);
  useEffect(() => {
    if (!tauri) return;
    let nextId = 0;
    const unlisten = listen<Omit<Notice, 'id'>>('listener-presence', event => {
      const notice = { ...event.payload, id: nextId++ };
      setNotices(current => [...current, notice].slice(-3));
      window.setTimeout(() => setNotices(current => current.filter(item => item.id !== notice.id)), 4_000);
    });
    return () => { void unlisten.then(stop => stop()); };
  }, []);
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === ',') { event.preventDefault(); open('settings-panel'); }
    };
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, []);

  const refreshSources = useCallback(async () => {
    try {
      const list = await call<Source[]>('list_sources');
      setSources(list); setSourceId(current => current || list[0]?.id || '');
    } catch (value) { setError(String(value)); }
  }, []);
  const selected = useMemo(() => sources.find(source => source.id === sourceId), [sources, sourceId]);
  const run = async (action: () => Promise<unknown>) => { setError(''); setBusy(true); try { await action(); } catch (value) { setError(String(value)); } finally { setBusy(false); } };
  const host = () => run(async () => {
    const sourceLabel = selected?.title || 'Audio source';
    const code = await call<string>('host_room', { displayName: name.trim() || 'Host', sourceId, sourceLabel, quality });
    setSnapshot(current => ({ ...current, mode: 'hosting', roomCode: code, sourceLabel, status: 'live', path: 'publishing' }));
  });
  const join = () => run(async () => { try { await call('join_room', { displayName: name.trim() || 'Listener', roomCode }); } catch (value) { setSnapshot(idle); throw value; } });
  const leave = () => run(async () => { await call('leave_room'); setSnapshot(idle); setNotices([]); });
  const open = (id: string) => (document.getElementById(id) as HTMLDialogElement | null)?.showModal();
  const close = (id: string) => (document.getElementById(id) as HTMLDialogElement | null)?.close();
  const toggleDiagnostics = () => run(async () => { const path = await call<string | null>('set_diagnostics_logging', { enabled: !diagnosticsPath }); setDiagnosticsPath(path); });

  return <main className="widget" style={theme}>
    <div className="artwork-backdrop" aria-hidden="true" />
    <div className="color-wash" aria-hidden="true" />
    <div className="widget-content">
      <WindowBar onSettings={() => { void call<string | null>('diagnostics_log_status').then(setDiagnosticsPath).catch(() => {}); open('settings-panel'); }} />
      {snapshot.mode === 'idle'
        ? <Home track={nowPlaying} roomCode={roomCode} setRoomCode={setRoomCode} busy={busy} error={error} onHost={() => { void refreshSources(); open('host-panel'); }} onJoin={join} />
        : <Room track={nowPlaying} snapshot={snapshot} error={error} notices={notices} volume={volume} onVolume={value => { setVolume(value); if (tauri) void call('set_volume', { volume: value }); }} onLeave={leave} busy={busy} />}
    </div>

    <dialog id="settings-panel" className="glass-dialog settings-dialog">
      <div className="dialog-head"><h2>Settings</h2><button className="round-control" aria-label="Close settings" onClick={() => close('settings-panel')}><Icon name="close" size={17} /></button></div>
      <label className="field-label" htmlFor="name">Display name</label>
      <input id="name" value={name} onChange={event => setName(event.target.value)} placeholder="Your name" maxLength={32} />
      <div className="settings-section"><span className="field-label">Diagnostics</span><p>{diagnosticsPath ? 'Connection metrics are being recorded.' : 'Record connection metrics for troubleshooting.'}</p><button className="text-button" onClick={toggleDiagnostics} disabled={busy}>{diagnosticsPath ? 'Stop logging' : 'Start logging'}</button>{diagnosticsPath && <small className="path-note">{diagnosticsPath}</small>}</div>
      <button className="button primary wide" onClick={() => close('settings-panel')}>Done</button>
    </dialog>

    <dialog id="host-panel" className="glass-dialog host-dialog">
      <div className="dialog-head"><div><h2>Choose a source</h2><p>What should the room hear?</p></div><button className="round-control" aria-label="Close source picker" onClick={() => close('host-panel')}><Icon name="close" size={17} /></button></div>
      <div className="source-list">{sources.length ? sources.map(source => <button key={source.id} className={`source-row ${source.id === sourceId ? 'selected' : ''}`} onClick={() => setSourceId(source.id)}><span className="source-glyph"><Icon name={source.kind === 'system' ? 'audio' : 'wave'} size={18} /></span><span className="source-copy"><strong>{source.title}</strong><small>{source.subtitle}</small></span><span className="radio" /></button>) : <p className="empty">No audio sources found. Check Screen Recording permission and refresh.</p>}</div>
      <fieldset className="quality-options"><legend>Quality</legend>{[['auto', 'Auto'], ['high', 'High'], ['balanced', 'Balanced'], ['saver', 'Saver']].map(([value, label]) => <label key={value}><input type="radio" name="quality" value={value} checked={quality === value} onChange={event => setQuality(event.target.value)} /><span>{label}</span></label>)}</fieldset>
      <div className="dialog-actions"><button className="round-control" aria-label="Refresh sources" title="Refresh sources" onClick={refreshSources}><Icon name="refresh" size={17} /></button><button className="button primary" disabled={!sourceId || busy} onClick={() => { close('host-panel'); host(); }}>Start room <Icon name="arrow" size={16} /></button></div>
    </dialog>
  </main>;
}

function Home({ track, roomCode, setRoomCode, busy, error, onHost, onJoin }: { track?: NowPlaying; roomCode: string; setRoomCode: (value: string) => void; busy: boolean; error: string; onHost: () => void; onJoin: () => void }) {
  return <section className="home-view">
    <div className="track-hero">
      <Artwork track={track} />
      <div className="track-copy"><h1>{track?.title || 'Hear it together.'}</h1><p>{track?.artist || 'Private, lossless listening rooms'}</p>{track?.album && <span>{track.album}</span>}</div>
    </div>
    <div className="action-dock">
      <div className="dock-heading"><span className="live-orb"><Icon name="wave" size={14} /></span><div><strong>Start a listening room</strong><small>{track ? 'Share what is playing now' : 'Choose any audio source'}</small></div><button className="round-control light" aria-label="Start a room" onClick={onHost} disabled={busy}><Icon name="arrow" size={18} /></button></div>
      <div className="join-control"><input aria-label="Room code" value={roomCode} onChange={event => setRoomCode(event.target.value.toUpperCase())} placeholder="SNR-XXXX-XXXX" /><button onClick={onJoin} disabled={busy || roomCode.length < 8}>Join</button></div>
      {error && <p className="error" role="alert">{error}</p>}
    </div>
  </section>;
}

function Room({ track, snapshot, error, notices, volume, onVolume, onLeave, busy }: { track?: NowPlaying; snapshot: Snapshot; error: string; notices: Notice[]; volume: number; onVolume: (value: number) => void; onLeave: () => void; busy: boolean }) {
  const hosting = snapshot.mode === 'hosting';
  const [copied, setCopied] = useState(false);
  const copyCode = async () => { if (!snapshot.roomCode) return; await navigator.clipboard.writeText(snapshot.roomCode); setCopied(true); window.setTimeout(() => setCopied(false), 1_600); };
  return <section className="room-view">
    <div className="room-status"><span className={`state-dot ${snapshot.status}`} /><span>{snapshot.status === 'live' ? (hosting ? 'Broadcasting live' : 'Listening live') : snapshot.status}</span><span className="path-label">{snapshot.path === 'connected' ? 'Direct' : snapshot.path}</span></div>
    <div className="playing-row"><Artwork track={track} compact /><div><h1>{track?.title || snapshot.sourceLabel || 'Connecting…'}</h1><p>{track?.artist || (hosting ? 'Your listening room' : `Hosted by ${snapshot.hostName || 'the room host'}`)}</p></div></div>
    <div className="signal-card"><div className="signal-line"><span>Lossless stream</span><span>{snapshot.streamQuality}</span></div><div className="meter"><span style={{ '--meter-level': Math.min(1, Math.max(.04, snapshot.level * 1.8)) } as CSSProperties} /></div><small>{snapshot.lossless}</small></div>
    <div className="room-dock">
      {hosting ? <><button className="room-code" onClick={copyCode} title="Copy room code"><span>Room code</span><strong>{snapshot.roomCode || '—'}</strong><Icon name="copy" size={17} /><em>{copied ? 'Copied' : ''}</em></button><div className="listener-line"><Icon name="users" size={17} /><span>{snapshot.listeners.length ? `${snapshot.listeners.length} ${snapshot.listeners.length === 1 ? 'listener' : 'listeners'}` : 'Waiting for listeners'}</span></div></> : <div className="volume-control"><label htmlFor="volume">Volume <span>{Math.round(volume * 100)}%</span></label><input id="volume" type="range" min="0" max="2" step="0.01" value={volume} onChange={event => onVolume(Number(event.target.value))} /></div>}
      <button className="leave-button" onClick={onLeave} disabled={busy}>Leave room</button>
      {(error || snapshot.error) && <p className="error" role="alert">{error || snapshot.error}</p>}
    </div>
    {notices.length > 0 && <div className="presence-notices" role="status">{notices.map(notice => <p key={notice.id}>{notice.name} {notice.action === 'joined' ? 'joined' : 'left'}</p>)}</div>}
  </section>;
}

export default App;
