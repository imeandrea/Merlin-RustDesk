// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

//! Merlin bridge: one RustDesk controlling session driven by Merlin.
//!
//! Merlin starts `merlin-bridge` as a child process.
//! - stdin: one JSON command per line (`connect` first, then input, chat, …);
//! - stdout: framed messages `[kind u8][len u32 LE][payload]`:
//!   - `J`: JSON event (`{"ev": …}`);
//!   - `F`: changed screen area, `[x y w h dw dh: u16 LE][RGBA]`.
//!
//! Every mode runs in Merlin's own config directory (HOST_APP_NAME), so it
//! never touches a RustDesk installed on the same machine. Sessions share it
//! with host mode, as RustDesk's UI and server share one install: the peer
//! sees Merlin's host ID, and switching sides reaches the host server via IPC.

use std::{
    io::{BufRead, Write},
    sync::{atomic::AtomicUsize, Arc, Mutex, RwLock},
};

use base::message_proto::*;
use hbb_common::{config::Config, log, rendezvous_proto::ConnType, SessionID};
use serde_json::{json, Value};

use crate::{
    client::*,
    ui_session_interface::{io_loop, InvokeUiSession, Session},
};

/// Config, ID and IPC of Merlin (host mode and sessions).
const HOST_APP_NAME: &str = "MerlinHost";
const TILE: usize = 64;

static OUT: Mutex<()> = Mutex::new(());
/// The session, for callbacks that must answer it (overwrite questions,
/// choice of the Windows session).
static SESSION: std::sync::OnceLock<Session<MerlinHandler>> = std::sync::OnceLock::new();

fn write_msg(kind: u8, payload: &[u8]) {
    let _guard = OUT.lock().unwrap();
    let mut out = std::io::stdout().lock();
    let len = (payload.len() as u32).to_le_bytes();
    if out.write_all(&[kind]).and_then(|_| out.write_all(&len)).and_then(|_| out.write_all(payload)).and_then(|_| out.flush()).is_err() {
        // Merlin went away: nothing else to do.
        std::process::exit(0);
    }
}

fn emit(v: Value) {
    write_msg(b'J', v.to_string().as_bytes());
}

#[derive(Default)]
struct Screen {
    w: usize,
    h: usize,
    prev: Vec<u8>,
}

#[derive(Clone, Default)]
pub struct MerlinHandler {
    screen: Arc<Mutex<Screen>>,
}

impl MerlinHandler {
    /// Sends the tiles that changed since the previous frame (all of them
    /// after a size change). Input is BGRA (ImageFormat::ARGB), output RGBA.
    fn send_changes(&self, rgba: &scrap::ImageRgb) {
        let (w, h) = (rgba.w, rgba.h);
        let stride = rgba.raw.len() / h.max(1);
        if w == 0 || h == 0 || stride < w * 4 {
            return;
        }
        let mut screen = self.screen.lock().unwrap();
        let resized = screen.w != w || screen.h != h || screen.prev.len() != rgba.raw.len();
        if resized {
            emit(json!({ "ev": "display", "width": w, "height": h }));
        }
        for ty in (0..h).step_by(TILE) {
            let th = TILE.min(h - ty);
            for tx in (0..w).step_by(TILE) {
                let tw = TILE.min(w - tx);
                let changed = resized
                    || (ty..ty + th).any(|y| {
                        let a = y * stride + tx * 4;
                        rgba.raw[a..a + tw * 4] != screen.prev[a..a + tw * 4]
                    });
                if !changed {
                    continue;
                }
                let mut msg = Vec::with_capacity(12 + tw * th * 4);
                for v in [tx, ty, tw, th, w, h] {
                    msg.extend_from_slice(&(v as u16).to_le_bytes());
                }
                for y in ty..ty + th {
                    let a = y * stride + tx * 4;
                    for px in rgba.raw[a..a + tw * 4].chunks_exact(4) {
                        msg.extend_from_slice(&[px[2], px[1], px[0], 255]);
                    }
                }
                write_msg(b'F', &msg);
            }
        }
        screen.w = w;
        screen.h = h;
        screen.prev.clear();
        screen.prev.extend_from_slice(&rgba.raw);
    }
}

impl InvokeUiSession for MerlinHandler {
    /// Remote cursor shape as PNG (Merlin uses it as the canvas cursor).
    fn set_cursor_data(&self, cd: CursorData) {
        let colors: Vec<u8> = cd.colors.into();
        let mut png = Vec::new();
        if repng::encode(&mut png, cd.width as _, cd.height as _, &colors).is_ok() {
            use hbb_common::base64::{engine::general_purpose::STANDARD, Engine};
            emit(json!({ "ev": "cursor", "id": cd.id.to_string(), "hotx": cd.hotx, "hoty": cd.hoty, "png": STANDARD.encode(&png) }));
        }
    }
    fn set_cursor_id(&self, id: String) {
        emit(json!({ "ev": "cursor_id", "id": id }));
    }
    fn set_cursor_position(&self, _cp: CursorPosition) {}
    fn set_display(&self, x: i32, y: i32, w: i32, h: i32, _cursor_embedded: bool, _scale: f64) {
        emit(json!({ "ev": "set_display", "x": x, "y": y, "width": w, "height": h }));
    }
    fn switch_display(&self, display: &SwitchDisplay) {
        emit(json!({ "ev": "switch_display", "display": display.display, "width": display.width, "height": display.height }));
    }
    fn set_peer_info(&self, pi: &PeerInfo) {
        let displays: Vec<Value> = pi.displays.iter().map(|d| json!({ "x": d.x, "y": d.y, "width": d.width, "height": d.height })).collect();
        emit(json!({
            "ev": "peer", "username": pi.username, "hostname": pi.hostname, "platform": pi.platform,
            "version": pi.version, "displays": displays, "currentDisplay": pi.current_display,
        }));
    }
    fn set_displays(&self, _displays: &Vec<DisplayInfo>) {}
    /// The peer is a Merlin build (client or Merlin's host mode): it can switch
    /// sides without the hbbs key exchange RustDesk 1.5 requires.
    fn set_platform_additions(&self, data: &str) {
        emit(json!({ "ev": "peer_additions", "merlin": is_merlin_peer(data) }));
    }
    fn on_connected(&self, _conn_type: ConnType) {
        emit(json!({ "ev": "connected" }));
    }
    fn update_privacy_mode(&self) {}
    fn set_permission(&self, name: &str, value: bool) {
        emit(json!({ "ev": "permission", "name": name, "value": value }));
    }
    // First frame arrived: the "connecting" message can go (not a session close).
    fn close_success(&self) {
        emit(json!({ "ev": "first_frame" }));
    }
    fn update_quality_status(&self, _qs: QualityStatus) {}
    fn set_connection_type(&self, is_secured: bool, direct: bool, _stream_type: &str) {
        emit(json!({ "ev": "connection_type", "secured": is_secured, "direct": direct }));
    }
    fn set_fingerprint(&self, _fingerprint: String) {}
    // File transfer session (`"mode":"file"`): results go to Merlin's file panel.
    fn job_error(&self, id: i32, err: String, file_num: i32) {
        emit(json!({ "ev": "fs_error", "job": id, "err": err, "file": file_num }));
    }
    fn job_done(&self, id: i32, file_num: i32) {
        emit(json!({ "ev": "fs_done", "job": id, "file": file_num }));
    }
    fn clear_all_jobs(&self) {}
    fn new_message(&self, msg: String) {
        emit(json!({ "ev": "chat", "text": msg }));
    }
    fn update_transfer_list(&self) {}
    fn load_last_job(&self, _cnt: i32, _job_json: &str, _auto_start: bool) {}
    fn update_folder_files(&self, id: i32, entries: &Vec<FileEntry>, path: String, is_local: bool, _only_count: bool) {
        if id == 0 && !is_local {
            let list: Vec<Value> = entries.iter().map(|e| {
                let kind = match e.entry_type.enum_value() {
                    Ok(FileType::Dir) | Ok(FileType::DirDrive) => "dir",
                    Ok(FileType::DirLink) | Ok(FileType::FileLink) => "symlink",
                    _ => "file",
                };
                json!({ "name": e.name, "type": kind, "size": e.size, "time": e.modified_time, "hidden": e.is_hidden })
            }).collect();
            emit(json!({ "ev": "fs_dir", "path": path, "entries": list }));
        } else if id > 0 {
            // Files of a transfer job: count and total size for the progress bar.
            let total: u64 = entries.iter().map(|e| e.size).sum();
            emit(json!({ "ev": "fs_job", "job": id, "count": entries.len(), "total": total }));
        }
    }
    fn confirm_delete_files(&self, _id: i32, _i: i32, _name: String) {}
    // Merlin asks about existing files before copying: here always overwrite.
    fn override_file_confirm(&self, id: i32, file_num: i32, _to: String, is_upload: bool, _is_identical: bool) {
        if let Some(session) = SESSION.get() {
            session.set_confirm_override_file(id, file_num, true, true, is_upload);
        }
    }
    fn update_block_input_state(&self, _on: bool) {}
    fn job_progress(&self, id: i32, file_num: i32, speed: f64, finished_size: f64) {
        emit(json!({ "ev": "fs_progress", "job": id, "file": file_num, "speed": speed, "done": finished_size }));
    }
    fn adapt_size(&self) {}
    fn on_rgba(&self, _display: usize, rgba: &mut scrap::ImageRgb) {
        self.send_changes(rgba);
    }
    fn msgbox(&self, msgtype: &str, title: &str, text: &str, link: &str, retry: bool) {
        emit(json!({ "ev": "msgbox", "type": msgtype, "title": title, "text": text, "link": link, "retry": retry }));
    }
    fn cancel_msgbox(&self, tag: &str) {
        emit(json!({ "ev": "cancel_msgbox", "tag": tag }));
    }
    /// The peer (connected to us by switching sides) gives control back:
    /// Merlin answers with `switch_sides`, as RustDesk's UI does.
    fn switch_back(&self, _id: &str) {
        emit(json!({ "ev": "switch_back" }));
    }
    fn portable_service_running(&self, _running: bool) {}
    fn on_voice_call_started(&self) {}
    fn on_voice_call_closed(&self, _reason: &str) {}
    fn on_voice_call_waiting(&self) {}
    fn on_voice_call_incoming(&self) {}
    fn get_rgba(&self, _display: usize) -> *const u8 {
        std::ptr::null()
    }
    fn next_rgba(&self, _display: usize) {}
    // Installed client on a PC with several Windows sessions: the server waits
    // for a choice before sending the screen. Pick the session it runs in
    // (the console user's), like the default of RustDesk's dialog.
    fn set_multiple_windows_session(&self, sessions: Vec<WindowsSession>) {
        let list: Vec<Value> = sessions.iter().map(|s| json!({ "sid": s.sid, "name": s.name })).collect();
        emit(json!({ "ev": "windows_sessions", "sessions": list }));
        std::thread::spawn(|| {
            let Some(session) = SESSION.get() else { return };
            let current = session.lc.read().unwrap().peer_info.as_ref().map(|pi| pi.windows_sessions.current_sid);
            if let Some(sid) = current {
                session.send_selected_session_id(sid.to_string());
            }
        });
    }
    fn set_current_display(&self, _disp_idx: i32) {}
    fn update_record_status(&self, _start: bool) {}
    fn printer_request(&self, _id: i32, _path: String) {}
    fn handle_screenshot_resp(&self, _sid: String, _msg: String) {}
    fn handle_terminal_response(&self, _response: TerminalResponse) {}
}

fn str_of<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

fn bool_of(v: &Value, k: &str) -> bool {
    v.get(k).and_then(|x| x.as_bool()).unwrap_or(false)
}

fn is_merlin_peer(platform_additions: &str) -> bool {
    serde_json::from_str::<Value>(platform_additions).is_ok_and(|v| v["merlin"].as_bool() == Some(true))
}

fn app_name_for(_cmd: &str) -> &'static str {
    HOST_APP_NAME
}

/// Code of a switch-sides login (empty or missing: normal login).
fn switch_uuid_of(cmd: &Value) -> Option<String> {
    Some(str_of(cmd, "switchUuid").to_owned()).filter(|u| !u.is_empty())
}

fn job_of(v: &Value) -> i32 {
    v.get("job").and_then(|x| x.as_i64()).unwrap_or(0) as i32
}

/// Viewer sessions use the server Merlin passes, not the one of a local
/// RustDesk service reached over IPC (it may be registered elsewhere, e.g. a
/// retry on another federation server).
static DIRECT_SERVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn direct_server() -> bool {
    DIRECT_SERVER.load(std::sync::atomic::Ordering::Relaxed)
}

/// Server, relay and key come from Merlin's settings.
fn apply_server(cmd: &Value) {
    for (opt, key) in [("custom-rendezvous-server", "server"), ("relay-server", "relay"), ("key", "key"), ("api-server", "api")] {
        Config::set_option(opt.to_owned(), str_of(cmd, key).trim().to_owned());
    }
}

/// Physical key from `KeyboardEvent.code` (map mode, like the Flutter client).
fn send_key(session: &Session<MerlinHandler>, sid: &SessionID, code: &str, down: bool, lock_modes: i32) {
    let key = rdev::chrome_key_from_code(code);
    let Some(usb_hid) = rdev::usb_hid_keycode_from_key(key) else {
        log::debug!("merlin: key without usb hid code: {code}");
        return;
    };
    session.handle_flutter_key_event(*sid, "map", "", usb_hid as _, lock_modes, down);
}

fn handle_command(session: &Session<MerlinHandler>, sid: &SessionID, cmd: &Value) {
    match str_of(cmd, "cmd") {
        "mouse" => {
            let n = |k: &str| cmd.get(k).and_then(|x| x.as_i64()).unwrap_or(0) as i32;
            session.send_mouse(n("mask"), n("x"), n("y"), bool_of(cmd, "alt"), bool_of(cmd, "ctrl"), bool_of(cmd, "shift"), bool_of(cmd, "command"));
        }
        "key" => {
            let locks = cmd.get("lockModes").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
            send_key(session, sid, str_of(cmd, "code"), bool_of(cmd, "down"), locks);
        }
        "text" => session.input_string(str_of(cmd, "text")),
        "login" => session.login(String::new(), String::new(), str_of(cmd, "password").to_owned(), false),
        "chat" => session.send_chat(str_of(cmd, "text").to_owned()),
        "ctrl_alt_del" => session.ctrl_alt_del(),
        "switch_display" => session.switch_display(cmd.get("display").and_then(|x| x.as_i64()).unwrap_or(0) as i32),
        "restart" => session.restart_remote_device(),
        "refresh" => session.refresh_video(-1),
        "reconnect" => session.reconnect(false),
        "close" => session.close(),
        "switch_sides" => {
            log::info!("merlin: switch sides requested to {}", session.get_id());
            let session = session.clone();
            std::thread::spawn(move || session.switch_sides());
        }
        // File transfer (session started with "mode":"file"); paths are remote.
        "fs_list" => session.read_remote_dir(str_of(cmd, "path").to_owned(), true),
        "fs_send" => session.send_files(job_of(cmd), 0, str_of(cmd, "path").to_owned(), str_of(cmd, "to").to_owned(), 0, true, bool_of(cmd, "download")),
        "fs_cancel" => session.cancel_job(job_of(cmd)),
        "fs_mkdir" => session.create_dir(job_of(cmd), str_of(cmd, "path").to_owned(), true),
        "fs_rename" => session.rename_file(job_of(cmd), str_of(cmd, "path").to_owned(), str_of(cmd, "name").to_owned(), true),
        "fs_remove" if bool_of(cmd, "dir") => session.remove_dir(job_of(cmd), str_of(cmd, "path").to_owned(), true),
        "fs_remove" => session.remove_file(job_of(cmd), str_of(cmd, "path").to_owned(), 0, true),
        other => log::debug!("merlin: unknown command {other}"),
    }
}

/// Entry point of `merlin-bridge`.
pub fn run_bridge() {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let Some(Ok(first)) = lines.next() else { return };
    let Ok(connect) = serde_json::from_str::<Value>(&first) else {
        emit(json!({ "ev": "error", "text": "invalid connect command" }));
        return;
    };
    // Host mode: this Merlin receives assistance (own ID and config).
    let host = str_of(&connect, "cmd") == "host";
    // Own ID only: Merlin hides this computer from its online list even when
    // host mode is off, so the ID of the host config is read without starting it.
    let own_id = str_of(&connect, "cmd") == "id";
    *hbb_common::config::APP_NAME.write().unwrap() = app_name_for(str_of(&connect, "cmd")).to_owned();
    if own_id {
        emit(json!({ "ev": "id", "id": hbb_common::config::Config::get_id() }));
        return;
    }
    hbb_common::init_log(false, if host { "merlin-host" } else { "merlin-bridge" });
    if !crate::common::global_init() {
        return;
    }
    if host {
        return run_host(&connect, lines);
    }
    let id = str_of(&connect, "id").replace(' ', "");
    if str_of(&connect, "cmd") != "connect" || id.is_empty() {
        emit(json!({ "ev": "error", "text": "the first command must be connect with an id" }));
        return;
    }
    apply_server(&connect);
    DIRECT_SERVER.store(true, std::sync::atomic::Ordering::Relaxed);

    let session: Session<MerlinHandler> = Session {
        password: str_of(&connect, "password").to_owned(),
        server_keyboard_enabled: Arc::new(RwLock::new(true)),
        server_file_transfer_enabled: Arc::new(RwLock::new(true)),
        server_clipboard_enabled: Arc::new(RwLock::new(true)),
        reconnect_count: Arc::new(AtomicUsize::new(0)),
        ..Default::default()
    };
    let conn_type = if str_of(&connect, "mode") == "file" { ConnType::FILE_TRANSFER } else { ConnType::DEFAULT_CONN };
    let _ = SESSION.set(session.clone());
    session.lc.write().unwrap().initialize(id, conn_type, switch_uuid_of(&connect), bool_of(&connect, "forceRelay"), None, None, None);
    let sid = SessionID::new_v4();
    {
        let session = session.clone();
        std::thread::spawn(move || {
            // After "restart remote device" the connection drops: try again until
            // the machine is back (like the Flutter client does from its UI).
            let mut restarting_since: Option<std::time::Instant> = None;
            loop {
                let round = session.connection_round_state.lock().unwrap().new_round();
                io_loop(session.clone(), round);
                if !session.is_restarting_remote_device() {
                    break;
                }
                let since = *restarting_since.get_or_insert_with(std::time::Instant::now);
                if since.elapsed() > std::time::Duration::from_secs(300) {
                    emit(json!({ "ev": "msgbox", "type": "error", "title": "Restarting remote device", "text": "The remote device did not come back within 5 minutes", "link": "", "retry": false }));
                    break;
                }
                emit(json!({ "ev": "restarting" }));
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
            emit(json!({ "ev": "ended" }));
        });
    }
    for line in lines {
        let Ok(line) = line else { break };
        match serde_json::from_str::<Value>(&line) {
            Ok(cmd) => handle_command(&session, &sid, &cmd),
            Err(e) => log::debug!("merlin: bad command: {e}"),
        }
    }
    // stdin closed: Merlin closed the tab.
    session.close();
}

/// Host mode: Merlin runs the RustDesk server in this process and reports
/// ID, password, status and incoming connections; stdin carries the answers
/// (accept, close, chat, permissions, passwords).
fn run_host(cfg: &Value, lines: impl Iterator<Item = std::io::Result<String>>) {
    use crate::merlin_client as mc;
    apply_server(cfg);
    mc::start_status_sync();
    mc::start_cm(|ev| emit(json!({ "ev": "cm", "data": ev })));
    mc::set_switch_sides_handler(|id, uuid| {
        log::info!("merlin host: switch sides back from {id}");
        emit(json!({ "ev": "switch_connect", "id": id, "uuid": uuid }))
    });
    std::thread::spawn(|| crate::start_server(false, false));
    std::thread::spawn(|| loop {
        emit(json!({
            "ev": "host_info", "id": mc::my_id(), "status": mc::status(),
            "password": mc::temporary_password(), "permanentSet": mc::is_permanent_password_set(),
        }));
        std::thread::sleep(std::time::Duration::from_secs(2));
    });
    for line in lines {
        let Ok(line) = line else { break };
        let Ok(cmd) = serde_json::from_str::<Value>(&line) else { continue };
        let id = cmd.get("id").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
        match str_of(&cmd, "cmd") {
            "new_password" => mc::update_temporary_password(),
            "set_permanent_password" => {
                let ok = mc::set_permanent_password(str_of(&cmd, "password").to_owned());
                emit(json!({ "ev": "permanent_password", "ok": ok }));
            }
            "authorize" => mc::cm_authorize(id),
            "close" => mc::cm_close(id),
            "chat" => mc::cm_send_chat(id, str_of(&cmd, "text").to_owned()),
            "switch_back" => mc::cm_switch_back(id),
            "permission" => mc::cm_switch_permission(id, str_of(&cmd, "name").to_owned(), bool_of(&cmd, "enabled")),
            other => log::debug!("merlin host: unknown command {other}"),
        }
    }
    // stdin closed: Merlin turned assistance off or quit.
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_uses_the_host_config() {
        for cmd in ["connect", "host", "id"] {
            assert_eq!(app_name_for(cmd), HOST_APP_NAME);
        }
    }

    #[test]
    fn merlin_peer_from_platform_additions() {
        assert!(is_merlin_peer(r#"{"merlin":true,"is_installed":false}"#));
        assert!(!is_merlin_peer(r#"{"is_installed":true}"#));
        assert!(!is_merlin_peer(""));
    }

    #[test]
    fn switch_uuid_only_when_given() {
        assert_eq!(switch_uuid_of(&json!({ "switchUuid": "" })), None);
        assert_eq!(switch_uuid_of(&json!({})), None);
        assert_eq!(switch_uuid_of(&json!({ "switchUuid": "u" })), Some("u".to_owned()));
    }
}
