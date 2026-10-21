// Client di assistenza remota Merlin-RustDesk: interfaccia Tauri sul nucleo
// RustDesk (questo repository, licenza AGPL-3.0).
//
// Lo stesso eseguibile ha più ruoli, come RustDesk: senza argomenti apre la
// finestra principale (e avvia il server se non c'è), con `--cm` la finestra
// delle connessioni in arrivo; gli altri ruoli (`--server`, `--service`…) li
// gestisce `core_main` del nucleo, senza finestre.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod brand;
#[cfg(target_os = "linux")]
mod linux_install;

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
    // Richiesta in arrivo: la finestra torna in primo piano (anche dalla
    // tray). Collegamento accettato (anche con la password): si riduce a icona.
    if ev["ev"] == "add" {
        if let Some(w) = app.get_webview_window(label) {
            if ev["client"]["authorized"].as_bool() == Some(true) {
                let _ = w.minimize();
            } else {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }
    }
    // Messaggio del tecnico in chat: la finestra torna visibile.
    if ev["ev"] == "chat" {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.show();
            let _ = w.unminimize();
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
        "version": env!("CARGO_PKG_VERSION"),
        "install": rd::install_state(),
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

/// «Avvia con il computer»: installa il servizio (Windows).
#[tauri::command]
async fn rd_install() -> Result<(), String> {
    #[cfg(windows)]
    return rd::install_service();
    #[cfg(target_os = "linux")]
    return tauri::async_runtime::spawn_blocking(linux_install::install).await.map_err(|e| e.to_string())?;
    #[cfg(not(any(windows, target_os = "linux")))]
    Err("Non disponibile su questo sistema".into())
}

#[tauri::command]
async fn rd_uninstall() -> Result<(), String> {
    #[cfg(windows)]
    return rd::uninstall_service();
    #[cfg(target_os = "linux")]
    return tauri::async_runtime::spawn_blocking(linux_install::uninstall).await.map_err(|e| e.to_string())?;
    #[cfg(not(any(windows, target_os = "linux")))]
    Err("Non disponibile su questo sistema".into())
}

#[tauri::command]
fn rd_exit(app: AppHandle) {
    // Linux: dopo l'installazione dall'AppImage si apre il client installato
    // (su Windows lo apre già l'installazione).
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPIMAGE").is_some() && rd::install_state() == "installed" {
        linux_install::open_installed();
    }
    app.exit(0)
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

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Icona nella tray: clic = apri; menu Apri / Esci.
fn make_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let name = BRAND.get().and_then(|b| b["name"].as_str()).filter(|n| !n.is_empty()).unwrap_or("Merlin-RustDesk").to_string();
    let open = MenuItem::with_id(app, "open", "Apri", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Esci", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let icon = BRAND.get().and_then(brand::logo_png).and_then(|b| tauri::image::Image::from_bytes(&b).ok())
        .or_else(|| app.default_window_icon().cloned());
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip(&name)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = icon {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

fn main() {
    let brand = BRAND.get_or_init(brand::load);
    let (server, key, relay, api) = brand::server(brand);
    rd::init(APP_NAME, &server, &key, &relay, &api);
    #[cfg(windows)]
    if std::env::args().nth(1).as_deref() == Some("--merlin-install") {
        std::process::exit(if rd::run_install() { 0 } else { 1 });
    }
    // `--tray`: lo lancia il servizio all'accesso dell'utente (client
    // installato). Al posto della tray di RustDesk parte questa interfaccia,
    // nascosta, con la sua icona nella tray.
    let tray_start = std::env::args().nth(1).as_deref() == Some("--tray");
    let args = if tray_start { Vec::new() } else { match rd::core_main() {
        Some(rd::CoreMainAction::StartUi(args)) => args,
        Some(rd::CoreMainAction::ExitFailure(_)) => std::process::exit(1),
        // Ruolo senza finestre (server, servizio…): già eseguito.
        None => return,
    } };
    let cm = args.first().is_some_and(|a| a == "--cm");
    rd::start_status_sync();

    let mut builder = tauri::Builder::default();
    if !cm {
        // Una sola finestra principale: riaprire il client la mostra.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().any(|a| a == "--tray") {
                show_main(app);
            }
        }));
    }
    builder
        .on_window_event(|w, ev| {
            // Chiudere la finestra principale la nasconde: il client resta nella tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                if w.label() == "main" {
                    api.prevent_close();
                    let _ = w.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            rd_info, rd_brand, rd_new_password, rd_install, rd_uninstall, rd_exit, rd_set_permanent_password, cm_backlog, cm_authorize, cm_close, cm_chat, cm_permission
        ])
        .setup(move |app| {
            let (label, url, w, h) = if cm { ("cm", "index.html#cm", 400.0, 600.0) } else { ("main", "index.html", 420.0, 680.0) };
            let win = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
                .title(BRAND.get().and_then(|b| b["name"].as_str()).filter(|n| !n.is_empty()).unwrap_or("Merlin-RustDesk"))
                .inner_size(w, h)
                .min_inner_size(400.0, if cm { 560.0 } else { 640.0 })
                .visible(!tray_start)
                .build()?;
            if !cm {
                make_tray(app)?;
            }
            // Gestore delle connessioni in arrivo: la finestra principale (il
            // server la trova già in ascolto e non apre altre finestre) o, se
            // il client non era aperto, la finestra --cm lanciata dal server.
            let handle = app.handle().clone();
            rd::start_cm(move |ev| push_cm_event(&handle, label, ev));
            // Logo personalizzato anche come icona della finestra e della barra.
            if let Some(icon) = BRAND.get().and_then(brand::logo_png) {
                if let Ok(img) = tauri::image::Image::from_bytes(&icon) {
                    let _ = win.set_icon(img);
                }
            }
            if cm {
                let _ = win.set_always_on_top(true);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("avvio del client");
}
