// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

//! API for the Merlin-RustDesk client (Tauri UI instead of Flutter/Sciter).
//!
//! - `init`: app name (own config and IPC, never RustDesk's) and the
//!   server/key compiled into the client;
//! - main window: ID, passwords, connection status;
//! - connection manager (`--cm` process): incoming connections, accept,
//!   close, chat and permissions, with events pushed to the UI.

use std::sync::Arc;

use hbb_common::config::{Config, APP_NAME};
use serde_json::{json, Value};

use crate::ui_cm_interface::{start_ipc, Client, ConnectionManager, InvokeUiCM};

pub use crate::core_main::{core_main, CoreMainAction};

/// Must run first in `main`, before `core_main`.
pub fn init(app_name: &str, server: &str, key: &str, relay: &str, api: &str) {
    *APP_NAME.write().unwrap() = app_name.to_owned();
    for (opt, value) in [("custom-rendezvous-server", server), ("key", key), ("relay-server", relay), ("api-server", api)] {
        if Config::get_option(opt) != value {
            Config::set_option(opt.to_owned(), value.to_owned());
        }
    }
}

/// Keeps ID, temporary password and status in sync with the server process.
pub fn start_status_sync() {
    crate::ui_interface::start_option_status_sync();
}

pub fn my_id() -> String {
    crate::ipc::get_id()
}

/// 1 ready, 0 connecting, -1 not connected to the server.
pub fn status() -> i32 {
    crate::ui_interface::get_connect_status().status_num
}

pub fn temporary_password() -> String {
    crate::ui_interface::temporary_password()
}

pub fn update_temporary_password() {
    crate::ui_interface::update_temporary_password()
}

pub fn is_permanent_password_set() -> bool {
    crate::ui_interface::is_permanent_password_set()
}

pub fn set_permanent_password(password: String) -> bool {
    crate::ui_interface::set_permanent_password_with_result(password)
}

pub fn version() -> String {
    crate::ui_interface::get_version()
}

/// Events of the connection manager, as JSON for the UI.
#[derive(Clone)]
pub struct CmEvents {
    emit: Arc<dyn Fn(Value) + Send + Sync>,
}

impl CmEvents {
    fn send(&self, v: Value) {
        (self.emit)(v)
    }
}

impl InvokeUiCM for CmEvents {
    fn add_connection(&self, client: &Client) {
        self.send(json!({ "ev": "add", "client": client }));
    }
    fn remove_connection(&self, id: i32, close: bool) {
        self.send(json!({ "ev": "remove", "id": id, "close": close }));
    }
    fn new_message(&self, id: i32, text: String) {
        self.send(json!({ "ev": "chat", "id": id, "text": text }));
    }
    fn change_theme(&self, _dark: String) {}
    fn change_language(&self) {}
    fn show_elevation(&self, show: bool) {
        self.send(json!({ "ev": "elevation", "show": show }));
    }
    fn update_voice_call_state(&self, _client: &Client) {}
    fn file_transfer_log(&self, _action: &str, _log: &str) {}
}

/// Starts listening for connections from the server (`--cm` process).
pub fn start_cm(emit: impl Fn(Value) + Send + Sync + 'static) {
    #[cfg(target_os = "linux")]
    std::thread::spawn(crate::ipc::start_pa);
    let cm = ConnectionManager { ui_handler: CmEvents { emit: Arc::new(emit) } };
    std::thread::spawn(move || start_ipc(cm));
}

pub fn cm_authorize(id: i32) {
    crate::ui_cm_interface::authorize(id)
}

pub fn cm_close(id: i32) {
    crate::ui_cm_interface::close(id)
}

pub fn cm_send_chat(id: i32, text: String) {
    crate::ui_cm_interface::send_chat(id, text)
}

pub fn cm_switch_permission(id: i32, name: String, enabled: bool) {
    crate::ui_cm_interface::switch_permission(id, name, enabled)
}

pub fn cm_check_click_time(id: i32) {
    crate::ui_cm_interface::check_click_time(id)
}

// ── Accesso non presidiato: installazione come servizio ────────────────────
//
// Windows: `install_me` copia tutta la cartella dell'eseguibile (XCOPY), quindi
// prima l'eseguibile va copiato da solo in una cartella temporanea pulita e
// l'installazione parte da lì (`--merlin-install`). L'eseguibile copiato porta
// con sé la personalizzazione, che sta in fondo al file.

/// Stato dell'installazione: "unsupported", "portable" o "installed".
pub fn install_state() -> &'static str {
    #[cfg(windows)]
    {
        if crate::platform::is_installed() {
            "installed"
        } else {
            "portable"
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Servizio systemd del client (installato dall'AppImage, vedi il client).
        if std::path::Path::new("/etc/systemd/system/merlinrustdesk.service").exists() {
            "installed"
        } else if std::env::var_os("APPIMAGE").is_some() {
            "portable"
        } else {
            "unsupported"
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        "unsupported"
    }
}

/// Avvia l'installazione (con la richiesta di autorizzazione di Windows).
#[cfg(windows)]
pub fn install_service() -> Result<(), String> {
    let cur = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = std::env::temp_dir().join(format!("{}-install", crate::get_app_name()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let staged = dir.join(format!("{}.exe", crate::get_app_name()));
    std::fs::copy(&cur, &staged).map_err(|e| e.to_string())?;
    std::process::Command::new(&staged)
        .arg("--merlin-install")
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Ruolo `--merlin-install` (dalla copia temporanea): installa e apre il
/// client installato; quello portatile si chiude da solo.
#[cfg(windows)]
pub fn run_install() -> bool {
    match crate::platform::install_me("desktopicon startmenu", String::new(), false, false) {
        Ok(()) => true,
        Err(e) => {
            hbb_common::log::error!("installazione non riuscita: {e}");
            false
        }
    }
}

/// Disinstalla servizio e programma (chiude anche questo processo).
#[cfg(windows)]
pub fn uninstall_service() -> Result<(), String> {
    crate::platform::uninstall_me(true).map_err(|e| e.to_string())
}
