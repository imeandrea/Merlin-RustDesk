// Client di assistenza remota Merlin-RustDesk: interfaccia Tauri sul nucleo
// RustDesk (questo repository, licenza AGPL-3.0).
//
// Lo stesso eseguibile ha più ruoli, come RustDesk: senza argomenti apre la
// finestra principale (e avvia il server se non c'è), con `--cm` la finestra
// delle connessioni in arrivo; gli altri ruoli (`--server`, `--service`…) li
// gestisce `core_main` del nucleo, senza finestre.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use librustdesk::merlin_client as rd;
use serde_json::{json, Value};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// Nome usato per configurazione e IPC: separato da RustDesk e da Merlin.
const APP_NAME: &str = "MerlinRustDesk";

#[tauri::command]
fn rd_info() -> Value {
    json!({
        "id": rd::my_id(),
        "status": rd::status(),
        "password": rd::temporary_password(),
        "permanentSet": rd::is_permanent_password_set(),
        "version": rd::version(),
    })
}

#[tauri::command]
fn rd_new_password() {
    rd::update_temporary_password()
}

#[tauri::command]
fn rd_set_permanent_password(password: String) -> bool {
    rd::set_permanent_password(password)
}

#[tauri::command]
fn cm_authorize(id: i32) {
    rd::cm_authorize(id)
}

#[tauri::command]
fn cm_close(id: i32) {
    rd::cm_close(id)
}

#[tauri::command]
fn cm_chat(id: i32, text: String) {
    rd::cm_send_chat(id, text)
}

#[tauri::command]
fn cm_permission(id: i32, name: String, enabled: bool) {
    rd::cm_switch_permission(id, name, enabled)
}

fn main() {
    rd::init(APP_NAME, env!("MERLIN_RD_SERVER"), env!("MERLIN_RD_KEY"), env!("MERLIN_RD_RELAY"));
    let args = match rd::core_main() {
        Some(rd::CoreMainAction::StartUi(args)) => args,
        Some(rd::CoreMainAction::ExitFailure(_)) => std::process::exit(1),
        // Ruolo senza finestre (server, servizio…): già eseguito.
        None => return,
    };
    let cm = args.first().is_some_and(|a| a == "--cm");
    rd::start_status_sync();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            rd_info, rd_new_password, rd_set_permanent_password, cm_authorize, cm_close, cm_chat, cm_permission
        ])
        .setup(move |app| {
            let (label, url, w, h) = if cm { ("cm", "index.html#cm", 400.0, 600.0) } else { ("main", "index.html", 420.0, 680.0) };
            let win = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
                .title("Merlin-RustDesk")
                .inner_size(w, h)
                .min_inner_size(400.0, if cm { 560.0 } else { 640.0 })
                .build()?;
            if cm {
                // Eventi delle connessioni in arrivo verso la finestra.
                let handle = app.handle().clone();
                rd::start_cm(move |ev| { let _ = handle.emit_to("cm", "cm", ev); });
                let _ = win.set_always_on_top(true);
            }
            let _ = app.get_webview_window(label);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("avvio del client");
}
