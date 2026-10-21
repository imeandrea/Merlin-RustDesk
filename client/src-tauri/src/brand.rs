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

/// Server scelto in Merlin (se c'è), altrimenti quello compilato nel client.
pub fn server(brand: &Value) -> (String, String, String) {
    let s = &brand["server"];
    let pick = |k: &str, fallback: &str| s[k].as_str().filter(|v| !v.is_empty()).unwrap_or(fallback).to_string();
    if s["host"].as_str().is_some_and(|h| !h.is_empty()) {
        (pick("host", ""), pick("key", ""), pick("relay", ""))
    } else {
        (env!("MERLIN_RD_SERVER").into(), env!("MERLIN_RD_KEY").into(), env!("MERLIN_RD_RELAY").into())
    }
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
