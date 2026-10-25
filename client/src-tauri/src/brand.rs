// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

// Personalizzazione aggiunta da Merlin in fondo al file del client (vedi
// rustdesk_client.rs in Merlin): [JSON][lunghezza u32 LE]["MERLINRD"] più
// al massimo 7 zeri di allineamento. Senza personalizzazione: Null.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use serde_json::Value;

const MAGIC: &[u8; 8] = b"MERLINRD";
const MAX_LEN: u64 = 4 << 20;

/// Il file da cui leggere: su Linux l'AppImage (l'eseguibile è dentro).
fn own_file() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("APPIMAGE") {
        return Some(PathBuf::from(p));
    }
    std::env::current_exe().ok()
}

pub fn load() -> Value {
    own_file().and_then(|p| read(&p)).unwrap_or(Value::Null)
}

fn read(path: &PathBuf) -> Option<Value> {
    let mut f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let tail_len = size.min(19);
    f.seek(SeekFrom::Start(size - tail_len)).ok()?;
    let mut tail = vec![0u8; tail_len as usize];
    f.read_exact(&mut tail).ok()?;
    let zeros = tail.iter().rev().take(7).take_while(|b| **b == 0).count();
    let end = tail.len() - zeros;
    if end < 12 || &tail[end - 8..end] != MAGIC {
        return None;
    }
    let len = u32::from_le_bytes(tail[end - 12..end - 8].try_into().ok()?) as u64;
    let json_end = size - zeros as u64 - 12;
    if len > MAX_LEN || len > json_end {
        return None;
    }
    f.seek(SeekFrom::Start(json_end - len)).ok()?;
    let mut json = vec![0u8; len as usize];
    f.read_exact(&mut json).ok()?;
    serde_json::from_slice(&json).ok()
}

/// Server del client.
pub struct Server {
    pub host: String,
    pub key: String,
    pub relay: String,
    pub api: String,
    /// «merlin» (Merlin RustDesk Server API) o «betterdesk».
    pub kind: String,
}

/// Server bloccato: quello della personalizzazione di Merlin o, in una versione
/// privata, quello compilato nel client. None: lo sceglie chi usa il client.
pub fn locked_server(brand: &Value) -> Option<Server> {
    if let Some(s) = brand_server(brand) {
        return (!server_editable(brand)).then_some(s);
    }
    let host = env!("MERLIN_RD_SERVER").trim();
    if host.is_empty() {
        return None;
    }
    let kind = env!("MERLIN_RD_TYPE").trim();
    Some(Server {
        host: host.into(),
        key: env!("MERLIN_RD_KEY").trim().into(),
        relay: env!("MERLIN_RD_RELAY").trim().into(),
        api: env!("MERLIN_RD_API").trim().into(),
        kind: if kind.is_empty() { "merlin".into() } else { kind.into() },
    })
}

/// Server della personalizzazione di Merlin, se c'è.
fn brand_server(brand: &Value) -> Option<Server> {
    let s = &brand["server"];
    let get = |k: &str| s[k].as_str().unwrap_or("").trim().to_string();
    if get("host").is_empty() {
        return None;
    }
    let kind = get("type");
    Some(Server { host: get("host"), key: get("key"), relay: get("relay"), api: get("api"), kind: if kind.is_empty() { "merlin".into() } else { kind } })
}

/// Il cliente può cambiare il server della personalizzazione (predefinito: no).
fn server_editable(brand: &Value) -> bool {
    brand["serverEditable"] == Value::Bool(true)
}

/// Server modificabile: quello della personalizzazione è solo il punto di
/// partenza, finché chi usa il client non ne sceglie un altro.
pub fn initial_server(brand: &Value) -> Option<Server> {
    brand_server(brand).filter(|_| server_editable(brand))
}

/// «Controlla un PC» (connessioni verso altri computer): la personalizzazione può toglierlo.
pub fn outgoing_enabled(brand: &Value) -> bool {
    brand["outgoing"] != Value::Bool(false)
}

/// Logo personalizzato (PNG) dal data URL della personalizzazione.
pub fn logo_png(brand: &Value) -> Option<Vec<u8>> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let data = brand["logo"].as_str()?.strip_prefix("data:image/png;base64,")?;
    STANDARD.decode(data).ok()
}

/// Coda della personalizzazione così com'è nel file (JSON, lunghezza, MERLINRD),
/// per ricopiarla nell'eseguibile installato.
pub fn raw_tail(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    let zeros = data.iter().rev().take(7).take_while(|b| **b == 0).count();
    let end = data.len() - zeros;
    if end < 12 || &data[end - 8..end] != MAGIC {
        return None;
    }
    let len = u32::from_le_bytes(data[end - 12..end - 8].try_into().ok()?) as usize;
    (len + 12 <= end).then(|| data[end - 12 - len..end].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn brand(extra: Value) -> Value {
        let mut b = json!({ "server": { "host": "rd.esempio.it", "key": "K", "relay": "", "api": "https://api.esempio.it", "type": "merlin" } });
        b.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        b
    }

    #[test]
    fn merlin_server_is_locked_by_default() {
        let b = brand(json!({}));
        assert_eq!(locked_server(&b).map(|s| s.host).as_deref(), Some("rd.esempio.it"));
        assert!(initial_server(&b).is_none());
    }

    #[test]
    fn editable_server_is_only_the_starting_point() {
        let b = brand(json!({ "serverEditable": true }));
        assert!(locked_server(&b).is_none());
        assert_eq!(initial_server(&b).map(|s| s.host).as_deref(), Some("rd.esempio.it"));
    }

    #[test]
    fn outgoing_connections_can_be_turned_off() {
        assert!(outgoing_enabled(&Value::Null));
        assert!(outgoing_enabled(&brand(json!({}))));
        assert!(!outgoing_enabled(&brand(json!({ "outgoing": false }))));
    }
}
