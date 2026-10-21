//! Merlin bridge: one RustDesk controlling session driven by Merlin.
//!
//! Merlin starts `merlin-bridge` as a child process.
//! - stdin: one JSON command per line (`connect` first, then input, chat, …);
//! - stdout: framed messages `[kind u8][len u32 LE][payload]`:
//!   - `J`: JSON event (`{"ev": …}`);
//!   - `F`: changed screen area, `[x y w h dw dh: u16 LE][RGBA]`.
//!
//! The session runs in its own config directory (APP_NAME), so it never
//! touches a RustDesk installed on the same machine.

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

const APP_NAME: &str = "MerlinRemote";
const TILE: usize = 64;

static OUT: Mutex<()> = Mutex::new(());

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
    fn set_cursor_data(&self, _cd: CursorData) {}
    fn set_cursor_id(&self, _id: String) {}
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
    fn set_platform_additions(&self, _data: &str) {}
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
    fn job_error(&self, _id: i32, _err: String, _file_num: i32) {}
    fn job_done(&self, _id: i32, _file_num: i32) {}
    fn clear_all_jobs(&self) {}
    fn new_message(&self, msg: String) {
        emit(json!({ "ev": "chat", "text": msg }));
    }
    fn update_transfer_list(&self) {}
    fn load_last_job(&self, _cnt: i32, _job_json: &str, _auto_start: bool) {}
    fn update_folder_files(&self, _id: i32, _entries: &Vec<FileEntry>, _path: String, _is_local: bool, _only_count: bool) {}
    fn confirm_delete_files(&self, _id: i32, _i: i32, _name: String) {}
    fn override_file_confirm(&self, _id: i32, _file_num: i32, _to: String, _is_upload: bool, _is_identical: bool) {}
    fn update_block_input_state(&self, _on: bool) {}
    fn job_progress(&self, _id: i32, _file_num: i32, _speed: f64, _finished_size: f64) {}
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
    fn switch_back(&self, _id: &str) {}
    fn portable_service_running(&self, _running: bool) {}
    fn on_voice_call_started(&self) {}
    fn on_voice_call_closed(&self, _reason: &str) {}
    fn on_voice_call_waiting(&self) {}
    fn on_voice_call_incoming(&self) {}
    fn get_rgba(&self, _display: usize) -> *const u8 {
        std::ptr::null()
    }
    fn next_rgba(&self, _display: usize) {}
    fn set_multiple_windows_session(&self, _sessions: Vec<WindowsSession>) {}
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

/// Server, relay and key come from Merlin's settings.
fn apply_server(cmd: &Value) {
    for (opt, key) in [("custom-rendezvous-server", "server"), ("relay-server", "relay"), ("key", "key")] {
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
        "restart" => session.restart_remote_device(),
        "refresh" => session.refresh_video(-1),
        "reconnect" => session.reconnect(false),
        "close" => session.close(),
        other => log::debug!("merlin: unknown command {other}"),
    }
}

/// Entry point of `merlin-bridge`.
pub fn run_bridge() {
    *hbb_common::config::APP_NAME.write().unwrap() = APP_NAME.to_owned();
    hbb_common::init_log(false, "merlin-bridge");
    if !crate::common::global_init() {
        return;
    }
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let Some(Ok(first)) = lines.next() else { return };
    let Ok(connect) = serde_json::from_str::<Value>(&first) else {
        emit(json!({ "ev": "error", "text": "invalid connect command" }));
        return;
    };
    let id = str_of(&connect, "id").replace(' ', "");
    if str_of(&connect, "cmd") != "connect" || id.is_empty() {
        emit(json!({ "ev": "error", "text": "the first command must be connect with an id" }));
        return;
    }
    apply_server(&connect);

    let session: Session<MerlinHandler> = Session {
        password: str_of(&connect, "password").to_owned(),
        server_keyboard_enabled: Arc::new(RwLock::new(true)),
        server_file_transfer_enabled: Arc::new(RwLock::new(true)),
        server_clipboard_enabled: Arc::new(RwLock::new(true)),
        reconnect_count: Arc::new(AtomicUsize::new(0)),
        ..Default::default()
    };
    session.lc.write().unwrap().initialize(id, ConnType::DEFAULT_CONN, None, bool_of(&connect, "forceRelay"), None, None, None);
    let sid = SessionID::new_v4();
    {
        let session = session.clone();
        std::thread::spawn(move || {
            let round = session.connection_round_state.lock().unwrap().new_round();
            io_loop(session, round);
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
