// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

//! Viewer of the Merlin-RustDesk client: controls a remote computer, either
//! after the technician asked to switch sides or on a normal connection
//! started by the user (ID and password).

use std::sync::{atomic::AtomicUsize, Arc, Mutex, OnceLock, RwLock};

use base::message_proto::*;
use hbb_common::{log, rendezvous_proto::ConnType, SessionID};
use serde_json::{json, Value};

use crate::{
    client::*,
    ui_session_interface::{io_loop, InvokeUiSession, Session},
};

const TILE: usize = 64;

type EventFn = Arc<dyn Fn(Value) + Send + Sync>;
type TileFn = Arc<dyn Fn(Vec<u8>) + Send + Sync>;

/// The session, for callbacks that must act on it (switch back).
static SESSION: OnceLock<Session<ViewerHandler>> = OnceLock::new();

/// One changed area: `[x y w h dw dh: u16 LE][RGBA]`, from BGRA rows of `stride` bytes
/// (same format as `merlin-bridge`).
pub fn encode_tile(raw: &[u8], stride: usize, x: usize, y: usize, w: usize, h: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut msg = Vec::with_capacity(12 + w * h * 4);
    for v in [x, y, w, h, dw, dh] {
        msg.extend_from_slice(&(v as u16).to_le_bytes());
    }
    for row in y..y + h {
        let a = row * stride + x * 4;
        for px in raw[a..a + w * 4].chunks_exact(4) {
            msg.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    msg
}

/// The peer asks for a password: the switch code was not accepted (RustDesk
/// falls back to password login). The viewer only exists for switching sides,
/// so it never logs in another way.
pub fn is_login_prompt(msgtype: &str) -> bool {
    msgtype.ends_with("input-password")
}

#[derive(Debug, PartialEq)]
pub enum LoginPrompt {
    /// Switch sides: the code was not accepted, the viewer never logs in another way.
    Close,
    /// Normal connection: the page asks for the password (again, if `wrong`).
    Ask { wrong: bool },
    None,
}

pub fn login_prompt_action(switch: bool, msgtype: &str) -> LoginPrompt {
    if !is_login_prompt(msgtype) {
        LoginPrompt::None
    } else if switch {
        LoginPrompt::Close
    } else {
        LoginPrompt::Ask { wrong: msgtype.starts_with("re-") }
    }
}

/// What the viewer connects with.
pub enum ViewerTarget {
    /// Code of a pending switch (checked with the local server by RustDesk's login).
    Switch(String),
    /// Normal connection; empty: the page asks for it, or the remote user accepts.
    Password(String),
}

#[derive(Default)]
struct Screen {
    w: usize,
    h: usize,
    prev: Vec<u8>,
}

#[derive(Clone)]
pub struct ViewerHandler {
    switch: bool,
    screen: Arc<Mutex<Screen>>,
    on_event: EventFn,
    on_tile: TileFn,
}

impl Default for ViewerHandler {
    fn default() -> Self {
        Self { switch: false, screen: Default::default(), on_event: Arc::new(|_| {}), on_tile: Arc::new(|_| {}) }
    }
}

impl ViewerHandler {
    fn emit(&self, v: Value) {
        (self.on_event)(v)
    }

    /// Sends the tiles that changed since the previous frame (all after a resize).
    fn send_changes(&self, rgba: &scrap::ImageRgb) {
        let (w, h) = (rgba.w, rgba.h);
        let stride = rgba.raw.len() / h.max(1);
        if w == 0 || h == 0 || stride < w * 4 {
            return;
        }
        let mut screen = self.screen.lock().unwrap();
        let resized = screen.w != w || screen.h != h || screen.prev.len() != rgba.raw.len();
        if resized {
            self.emit(json!({ "ev": "display", "width": w, "height": h }));
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
                if changed {
                    (self.on_tile)(encode_tile(&rgba.raw, stride, tx, ty, tw, th, w, h));
                }
            }
        }
        screen.w = w;
        screen.h = h;
        screen.prev.clear();
        screen.prev.extend_from_slice(&rgba.raw);
    }
}

impl InvokeUiSession for ViewerHandler {
    fn set_cursor_data(&self, cd: CursorData) {
        let colors: Vec<u8> = cd.colors.into();
        let mut png = Vec::new();
        if repng::encode(&mut png, cd.width as _, cd.height as _, &colors).is_ok() {
            use hbb_common::base64::{engine::general_purpose::STANDARD, Engine};
            self.emit(json!({ "ev": "cursor", "id": cd.id.to_string(), "hotx": cd.hotx, "hoty": cd.hoty, "png": STANDARD.encode(&png) }));
        }
    }
    fn set_cursor_id(&self, id: String) {
        self.emit(json!({ "ev": "cursor_id", "id": id }));
    }
    fn set_cursor_position(&self, _cp: CursorPosition) {}
    fn set_display(&self, _x: i32, _y: i32, _w: i32, _h: i32, _cursor_embedded: bool, _scale: f64) {}
    fn switch_display(&self, display: &SwitchDisplay) {
        self.emit(json!({ "ev": "switch_display", "display": display.display }));
    }
    fn set_peer_info(&self, pi: &PeerInfo) {
        self.emit(json!({ "ev": "peer", "hostname": pi.hostname, "platform": pi.platform, "displays": pi.displays.len(), "currentDisplay": pi.current_display }));
    }
    fn set_displays(&self, _displays: &Vec<DisplayInfo>) {}
    fn set_platform_additions(&self, _data: &str) {}
    fn on_connected(&self, _conn_type: ConnType) {
        self.emit(json!({ "ev": "connected" }));
    }
    fn update_privacy_mode(&self) {}
    fn set_permission(&self, _name: &str, _value: bool) {}
    fn close_success(&self) {
        self.emit(json!({ "ev": "first_frame" }));
    }
    fn update_quality_status(&self, _qs: QualityStatus) {}
    fn set_connection_type(&self, _is_secured: bool, _direct: bool, _stream_type: &str) {}
    fn set_fingerprint(&self, _fingerprint: String) {}
    fn job_error(&self, _id: i32, _err: String, _file_num: i32) {}
    fn job_done(&self, _id: i32, _file_num: i32) {}
    fn clear_all_jobs(&self) {}
    fn new_message(&self, msg: String) {
        self.emit(json!({ "ev": "chat", "text": msg }));
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
    fn msgbox(&self, msgtype: &str, title: &str, text: &str, _link: &str, _retry: bool) {
        match login_prompt_action(self.switch, msgtype) {
            LoginPrompt::Close => {
                self.emit(json!({ "ev": "msgbox", "type": "error", "title": title, "text": "Lo scambio dei ruoli non è valido o è scaduto." }));
                if let Some(session) = SESSION.get() {
                    session.close();
                }
            }
            LoginPrompt::Ask { wrong } => self.emit(json!({ "ev": "need_password", "wrong": wrong })),
            LoginPrompt::None => self.emit(json!({ "ev": "msgbox", "type": msgtype, "title": title, "text": text })),
        }
    }
    fn cancel_msgbox(&self, _tag: &str) {}
    /// The technician takes control back: swap again (their Merlin connects
    /// to this computer) and close, as RustDesk's UI does.
    fn switch_back(&self, _id: &str) {
        // Called from RustDesk's io loop (a Tokio runtime): `switch_sides` starts
        // its own, so it runs on another thread; close only after it was sent.
        if let Some(session) = SESSION.get().cloned() {
            self.emit(json!({ "ev": "switched_back" }));
            std::thread::spawn(move || {
                session.switch_sides();
                session.close();
            });
        }
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
    fn set_multiple_windows_session(&self, _sessions: Vec<WindowsSession>) {}
    fn set_current_display(&self, _disp_idx: i32) {}
    fn update_record_status(&self, _start: bool) {}
    fn printer_request(&self, _id: i32, _path: String) {}
    fn handle_screenshot_resp(&self, _sid: String, _msg: String) {}
    fn handle_terminal_response(&self, _response: TerminalResponse) {}
}

/// Input of the viewer window, toward the technician's computer.
pub struct ViewerHandle {
    session: Session<ViewerHandler>,
    sid: SessionID,
}

impl ViewerHandle {
    pub fn mouse(&self, mask: i32, x: i32, y: i32, alt: bool, ctrl: bool, shift: bool, command: bool) {
        self.session.send_mouse(mask, x, y, alt, ctrl, shift, command);
    }
    /// Physical key from `KeyboardEvent.code` (map mode, like the Flutter client).
    pub fn key(&self, code: &str, down: bool, lock_modes: i32) {
        let key = rdev::chrome_key_from_code(code);
        let Some(usb_hid) = rdev::usb_hid_keycode_from_key(key) else {
            log::debug!("merlin viewer: key without usb hid code: {code}");
            return;
        };
        self.session.handle_flutter_key_event(self.sid, "map", "", usb_hid as _, lock_modes, down);
    }
    pub fn text(&self, text: &str) {
        self.session.input_string(text);
    }
    pub fn switch_display(&self, display: i32) {
        self.session.switch_display(display);
    }
    /// Swap roles: the remote computer gets to see and control this one (it
    /// connects back to this client's server, which must be running).
    pub fn switch_sides(&self) {
        let session = self.session.clone();
        std::thread::spawn(move || session.switch_sides());
    }
    pub fn chat(&self, text: &str) {
        self.session.send_chat(text.to_owned());
    }
    pub fn ctrl_alt_del(&self) {
        self.session.ctrl_alt_del();
    }
    pub fn restart(&self) {
        self.session.restart_remote_device();
    }
    pub fn login(&self, password: &str) {
        self.session.login(String::new(), String::new(), password.to_owned(), false);
    }
    pub fn close(&self) {
        self.session.close();
    }
}

/// Connects to `peer_id`. `None` if the process cannot start RustDesk.
pub fn start_viewer(
    peer_id: String,
    target: ViewerTarget,
    on_event: impl Fn(Value) + Send + Sync + 'static,
    on_tile: impl Fn(Vec<u8>) + Send + Sync + 'static,
) -> Option<ViewerHandle> {
    if !crate::common::global_init() {
        return None;
    }
    let on_event: EventFn = Arc::new(on_event);
    let (switch_uuid, password) = match target {
        ViewerTarget::Switch(uuid) => (Some(uuid), String::new()),
        ViewerTarget::Password(password) => (None, password),
    };
    let session: Session<ViewerHandler> = Session {
        password,
        server_keyboard_enabled: Arc::new(RwLock::new(true)),
        server_file_transfer_enabled: Arc::new(RwLock::new(false)),
        server_clipboard_enabled: Arc::new(RwLock::new(true)),
        reconnect_count: Arc::new(AtomicUsize::new(0)),
        ui_handler: ViewerHandler { switch: switch_uuid.is_some(), screen: Default::default(), on_event: on_event.clone(), on_tile: Arc::new(on_tile) },
        ..Default::default()
    };
    let _ = SESSION.set(session.clone());
    session.lc.write().unwrap().initialize(peer_id, ConnType::DEFAULT_CONN, switch_uuid, false, None, None, None);
    {
        let session = session.clone();
        std::thread::spawn(move || {
            let round = session.connection_round_state.lock().unwrap().new_round();
            io_loop(session, round);
            on_event(json!({ "ev": "ended" }));
        });
    }
    Some(ViewerHandle { session, sid: SessionID::new_v4() })
}

// ── File: second connection of type FILE_TRANSFER (RustDesk accepts file
// actions only there). It logs in again: the password of the viewer, or the
// remote user accepts; after switching sides the code is already used.

/// Last part of a path, local or remote (`/` or `\`).
pub fn base_name(path: &str) -> String {
    path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("").to_owned()
}

/// `name` inside the remote folder `dir` (Windows separator if the peer is Windows).
pub fn remote_join(dir: &str, name: &str, windows: bool) -> String {
    let sep = if windows { '\\' } else { '/' };
    let dir = dir.trim_end_matches(['/', '\\']);
    if !windows && dir.is_empty() {
        format!("/{name}")
    } else {
        format!("{dir}{sep}{name}")
    }
}

/// The file connection (one at a time; reopened if it ends).
static FILES_SESSION: Mutex<Option<Session<FilesHandler>>> = Mutex::new(None);

#[derive(Clone)]
pub struct FilesHandler {
    on_event: EventFn,
}

impl Default for FilesHandler {
    fn default() -> Self {
        Self { on_event: Arc::new(|_| {}) }
    }
}

impl FilesHandler {
    fn emit(&self, v: Value) {
        (self.on_event)(v)
    }
}

impl InvokeUiSession for FilesHandler {
    fn set_cursor_data(&self, _cd: CursorData) {}
    fn set_cursor_id(&self, _id: String) {}
    fn set_cursor_position(&self, _cp: CursorPosition) {}
    fn set_display(&self, _x: i32, _y: i32, _w: i32, _h: i32, _cursor_embedded: bool, _scale: f64) {}
    fn switch_display(&self, _display: &SwitchDisplay) {}
    fn set_peer_info(&self, pi: &PeerInfo) {
        self.emit(json!({ "ev": "fs_peer", "platform": pi.platform, "home": pi.username }));
    }
    fn set_displays(&self, _displays: &Vec<DisplayInfo>) {}
    fn set_platform_additions(&self, _data: &str) {}
    fn on_connected(&self, _conn_type: ConnType) {
        self.emit(json!({ "ev": "fs_connected" }));
    }
    fn update_privacy_mode(&self) {}
    fn set_permission(&self, _name: &str, _value: bool) {}
    fn close_success(&self) {}
    fn update_quality_status(&self, _qs: QualityStatus) {}
    fn set_connection_type(&self, _is_secured: bool, _direct: bool, _stream_type: &str) {}
    fn set_fingerprint(&self, _fingerprint: String) {}
    fn job_error(&self, id: i32, err: String, file_num: i32) {
        self.emit(json!({ "ev": "fs_error", "job": id, "err": err, "file": file_num }));
    }
    fn job_done(&self, id: i32, file_num: i32) {
        self.emit(json!({ "ev": "fs_done", "job": id, "file": file_num }));
    }
    fn clear_all_jobs(&self) {}
    fn new_message(&self, _msg: String) {}
    fn update_transfer_list(&self) {}
    fn load_last_job(&self, _cnt: i32, _job_json: &str, _auto_start: bool) {}
    fn update_folder_files(&self, id: i32, entries: &Vec<FileEntry>, path: String, is_local: bool, _only_count: bool) {
        if id == 0 && !is_local {
            let list: Vec<Value> = entries
                .iter()
                .map(|e| {
                    let dir = matches!(e.entry_type.enum_value(), Ok(FileType::Dir) | Ok(FileType::DirDrive) | Ok(FileType::DirLink));
                    json!({ "name": e.name, "dir": dir, "size": e.size, "time": e.modified_time, "hidden": e.is_hidden })
                })
                .collect();
            self.emit(json!({ "ev": "fs_dir", "path": path, "entries": list }));
        } else if id > 0 {
            let total: u64 = entries.iter().map(|e| e.size).sum();
            self.emit(json!({ "ev": "fs_job", "job": id, "total": total }));
        }
    }
    fn confirm_delete_files(&self, _id: i32, _i: i32, _name: String) {}
    // Existing file at the destination: overwrite (the page asks first).
    fn override_file_confirm(&self, id: i32, file_num: i32, _to: String, is_upload: bool, _is_identical: bool) {
        if let Some(session) = FILES_SESSION.lock().unwrap().as_ref() {
            session.set_confirm_override_file(id, file_num, true, true, is_upload);
        }
    }
    fn update_block_input_state(&self, _on: bool) {}
    fn job_progress(&self, id: i32, file_num: i32, speed: f64, finished_size: f64) {
        self.emit(json!({ "ev": "fs_progress", "job": id, "file": file_num, "speed": speed, "done": finished_size }));
    }
    fn adapt_size(&self) {}
    fn on_rgba(&self, _display: usize, _rgba: &mut scrap::ImageRgb) {}
    fn msgbox(&self, msgtype: &str, title: &str, text: &str, _link: &str, _retry: bool) {
        match login_prompt_action(false, msgtype) {
            LoginPrompt::Ask { wrong } => self.emit(json!({ "ev": "fs_need_password", "wrong": wrong })),
            _ => self.emit(json!({ "ev": "fs_msgbox", "type": msgtype, "title": title, "text": text })),
        }
    }
    fn cancel_msgbox(&self, _tag: &str) {}
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

/// Files of the remote computer (second connection).
pub struct FilesHandle {
    session: Session<FilesHandler>,
}

impl FilesHandle {
    pub fn list(&self, path: &str) {
        self.session.read_remote_dir(path.to_owned(), true);
    }
    /// Remote file or folder `remote` into the local folder `local_dir`.
    pub fn download(&self, job: i32, remote: &str, local_dir: &str) {
        let to = std::path::Path::new(local_dir).join(base_name(remote)).to_string_lossy().to_string();
        self.session.send_files(job, 0, remote.to_owned(), to, 0, true, true);
    }
    /// Local file or folder `local` into the remote folder `remote_dir`.
    pub fn upload(&self, job: i32, local: &str, remote_dir: &str, windows: bool) {
        let to = remote_join(remote_dir, &base_name(local), windows);
        self.session.send_files(job, 0, local.to_owned(), to, 0, true, false);
    }
    pub fn cancel(&self, job: i32) {
        self.session.cancel_job(job);
    }
    pub fn mkdir(&self, job: i32, path: &str) {
        self.session.create_dir(job, path.to_owned(), true);
    }
    /// `path` gets the new name `name` (same folder).
    pub fn rename(&self, job: i32, path: &str, name: &str) {
        self.session.rename_file(job, path.to_owned(), name.to_owned(), true);
    }
    pub fn remove(&self, job: i32, path: &str, dir: bool) {
        if dir {
            self.session.remove_dir(job, path.to_owned(), true);
        } else {
            self.session.remove_file(job, path.to_owned(), 0, true);
        }
    }
    pub fn login(&self, password: &str) {
        self.session.login(String::new(), String::new(), password.to_owned(), false);
    }
    pub fn close(&self) {
        self.session.close();
    }
}

/// Opens the file connection to `peer_id` (empty password: asked, or accepted remotely).
pub fn start_files(peer_id: String, password: String, on_event: impl Fn(Value) + Send + Sync + 'static) -> FilesHandle {
    let on_event: EventFn = Arc::new(on_event);
    let session: Session<FilesHandler> = Session {
        password,
        server_keyboard_enabled: Arc::new(RwLock::new(false)),
        server_file_transfer_enabled: Arc::new(RwLock::new(true)),
        server_clipboard_enabled: Arc::new(RwLock::new(false)),
        reconnect_count: Arc::new(AtomicUsize::new(0)),
        ui_handler: FilesHandler { on_event: on_event.clone() },
        ..Default::default()
    };
    if let Some(old) = FILES_SESSION.lock().unwrap().replace(session.clone()) {
        old.close();
    }
    session.lc.write().unwrap().initialize(peer_id, ConnType::FILE_TRANSFER, None, false, None, None, None);
    {
        let session = session.clone();
        std::thread::spawn(move || {
            let round = session.connection_round_state.lock().unwrap().new_round();
            io_loop(session, round);
            on_event(json!({ "ev": "fs_ended" }));
        });
    }
    FilesHandle { session }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_is_rgba_with_header() {
        let raw = [1, 2, 3, 4, 5, 6, 7, 8];
        let tile = encode_tile(&raw, 8, 0, 0, 2, 1, 2, 1);
        assert_eq!(&tile[..12], &[0, 0, 0, 0, 2, 0, 1, 0, 2, 0, 1, 0]);
        assert_eq!(&tile[12..], &[3, 2, 1, 255, 7, 6, 5, 255]);
    }

    /// Open-source hbbs has no TCP key exchange: a switch code would make RustDesk
    /// 1.5 wait for one and time out ("Failed to secure tcp"). Merlin builds never send it.
    #[test]
    fn switch_code_is_not_sent_to_the_rendezvous_server() {
        let session: Session<ViewerHandler> = Session::default();
        session.lc.write().unwrap().initialize("merlin-test-switch-code".into(), ConnType::DEFAULT_CONN, Some("u".into()), false, None, None, None);
        assert_eq!(session.get_switch_code(), "");
    }

    #[test]
    fn file_names_and_remote_paths() {
        assert_eq!(base_name("C:\\Users\\a\\doc.txt"), "doc.txt");
        assert_eq!(base_name("/home/a/foto.png"), "foto.png");
        assert_eq!(base_name("/home/a/cartella/"), "cartella");
        assert_eq!(remote_join("C:\\Users\\a", "doc.txt", true), "C:\\Users\\a\\doc.txt");
        assert_eq!(remote_join("C:\\", "doc.txt", true), "C:\\doc.txt");
        assert_eq!(remote_join("/home/a", "doc.txt", false), "/home/a/doc.txt");
        assert_eq!(remote_join("/", "doc.txt", false), "/doc.txt");
    }

    /// RustDesk calls `switch_back` from inside its io loop (a Tokio runtime):
    /// `switch_sides` starts its own runtime and must not run on that thread.
    #[test]
    fn switch_back_inside_the_io_loop_does_not_panic() {
        let _ = SESSION.set(Session::default());
        let rt = hbb_common::tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async { ViewerHandler::default().switch_back("123") });
    }

    #[test]
    fn login_prompts_end_the_viewer() {
        assert!(is_login_prompt("input-password"));
        assert!(is_login_prompt("re-input-password"));
        assert!(!is_login_prompt("error"));
        assert!(!is_login_prompt("success"));
    }

    #[test]
    fn password_is_asked_only_for_a_normal_connection() {
        assert_eq!(login_prompt_action(true, "input-password"), LoginPrompt::Close);
        assert_eq!(login_prompt_action(false, "input-password"), LoginPrompt::Ask { wrong: false });
        assert_eq!(login_prompt_action(false, "re-input-password"), LoginPrompt::Ask { wrong: true });
        assert_eq!(login_prompt_action(false, "error"), LoginPrompt::None);
    }
}
