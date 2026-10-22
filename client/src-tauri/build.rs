// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

// Server compilato nel client, solo per una versione privata: dalle variabili
// d'ambiente o dal file .env accanto a package.json. Senza (versione pubblica)
// il client chiede il server al primo avvio.
fn main() {
    let mut vals = std::collections::HashMap::new();
    if let Ok(text) = std::fs::read_to_string("../.env") {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                vals.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    for k in ["MERLIN_RD_SERVER", "MERLIN_RD_KEY", "MERLIN_RD_RELAY", "MERLIN_RD_API", "MERLIN_RD_TYPE"] {
        let v = std::env::var(k).ok().or_else(|| vals.get(k).cloned()).unwrap_or_default();
        println!("cargo:rustc-env={k}={v}");
        println!("cargo:rerun-if-env-changed={k}");
    }
    println!("cargo:rerun-if-changed=../.env");
    tauri_build::build()
}
