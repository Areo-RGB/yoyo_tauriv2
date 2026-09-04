mod server;

use server::{create_shared_state, SharedServerState};
use tauri::State;

#[tauri::command]
fn start_sync_server(
    state: State<'_, SharedServerState>,
    port: Option<u16>,
) -> Result<String, String> {
    let p = port.unwrap_or(8080);
    server::start_http_server((*state).clone(), p)
}

#[tauri::command]
fn stop_sync_server(state: State<'_, SharedServerState>) -> Result<(), String> {
    let mut lock = state.lock().map_err(|e| e.to_string())?;
    lock.running = false;
    Ok(())
}

#[tauri::command]
fn broadcast_test_state(
    state: State<'_, SharedServerState>,
    state_json: String,
) -> Result<(), String> {
    let mut lock = state.lock().map_err(|e| e.to_string())?;
    lock.latest_state = state_json;
    Ok(())
}

#[tauri::command]
fn pop_remote_actions(state: State<'_, SharedServerState>) -> Result<Vec<String>, String> {
    let mut lock = state.lock().map_err(|e| e.to_string())?;
    let actions = lock.pending_actions.clone();
    lock.pending_actions.clear();
    Ok(actions)
}

#[tauri::command]
fn get_local_ips() -> Vec<String> {
    server::get_local_ips()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shared_state = create_shared_state();

    // Auto-start sync server on port 8080 immediately when app opens
    let _ = server::start_http_server(shared_state.clone(), 8080);

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(shared_state)
        .invoke_handler(tauri::generate_handler![
            start_sync_server,
            stop_sync_server,
            broadcast_test_state,
            pop_remote_actions,
            get_local_ips
        ])
        .run(tauri::generate_context!())
        .expect("error while running Yo-Yo Tracker");
}
