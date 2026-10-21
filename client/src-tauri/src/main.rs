// Client di assistenza remota Merlin-RustDesk: interfaccia Tauri sul nucleo
// RustDesk (questo repository, licenza AGPL-3.0).
//
// Lo stesso eseguibile ha più ruoli, come RustDesk: senza argomenti apre la
// finestra principale (e avvia il server se non c'è), con `--cm` la finestra
// delle connessioni in arrivo; gli altri ruoli (`--server`, `--service`…) li
// gestisce `core_main` del nucleo, senza finestre.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod brand;

use std::sync::{Mutex, OnceLock};

use librustdesk::merlin_client as rd;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// Eventi delle connessioni (numerati): la pagina li recupera appena è
/// pronta, così niente va perso se arrivano prima che sia caricata.
static CM_EVENTS: Mutex<Vec<Value>> = Mutex::new(Vec::new());

fn push_cm_event(app: &AppHandle, label: &str, mut ev: Value) {
    let seq = {
        let mut events = CM_EVENTS.lock().unwrap();
        let seq = events.len() as u64 + 1;
        ev["seq"] = json!(seq);
        events.push(ev.clone());
        seq
    };
    // Richiesta in arrivo: la finestra torna in primo piano.
    if ev["ev"] == "add" {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
    let _ = app.emit("cm", ev);
    let _ = seq;
}

#[tauri::command]
fn cm_backlog() -> Vec<Value> {
    CM_EVENTS.lock().unwrap().clone()
}

static BRAND: OnceLock<Value> = OnceLock::new();

/// Personalizzazione del client (nome, colore, logo, contatti), o null.
#[tauri::command]
fn rd_brand() -> Value {
    let mut b = BRAND.get().cloned().unwrap_or(Value::Null);
    // Il server non serve alla pagina.
    if let Some(o) = b.as_object_mut() {
        o.remove("server");
    }
    b
}

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
    let brand = BRAND.get_or_init(brand::load);
    let (server, key, relay) = brand::server(brand);
    rd::init(APP_NAME, &server, &key, &relay);
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
            rd_info, rd_brand, rd_new_password, rd_set_permanent_password, cm_backlog, cm_authorize, cm_close, cm_chat, cm_permission
        ])
        .setup(move |app| {
            let (label, url, w, h) = if cm { ("cm", "index.html#cm", 400.0, 600.0) } else { ("main", "index.html", 420.0, 680.0) };
            let win = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
                .title(BRAND.get().and_then(|b| b["name"].as_str()).filter(|n| !n.is_empty()).unwrap_or("Merlin-RustDesk"))
                .inner_size(w, h)
                .min_inner_size(400.0, if cm { 560.0 } else { 640.0 })
                .build()?;
            // Gestore delle connessioni in arrivo: la finestra principale (il
            // server la trova già in ascolto e non apre altre finestre) o, se
            // il client non era aperto, la finestra --cm lanciata dal server.
            let handle = app.handle().clone();
            rd::start_cm(move |ev| push_cm_event(&handle, label, ev));
            if cm {
                let _ = win.set_always_on_top(true);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("avvio del client");
}
