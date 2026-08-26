mod audio;
mod identity;
mod now_playing;
mod protocol;
mod room;

use room::{AppState, RoomSnapshot};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
fn list_sources() -> Result<Vec<audio::AudioSource>, String> {
    room::list_sources()
}

#[tauri::command]
fn room_snapshot(state: State<Arc<AppState>>) -> RoomSnapshot {
    room::current_snapshot(&state)
}

#[tauri::command]
async fn host_room(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    display_name: String,
    source_id: String,
    source_label: String,
    quality: String,
) -> Result<String, String> {
    room::host_room(
        app,
        state.inner().clone(),
        display_name,
        source_id,
        source_label,
        quality,
    )
    .await
}

#[tauri::command]
async fn join_room(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    display_name: String,
    room_code: String,
) -> Result<(), String> {
    room::join_room(app, state.inner().clone(), display_name, room_code).await
}

#[tauri::command]
async fn leave_room(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    room::leave(state.inner()).await;
    Ok(())
}

#[tauri::command]
fn set_volume(state: State<Arc<AppState>>, volume: f32) {
    room::set_volume(&state, volume);
}

#[tauri::command]
fn set_diagnostics_logging(
    app: tauri::AppHandle,
    state: State<Arc<AppState>>,
    enabled: bool,
) -> Result<Option<String>, String> {
    room::set_diagnostics_logging(&app, state.inner(), enabled)
}

#[tauri::command]
fn diagnostics_log_status(state: State<Arc<AppState>>) -> Option<String> {
    room::diagnostics_log_status(state.inner())
}

#[tauri::command]
fn current_now_playing() -> Option<now_playing::NowPlaying> {
    now_playing::current()
}

pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("sonora=info,iroh=info")
        .try_init();

    let state = Arc::new(AppState::new());
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            list_sources,
            room_snapshot,
            host_room,
            join_room,
            leave_room,
            set_volume,
            set_diagnostics_logging,
            diagnostics_log_status,
            current_now_playing
        ])
        .run(tauri::generate_context!())
        .expect("error while running Sonora");
}
