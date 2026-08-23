import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type Mode = 'idle' | 'hosting' | 'listening';
type Source = { id: string; kind: string; title: string; subtitle: string };
type Snapshot = { mode: Mode; roomCode?: string; sourceLabel?: string; listeners: string[]; level: number; path: string; status: string; error?: string; hostName?: string; lossless: string; streamQuality: string };
type Notice = { id: number; name: string; action: 'joined' | 'left' };
const idle: Snapshot = { mode: 'idle', listeners: [], level: 0, path: 'idle', status: 'idle', lossless: '16-bit · 48 kHz stereo PCM', streamQuality: 'Auto' };
const tauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const call = async <T,>(cmd: string, args?: Record<string, unknown>) => tauri ? invoke<T>(cmd, args) : Promise.reject(new Error('Run Sonora as a desktop app to connect audio.'));
if (typeof document !== 'undefined' && !document.getElementById('sonora-quality-options')) {
  const style = document.createElement('style');
  style.id = 'sonora-quality-options';
  style.textContent = `.quality-options{border:0;border-top:1px solid var(--line);margin:22px 0 0;padding:18px 0 0}.quality-options legend{padding:0}.quality-options label{display:grid;grid-template-columns:16px minmax(90px,auto) 1fr;align-items:center;gap:10px;padding:10px 4px;border-radius:8px;color:var(--ink);cursor:pointer;transition:background 180ms ease}.quality-options label:hover{background:var(--surface-2)}.quality-options input[type=radio]{appearance:none;width:14px;height:14px;margin:0;padding:0;border:1px solid var(--muted);border-radius:50%;background:transparent;box-shadow:inset 0 0 0 3px var(--surface)}.quality-options input[type=radio]:checked{border-color:var(--primary);background:var(--primary)}.quality-options input[type=radio]:focus-visible{outline:2px solid var(--primary);outline-offset:3px}.quality-options small{color:var(--muted);font-size:12px}`;
  document.head.append(style);
}
if (typeof document !== 'undefined') {
  const addDiagnosticsControl = () => {
    const dialog = document.getElementById('settings-panel');
    if (!dialog || document.getElementById('diagnostics-log-control')) return;
    const section = document.createElement('section');
    section.id = 'diagnostics-log-control';
    section.style.cssText = 'margin-top:24px;padding-top:18px;border-top:1px solid var(--line)';
    section.innerHTML = '<span class="field-label">Diagnostics</span><p class="settings-note" style="margin-bottom:12px">Write connection and playback metrics to a text file for support.</p><button class="button ghost" type="button">Start diagnostics log</button><p class="settings-note" aria-live="polite"></p>';
    const button = section.querySelector('button')!;
    const note = section.querySelectorAll('p')[1]!;
    let active = false;
    button.addEventListener('click', async () => {
      button.setAttribute('disabled', 'true');
      try {
        const path = await call<string | null>('set_diagnostics_logging', { enabled: !active });
        active = !active;
        button.textContent = active ? 'Stop diagnostics log' : 'Start diagnostics log';
        note.textContent = active ? `Writing to: ${path}` : 'Diagnostics logging stopped.';
      } catch (error) { note.textContent = String(error); }
      button.removeAttribute('disabled');
    });
    dialog.append(section);
  };
  new MutationObserver(addDiagnosticsControl).observe(document.documentElement, { childList: true, subtree: true });
  queueMicrotask(addDiagnosticsControl);
}

function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(idle);
  const [sources, setSources] = useState<Source[]>([]);
  const [name, setName] = useState(''); const [roomCode, setRoomCode] = useState(''); const [sourceId, setSourceId] = useState('');
  const [quality, setQuality] = useState('auto'); const [busy, setBusy] = useState(false); const [error, setError] = useState(''); const [volume, setVolume] = useState(1); const [notices, setNotices] = useState<Notice[]>([]);
  useEffect(() => { if (!tauri) return; let active = true; void call<Snapshot>('room_snapshot').then(s => { if (active) setSnapshot(s); }).catch(e => setError(String(e))); const unlisten = listen<Snapshot>('room-state', e => { if (active) setSnapshot(e.payload); }); return () => { active = false; void unlisten.then(stop => stop()); }; }, []);
  useEffect(() => { if (!tauri) return; let nextId = 0; const unlisten = listen<Omit<Notice, 'id'>>('listener-presence', event => { const notice = { ...event.payload, id: nextId++ }; setNotices(current => [...current, notice].slice(-3)); window.setTimeout(() => setNotices(current => current.filter(item => item.id !== notice.id)), 4_000); }); return () => { void unlisten.then(stop => stop()); }; }, []);
  const refreshSources = useCallback(async () => { try { const list = await call<Source[]>('list_sources'); setSources(list); setSourceId(current => current || list[0]?.id || ''); } catch (e) { setError(String(e)); } }, []);
  useEffect(() => { if (snapshot.mode === 'hosting') void refreshSources(); }, [snapshot.mode, refreshSources]);
  const selected = useMemo(() => sources.find(s => s.id === sourceId), [sources, sourceId]);
  const run = async (action: () => Promise<unknown>) => { setError(''); setBusy(true); try { await action(); } catch (e) { setError(String(e)); } finally { setBusy(false); } };
  const host = () => run(async () => { const sourceLabel = selected?.title || 'Audio source'; const code = await call<string>('host_room', { displayName: name.trim() || 'Host', sourceId, sourceLabel, quality }); setSnapshot(s => ({ ...s, mode: 'hosting', roomCode: code, sourceLabel, status: 'live', path: 'publishing' })); });
  const join = () => run(async () => { try { await call('join_room', { displayName: name.trim() || 'Listener', roomCode }); } catch (e) { setSnapshot(idle); throw e; } });
  const leave = () => run(async () => { await call('leave_room'); setSnapshot(idle); setNotices([]); });
  const open = (id: string) => (document.getElementById(id) as HTMLDialogElement | null)?.showModal(); const close = (id: string) => (document.getElementById(id) as HTMLDialogElement | null)?.close();
  if (snapshot.mode !== 'idle') return <Room snapshot={snapshot} error={error} notices={notices} volume={volume} onVolume={value => { setVolume(value); if (tauri) void call('set_volume', { volume: value }); }} onLeave={leave} busy={busy} />;
  return <main className="shell home"><header className="topbar"><div className="brand"><span className="mark">◒</span><span>sonora</span></div><button className="settings-button" onClick={() => open('settings-panel')}>Settings <span>⌘,</span></button></header><section className="welcome"><div className="intro"><p className="kicker">Private listening</p><h1>Hear it<br />together.</h1></div><div className="choice-grid"><section className="choice"><div><span className="choice-label">Host</span><h2>Start a room</h2></div><button className="button primary" onClick={() => { void refreshSources(); open('host-panel'); }} disabled={busy}>Choose source →</button></section><section className="choice"><div><span className="choice-label">Listen</span><h2>Join a room</h2></div><div className="join-row"><input aria-label="Room code" value={roomCode} onChange={e => setRoomCode(e.target.value)} placeholder="SNR-XXXX-XXXX" /><button className="button ghost" onClick={join} disabled={busy || roomCode.length < 8}>Join</button></div></section>{error && <p className="error">{error}</p>}</div></section><dialog id="settings-panel" className="settings-dialog"><div className="dialog-head"><div><span className="kicker">Preferences</span><h2>Settings</h2></div><button className="icon-button" onClick={() => close('settings-panel')}>×</button></div><label className="field-label" htmlFor="name">Display name</label><input id="name" value={name} onChange={e => setName(e.target.value)} placeholder="How should we call you?" maxLength={32} /><p className="settings-note">Used when you host or join a room.</p><div className="dialog-actions"><button className="button primary" onClick={() => close('settings-panel')}>Done</button></div></dialog><dialog id="host-panel"><div className="dialog-head"><div><span className="kicker">Host a room</span><h2>What should we stream?</h2></div><button className="icon-button" onClick={() => close('host-panel')}>×</button></div><div className="source-list">{sources.length ? sources.map(source => <button key={source.id} className={`source-row ${source.id === sourceId ? 'selected' : ''}`} onClick={() => setSourceId(source.id)}><span className={`source-icon ${source.kind}`}>{source.kind === 'window' ? '□' : source.kind === 'system' ? '⌁' : '◉'}</span><span><strong>{source.title}</strong><small>{source.subtitle}</small></span><span className="radio" /></button>) : <p className="empty">No sources found. Check Screen Recording permission, then refresh.</p>}</div><fieldset className="quality-options"><legend className="field-label">Streaming quality</legend>{[['auto', 'Auto', 'Adapts when a listener falls behind.'], ['high', 'High', '48 kHz stereo · 1.5 Mbps'], ['balanced', 'Balanced', '32 kHz stereo · 1.0 Mbps'], ['saver', 'Data saver', '24 kHz stereo · 0.75 Mbps']].map(([value, label, note]) => <label key={value}><input type="radio" name="quality" value={value} checked={quality === value} onChange={e => setQuality(e.target.value)} />{label}<small>{note}</small></label>)}</fieldset><div className="dialog-actions"><button className="button ghost" onClick={refreshSources}>Refresh sources</button><button className="button primary" disabled={!sourceId || busy} onClick={() => { close('host-panel'); host(); }}>Go live →</button></div></dialog></main>;
}

function Room({ snapshot, error, notices, volume, onVolume, onLeave, busy }: { snapshot: Snapshot; error: string; notices: Notice[]; volume: number; onVolume: (n: number) => void; onLeave: () => void; busy: boolean }) { const hosting = snapshot.mode === 'hosting'; return <main className="shell room"><header className="topbar"><div className="brand"><span className="mark">◒</span><span>sonora</span></div><div className="room-state"><span className={`state-dot ${snapshot.status}`} />{snapshot.status === 'live' ? 'Live' : snapshot.status}</div></header><section className="room-content"><div className="room-heading"><div><p className="kicker">{hosting ? 'You are hosting' : `Listening with ${snapshot.hostName || 'the host'}`}</p><h1>{snapshot.sourceLabel || 'Connecting to room'}</h1></div><button className="button danger" onClick={onLeave} disabled={busy}>Leave room</button></div>{notices.length > 0 && <div className="presence-notices" role="status">{notices.map(n => <p key={n.id}><span>{n.action === 'joined' ? '+' : '−'}</span>{n.name} {n.action === 'joined' ? 'joined the room' : 'left the room'}</p>)}</div>}<div className="signal"><div className="signal-top"><span>{snapshot.path === 'connected' ? 'Direct peer connection' : snapshot.path === 'relay' ? 'Relayed connection' : snapshot.path}</span><span className="mono">{snapshot.lossless} · {snapshot.streamQuality}</span></div><div className="meter"><span style={{ width: `${Math.min(100, Math.round(snapshot.level * 180))}%` }} /></div><p className="signal-note">{snapshot.status === 'live' ? 'Audio is moving through the room.' : 'Finding the best path to the room.'}</p></div><div className="room-grid"><section className="room-panel invite"><span className="panel-label">Room code</span><div className="code">{snapshot.roomCode || '—'}</div><p>Share this code with the people you want to hear it.</p></section><section className="room-panel listeners"><div className="panel-head"><span className="panel-label">Listeners</span><span className="count">{snapshot.listeners.length}</span></div>{snapshot.listeners.length ? <ul>{snapshot.listeners.map(listener => <li key={listener}><span className="avatar">{listener.charAt(0).toUpperCase()}</span>{listener}</li>)}</ul> : <p className="empty">{hosting ? 'Waiting for someone to join.' : 'You are the first listener here.'}</p>}</section></div>{!hosting && <section className="volume"><label htmlFor="volume">Listening volume <span>{Math.round(volume * 100)}%</span></label><input id="volume" type="range" min="0" max="2" step="0.01" value={volume} onChange={e => onVolume(Number(e.target.value))} /></section>}{(error || snapshot.error) && <p className="error">{error || snapshot.error}</p>}</section></main>; }
export default App;
