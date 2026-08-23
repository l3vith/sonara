import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type Mode = 'idle' | 'hosting' | 'listening';
type Source = { id: string; kind: string; title: string; subtitle: string };
type Snapshot = { mode: Mode; roomCode?: string; sourceLabel?: string; listeners: string[]; level: number; path: string; status: string; error?: string; hostName?: string; lossless: string };
const idle: Snapshot = { mode: 'idle', listeners: [], level: 0, path: 'idle', status: 'idle', lossless: '16-bit · 48 kHz stereo PCM' };
const tauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const call = async <T,>(cmd: string, args?: Record<string, unknown>) => tauri ? invoke<T>(cmd, args) : Promise.reject(new Error('Run Sonora as a desktop app to connect audio.'));

function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(idle);
  const [sources, setSources] = useState<Source[]>([]);
  const [name, setName] = useState('');
  const [roomCode, setRoomCode] = useState('');
  const [sourceId, setSourceId] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [volume, setVolume] = useState(1);

  useEffect(() => {
    if (!tauri) return;

    let active = true;
    const restoreSnapshot = async () => {
      try {
        const restored = await call<Snapshot>('room_snapshot');
        if (active) setSnapshot(restored);
      } catch (e) {
        if (active) setError(e instanceof Error ? e.message : String(e));
      }
    };
    void restoreSnapshot();

    const unlisten = listen<Snapshot>('room-state', e => {
      if (active) setSnapshot(e.payload);
    });
    return () => {
      active = false;
      void unlisten.then(stop => stop());
    };
  }, []);
  const refreshSources = useCallback(async () => { try { const list = await call<Source[]>('list_sources'); setSources(list); setSourceId(current => current || list[0]?.id || ''); } catch (e) { setError(e instanceof Error ? e.message : String(e)); } }, []);
  useEffect(() => { if (snapshot.mode === 'hosting') void refreshSources(); }, [snapshot.mode, refreshSources]);
  const selected = useMemo(() => sources.find(s => s.id === sourceId), [sources, sourceId]);
  const run = async (action: () => Promise<unknown>) => { setError(''); setBusy(true); try { await action(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); } finally { setBusy(false); } };
  const host = () => run(async () => { const code = await call<string>('host_room', { displayName: name.trim() || 'Host', sourceId, sourceLabel: selected?.title || 'Audio source' }); setSnapshot(s => ({ ...s, mode: 'hosting', roomCode: code, sourceLabel: selected?.title || 'Audio source', status: 'live', path: 'publishing' })); });
  const join = () => run(async () => {
    try {
      await call('join_room', { displayName: name.trim() || 'Listener', roomCode });
    } catch (e) {
      // The backend emits a transient connecting state before it attempts the
      // peer connection. Return to the form when that attempt fails.
      setSnapshot(idle);
      throw e;
    }
  });
  const leave = () => run(async () => { await call('leave_room'); setSnapshot(idle); });
  const changeVolume = (value: number) => { setVolume(value); if (tauri) void call('set_volume', { volume: value }); };

  if (snapshot.mode !== 'idle') return <Room snapshot={snapshot} error={error} volume={volume} onVolume={changeVolume} onLeave={leave} busy={busy} />;
  return <main className="shell home"><header className="topbar"><div className="brand"><span className="mark" aria-hidden="true">◒</span><span>sonora</span></div><span className="build">PRIVATE LISTENING / {tauri ? 'DESKTOP' : 'PREVIEW'}</span></header><section className="welcome"><p className="kicker">A room for sound</p><h1>Hear it together.</h1><p className="lede">Share the audio already playing on your computer. No uploads, no accounts — just a room code.</p><label className="field-label" htmlFor="name">Your name</label><input id="name" value={name} onChange={e => setName(e.target.value)} placeholder="How should we call you?" maxLength={32} /><div className="choice-grid"><section className="choice"><div><span className="choice-number">01</span><h2>Host a room</h2><p>Choose a window or app. Sonora streams its audio in lossless PCM.</p></div><button className="button primary" onClick={() => { void refreshSources(); (document.getElementById('host-panel') as HTMLDialogElement | null)?.showModal(); }} disabled={busy}>Choose source <span>→</span></button></section><section className="choice"><div><span className="choice-number">02</span><h2>Join a room</h2><p>Enter the code from your host and listen from anywhere.</p></div><div className="join-row"><input aria-label="Room code" value={roomCode} onChange={e => setRoomCode(e.target.value)} placeholder="SNR-XXXX-XXXX" /><button className="button ghost" onClick={join} disabled={busy || roomCode.length < 8}>Join</button></div></section></div>{error && <p className="error" role="alert">{error}</p>}<p className="fineprint">End-to-end peer connection · Direct when possible · Relay when needed</p></section><dialog id="host-panel"><div className="dialog-head"><div><span className="kicker">Host a room</span><h2>What should we stream?</h2></div><button className="icon-button" onClick={() => (document.getElementById('host-panel') as HTMLDialogElement | null)?.close()} aria-label="Close">×</button></div><div className="source-list">{sources.length ? sources.map(source => <button key={source.id} className={`source-row ${source.id === sourceId ? 'selected' : ''}`} onClick={() => setSourceId(source.id)}><span className={`source-icon ${source.kind}`} aria-hidden="true">{source.kind === 'window' ? '□' : source.kind === 'system' ? '⌁' : '◉'}</span><span><strong>{source.title}</strong><small>{source.subtitle}</small></span><span className="radio" aria-hidden="true" /></button>) : <p className="empty">No sources found. Check Screen Recording permission, then refresh.</p>}</div><div className="dialog-actions"><button className="button ghost" onClick={refreshSources}>Refresh sources</button><button className="button primary" disabled={!sourceId || busy} onClick={() => { (document.getElementById('host-panel') as HTMLDialogElement | null)?.close(); host(); }}>Go live <span>→</span></button></div></dialog></main>;
}

function Room({ snapshot, error, volume, onVolume, onLeave, busy }: { snapshot: Snapshot; error: string; volume: number; onVolume: (n: number) => void; onLeave: () => void; busy: boolean }) { const hosting = snapshot.mode === 'hosting'; return <main className="shell room"><header className="topbar"><div className="brand"><span className="mark" aria-hidden="true">◒</span><span>sonora</span></div><div className="room-state"><span className={`state-dot ${snapshot.status}`} />{snapshot.status === 'live' ? 'Live' : snapshot.status}</div></header><section className="room-content"><div className="room-heading"><div><p className="kicker">{hosting ? 'You are hosting' : `Listening with ${snapshot.hostName || 'the host'}`}</p><h1>{snapshot.sourceLabel || 'Connecting to room'}</h1></div><button className="button danger" onClick={onLeave} disabled={busy}>Leave room</button></div><div className="signal"><div className="signal-top"><span>{snapshot.path === 'connected' ? 'Direct peer connection' : snapshot.path === 'relay' ? 'Relayed connection' : snapshot.path}</span><span className="mono">{snapshot.lossless}</span></div><div className="meter" aria-label={`Audio level ${Math.round(snapshot.level * 100)} percent`}><span style={{ width: `${Math.min(100, Math.round(snapshot.level * 180))}%` }} /></div><p className="signal-note">{snapshot.status === 'live' ? 'Audio is moving through the room.' : 'Finding the best path to the room.'}</p></div><div className="room-grid"><section className="room-panel invite"><span className="panel-label">Room code</span><div className="code">{snapshot.roomCode || '—'}</div><p>Share this code with the people you want to hear it.</p></section><section className="room-panel listeners"><div className="panel-head"><span className="panel-label">Listeners</span><span className="count">{snapshot.listeners.length}</span></div>{snapshot.listeners.length ? <ul>{snapshot.listeners.map(listener => <li key={listener}><span className="avatar">{listener.charAt(0).toUpperCase()}</span>{listener}{listener === snapshot.hostName && <small>host</small>}</li>)}</ul> : <p className="empty">{hosting ? 'Waiting for someone to join.' : 'You are the first listener here.'}</p>}</section></div>{!hosting && <section className="volume"><label htmlFor="volume">Listening volume <span>{Math.round(volume * 100)}%</span></label><input id="volume" type="range" min="0" max="1" step="0.01" value={volume} onChange={e => onVolume(Number(e.target.value))} /></section>}{(error || snapshot.error) && <p className="error" role="alert">{error || snapshot.error}</p>}</section></main>; }
export default App;
