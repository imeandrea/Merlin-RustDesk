// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

// Finestra di visione: questo client vede e comanda un altro computer, processo a sé.
// - Inversione di ruolo: la lancia il server del client (`--merlin-view <id>
//   --switch_uuid <codice>`) come utente collegato; il codice lo verifica il
//   login di RustDesk col server locale.
// - «Controlla un PC» dalla finestra principale: `--merlin-connect <id>`, la
//   password arriva dallo standard input (mai tra gli argomenti, visibili a tutti).

use std::sync::Mutex;

use librustdesk::merlin_viewer::{start_files, start_viewer, FilesHandle, ViewerHandle, ViewerTarget};
use serde_json::Value;
use tauri::{
    ipc::{Channel, Response},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};

static HANDLE: Mutex<Option<ViewerHandle>> = Mutex::new(None);
static TARGET: Mutex<Option<(String, ViewerTarget)>> = Mutex::new(None);
/// PC remoto e ultima password usata: per la connessione dei file.
static PEER: Mutex<String> = Mutex::new(String::new());
static PASSWORD: Mutex<String> = Mutex::new(String::new());
static FILES: Mutex<Option<FilesHandle>> = Mutex::new(None);

#[tauri::command]
fn viewer_start(on_tile: Channel<Response>, on_event: Channel<Value>) -> Result<(), String> {
    let Some((peer, target)) = TARGET.lock().unwrap().take() else {
        return Err("visione già avviata".into());
    };
    let events = on_event.clone();
    let handle = start_viewer(
        peer,
        target,
        move |ev| {
            let _ = events.send(ev);
        },
        move |tile| {
            let _ = on_tile.send(Response::new(tile));
        },
    )
    .ok_or("RustDesk non avviato")?;
    *HANDLE.lock().unwrap() = Some(handle);
    Ok(())
}

#[tauri::command]
fn viewer_input(input: Value) {
    let guard = HANDLE.lock().unwrap();
    let Some(h) = guard.as_ref() else { return };
    let s = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let n = |k: &str| input.get(k).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let b = |k: &str| input.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    match s("t") {
        "mouse" => h.mouse(n("mask"), n("x"), n("y"), b("alt"), b("ctrl"), b("shift"), b("command")),
        "key" => h.key(s("code"), b("down"), n("locks")),
        "text" => h.text(s("text")),
        "display" => h.switch_display(n("display")),
        "login" => {
            *PASSWORD.lock().unwrap() = s("password").to_owned();
            h.login(s("password"))
        }
        "switch_sides" => h.switch_sides(),
        "chat" => h.chat(s("text")),
        "ctrl_alt_del" => h.ctrl_alt_del(),
        "restart" => h.restart(),
        _ => {}
    }
}

/// Apre la connessione dei file (con l'ultima password; senza, la chiede o il PC remoto accetta).
#[tauri::command]
fn viewer_files_open(on_event: Channel<Value>) {
    let peer = PEER.lock().unwrap().clone();
    let password = PASSWORD.lock().unwrap().clone();
    let handle = start_files(peer, password, move |ev| {
        let _ = on_event.send(ev);
    });
    if let Some(old) = FILES.lock().unwrap().replace(handle) {
        old.close();
    }
}

#[tauri::command]
fn viewer_files(input: Value) {
    let guard = FILES.lock().unwrap();
    let Some(f) = guard.as_ref() else { return };
    let s = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let n = |k: &str| input.get(k).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    match s("t") {
        "list" => f.list(s("path")),
        "download" => f.download(n("job"), s("path"), s("to")),
        "upload" => f.upload(n("job"), s("path"), s("to"), input.get("windows").and_then(|v| v.as_bool()).unwrap_or(false)),
        "cancel" => f.cancel(n("job")),
        "login" => {
            *PASSWORD.lock().unwrap() = s("password").to_owned();
            f.login(s("password"))
        }
        _ => {}
    }
}

/// Cartella dove finiscono i file scaricati (Download dell'utente).
#[tauri::command]
fn viewer_download_dir(app: AppHandle) -> String {
    app.path().download_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

#[tauri::command]
fn viewer_close(app: AppHandle) {
    if let Some(h) = HANDLE.lock().unwrap().take() {
        h.close();
    }
    if let Some(f) = FILES.lock().unwrap().take() {
        f.close();
    }
    app.exit(0);
}

/// Argomenti `--merlin-view <id> --switch_uuid <codice>` (dopo il nome del programma).
pub fn parse_args(args: &[String]) -> Option<(String, String)> {
    match args {
        [_, flag, id, uuid_flag, uuid, ..] if flag == "--merlin-view" && uuid_flag == "--switch_uuid" && !id.is_empty() && !uuid.is_empty() => {
            Some((id.clone(), uuid.clone()))
        }
        _ => None,
    }
}

/// Argomenti `--merlin-connect <id>` (connessione normale, password dallo standard input).
pub fn parse_connect_args(args: &[String]) -> Option<String> {
    match args {
        [_, flag, id, ..] if flag == "--merlin-connect" && !id.is_empty() => Some(id.clone()),
        _ => None,
    }
}

/// Password della connessione normale: una riga dallo standard input (vuota se non c'è).
pub fn read_password() -> String {
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    line.trim_end_matches(['\r', '\n']).to_owned()
}

pub fn run(title: &str, peer: String, target: ViewerTarget, context: tauri::Context<tauri::Wry>) {
    let title = match target {
        ViewerTarget::Switch(_) => format!("{title} — computer del tecnico ({peer})"),
        ViewerTarget::Password(_) => format!("{title} — {peer}"),
    };
    *PEER.lock().unwrap() = peer.clone();
    if let ViewerTarget::Password(p) = &target {
        *PASSWORD.lock().unwrap() = p.clone();
    }
    *TARGET.lock().unwrap() = Some((peer, target));
    tauri::Builder::default()
        .on_window_event(|w, ev| {
            if let tauri::WindowEvent::CloseRequested { .. } = ev {
                if let Some(h) = HANDLE.lock().unwrap().take() {
                    h.close();
                }
                w.app_handle().exit(0);
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![viewer_start, viewer_input, viewer_close, viewer_files_open, viewer_files, viewer_download_dir])
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "viewer", WebviewUrl::App("index.html#viewer".into()))
                .title(&title)
                .inner_size(1280.0, 800.0)
                .min_inner_size(640.0, 400.0)
                .focused(true)
                .build()?;
            Ok(())
        })
        .run(context)
        .expect("avvio della finestra di visione");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn viewer_args_need_id_and_code() {
        assert_eq!(parse_args(&args(&["c", "--merlin-view", "123", "--switch_uuid", "u"])), Some(("123".into(), "u".into())));
        assert_eq!(parse_args(&args(&["c", "--merlin-view", "123"])), None);
        assert_eq!(parse_args(&args(&["c", "--connect", "123", "--switch_uuid", "u"])), None);
        assert_eq!(parse_args(&args(&["c", "--merlin-view", "", "--switch_uuid", "u"])), None);
    }

    #[test]
    fn connect_args_need_only_the_id() {
        assert_eq!(parse_connect_args(&args(&["c", "--merlin-connect", "123"])), Some("123".into()));
        assert_eq!(parse_connect_args(&args(&["c", "--merlin-connect", ""])), None);
        assert_eq!(parse_connect_args(&args(&["c", "--merlin-view", "123"])), None);
    }
}
