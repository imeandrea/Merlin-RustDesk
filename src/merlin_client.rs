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
pub fn init(app_name: &str, server: &str, key: &str, relay: &str) {
    *APP_NAME.write().unwrap() = app_name.to_owned();
    for (opt, value) in [("custom-rendezvous-server", server), ("key", key), ("relay-server", relay)] {
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
