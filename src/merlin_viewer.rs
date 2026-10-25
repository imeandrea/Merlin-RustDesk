// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

//! Viewer of the Merlin-RustDesk client for switching sides: the technician
//! asked to swap roles, so this client controls the technician's computer.

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

#[derive(Default)]
struct Screen {
    w: usize,
    h: usize,
    prev: Vec<u8>,
}

#[derive(Clone)]
pub struct ViewerHandler {
    screen: Arc<Mutex<Screen>>,
    on_event: EventFn,
    on_tile: TileFn,
}

impl Default for ViewerHandler {
    fn default() -> Self {
        Self { screen: Default::default(), on_event: Arc::new(|_| {}), on_tile: Arc::new(|_| {}) }
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
    fn new_message(&self, _msg: String) {}
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
        if is_login_prompt(msgtype) {
            self.emit(json!({ "ev": "msgbox", "type": "error", "title": title, "text": "Lo scambio dei ruoli non è valido o è scaduto." }));
            if let Some(session) = SESSION.get() {
                session.close();
            }
            return;
        }
        self.emit(json!({ "ev": "msgbox", "type": msgtype, "title": title, "text": text }));
    }
    fn cancel_msgbox(&self, _tag: &str) {}
    /// The technician takes control back: swap again (their Merlin connects
    /// to this computer) and close, as RustDesk's UI does.
    fn switch_back(&self, _id: &str) {
        if let Some(session) = SESSION.get() {
            session.switch_sides();
            self.emit(json!({ "ev": "switched_back" }));
            session.close();
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
    pub fn close(&self) {
        self.session.close();
    }
}

/// Connects to `peer_id` with the code of a pending switch (checked with the
/// local server by RustDesk's login). `None` if the process cannot start RustDesk.
pub fn start_viewer(
    peer_id: String,
    switch_uuid: String,
    on_event: impl Fn(Value) + Send + Sync + 'static,
    on_tile: impl Fn(Vec<u8>) + Send + Sync + 'static,
) -> Option<ViewerHandle> {
    if !crate::common::global_init() {
        return None;
    }
    let on_event: EventFn = Arc::new(on_event);
    let session: Session<ViewerHandler> = Session {
        server_keyboard_enabled: Arc::new(RwLock::new(true)),
        server_file_transfer_enabled: Arc::new(RwLock::new(false)),
        server_clipboard_enabled: Arc::new(RwLock::new(true)),
        reconnect_count: Arc::new(AtomicUsize::new(0)),
        ui_handler: ViewerHandler { screen: Default::default(), on_event: on_event.clone(), on_tile: Arc::new(on_tile) },
        ..Default::default()
    };
    let _ = SESSION.set(session.clone());
    session.lc.write().unwrap().initialize(peer_id, ConnType::DEFAULT_CONN, Some(switch_uuid), false, None, None, None);
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
    fn login_prompts_end_the_viewer() {
        assert!(is_login_prompt("input-password"));
        assert!(is_login_prompt("re-input-password"));
        assert!(!is_login_prompt("error"));
        assert!(!is_login_prompt("success"));
    }
}
