// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

// Aggiornamento del client dalle GitHub Releases del repo pubblico.

use serde_json::Value;

/// Chiave pubblica degli aggiornamenti (la privata è solo sul Mac: secrets/client-updater.key).
pub const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDM0OTZDNkRGMENEMDlGRjEKUldUeG45QU0zOGFXTktEb1pYL1BnMWJGTG1HQ2g1bEphZHl0Mm5wYzZsQlAxcDRJeExLR3JpYzQK";

#[derive(Clone, Debug)]
pub struct Update {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub signature: String,
}

fn parts(v: &str) -> Option<Vec<u64>> {
    v.trim().trim_start_matches('v').split(['.', '-', '+']).take(3).map(|p| p.parse().ok()).collect()
}

/// `latest` è più recente di `current` (x.y.z; una versione non leggibile non lo è mai).
pub fn is_newer(current: &str, latest: &str) -> bool {
    match (parts(current), parts(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

/// Chiave della piattaforma nel feed (come quello di Tauri).
pub fn platform_key() -> &'static str {
    if cfg!(windows) {
        "windows-x86_64"
    } else if cfg!(target_os = "macos") {
        "darwin-aarch64"
    } else {
        "linux-x86_64"
    }
}

/// L'aggiornamento per `platform` nel feed `latest.json`.
pub fn pick(feed: &Value, platform: &str) -> Option<Update> {
    let p = &feed["platforms"][platform];
    Some(Update {
        version: feed["version"].as_str()?.to_owned(),
        notes: feed["notes"].as_str().unwrap_or("").to_owned(),
        url: p["url"].as_str()?.to_owned(),
        signature: p["signature"].as_str()?.to_owned(),
    })
}

/// Il file nuovo con in fondo la personalizzazione del vecchio (nome, logo, server).
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn with_tail(mut data: Vec<u8>, tail: Option<Vec<u8>>) -> Vec<u8> {
    if let Some(t) = tail {
        data.extend_from_slice(&t);
    }
    data
}

/// Firma del file scaricato (formato di `tauri signer`: base64 del testo minisign).
pub fn verify(data: &[u8], signature_b64: &str, public_key_b64: &str) -> Result<(), String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let text = |b64: &str| STANDARD.decode(b64.trim()).ok().and_then(|b| String::from_utf8(b).ok()).ok_or("formato non valido");
    let pk = minisign_verify::PublicKey::decode(&text(public_key_b64)?).map_err(|e| format!("chiave: {e}"))?;
    let sig = minisign_verify::Signature::decode(&text(signature_b64)?).map_err(|e| format!("firma: {e}"))?;
    pk.verify(data, &sig, true).map_err(|e| format!("firma non valida: {e}"))
}

/// Feed dell'ultima versione pubblicata (repo pubblico, senza token).
const FEED_URL: &str = "https://github.com/imeandrea/Merlin-RustDesk/releases/latest/download/latest.json";

/// Versione privata compilata (server nel binario): non si aggiorna mai,
/// diventerebbe la pubblica e perderebbe il server.
pub fn enabled() -> bool {
    env!("MERLIN_RD_SERVER").trim().is_empty()
}

fn http() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("Merlin-RustDesk/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())
}

/// C'è una versione più recente per questa piattaforma?
pub fn check() -> Result<Option<Update>, String> {
    if !enabled() {
        return Ok(None);
    }
    let feed: Value = http()?.get(FEED_URL).send().and_then(|r| r.error_for_status()).and_then(|r| r.json()).map_err(|e| e.to_string())?;
    Ok(pick(&feed, platform_key()).filter(|u| is_newer(env!("CARGO_PKG_VERSION"), &u.version)))
}

fn download(u: &Update) -> Result<Vec<u8>, String> {
    let data = http()?.get(&u.url).send().and_then(|r| r.error_for_status()).and_then(|r| r.bytes()).map_err(|e| format!("scaricamento: {e}"))?;
    verify(&data, &u.signature, PUBLIC_KEY)?;
    Ok(data.to_vec())
}

/// Il file in esecuzione (su Linux l'AppImage, se si parte da lì).
fn own_file() -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "linux")]
    if let Some(p) = std::env::var_os("APPIMAGE") {
        return Ok(p.into());
    }
    std::env::current_exe().map_err(|e| e.to_string())
}

/// Sostituisce `cur` con `data` (rinominando il file in uso, possibile anche su
/// Windows) e avvia la copia nuova; il vecchio resta come `.old` fino al prossimo avvio.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn swap_and_restart(cur: &std::path::Path, data: &[u8]) -> Result<(), String> {
    let new = cur.with_extension("update-new");
    let old = cur.with_extension("update-old");
    std::fs::write(&new, data).map_err(|e| format!("scrittura: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::fs::remove_file(&old);
    std::fs::rename(cur, &old).map_err(|e| format!("sostituzione: {e}"))?;
    if let Err(e) = std::fs::rename(&new, cur) {
        let _ = std::fs::rename(&old, cur);
        return Err(format!("sostituzione: {e}"));
    }
    std::process::Command::new(cur).spawn().map_err(|e| format!("riavvio: {e}"))?;
    Ok(())
}

/// Copie vecchie lasciate da un aggiornamento (all'avvio).
pub fn cleanup() {
    if let Ok(cur) = own_file() {
        let _ = std::fs::remove_file(cur.with_extension("update-old"));
    }
}

/// Scarica, verifica e installa `u`; al ritorno la finestra va chiusa (parte la versione nuova).
pub fn install(u: &Update) -> Result<(), String> {
    let data = download(u)?;
    let cur = own_file()?;
    #[cfg(target_os = "macos")]
    {
        let _ = &cur;
        return install_macos(&data);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let data = with_tail(data, crate::brand::raw_tail(&cur));
        #[cfg(windows)]
        if installed_windows(&cur) {
            // Come «Installa»: copia temporanea lanciata con --merlin-install (chiede i permessi).
            let dir = std::env::temp_dir().join("MerlinRustDesk-update");
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let staged = dir.join("MerlinRustDesk.exe");
            std::fs::write(&staged, &data).map_err(|e| e.to_string())?;
            std::process::Command::new(&staged).arg("--merlin-install").spawn().map_err(|e| e.to_string())?;
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        if std::env::var_os("APPIMAGE").is_none() {
            // Client installato in /usr/lib: si reinstalla dall'AppImage nuova (pkexec).
            let staged = std::env::temp_dir().join("merlinrustdesk-update.AppImage");
            std::fs::write(&staged, &data).map_err(|e| e.to_string())?;
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
            }
            let res = crate::linux_install::install_from(&staged.to_string_lossy());
            let _ = std::fs::remove_file(&staged);
            res?;
            crate::linux_install::open_installed();
            return Ok(());
        }
        swap_and_restart(&cur, &data)
    }
}

/// Eseguibile installato con «Installa» (in Program Files), non la copia portatile.
#[cfg(windows)]
fn installed_windows(cur: &std::path::Path) -> bool {
    std::env::var_os("ProgramFiles").is_some_and(|pf| cur.starts_with(pf))
}

/// macOS: `data` è l'app compressa (.app.tar.gz, firmata e notarizzata da Apple);
/// sostituisce il bundle in uso e la riapre.
#[cfg(target_os = "macos")]
fn install_macos(data: &[u8]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bundle = exe.ancestors().nth(3).ok_or("app non trovata")?.to_path_buf();
    let tmp = std::env::temp_dir().join("merlinrustdesk-update");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let archive = tmp.join("app.tar.gz");
    std::fs::write(&archive, data).map_err(|e| e.to_string())?;
    let ok = std::process::Command::new("tar").arg("-xzf").arg(&archive).arg("-C").arg(&tmp).status().map_err(|e| e.to_string())?.success();
    let new = std::fs::read_dir(&tmp).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "app")).ok_or("app non trovata nell'archivio")?;
    if !ok {
        return Err("archivio non valido".into());
    }
    let old = tmp.join("old.app");
    std::fs::rename(&bundle, &old).map_err(|e| format!("sostituzione: {e}"))?;
    if let Err(e) = std::fs::rename(&new, &bundle) {
        let _ = std::fs::rename(&old, &bundle);
        return Err(format!("sostituzione: {e}"));
    }
    std::process::Command::new("open").arg("-n").arg(&bundle).spawn().map_err(|e| format!("riavvio: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_PUB: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IENBMjk4RUIzMjhFQ0VGQjcKUldTMzcrd29zNDRweW9oWm5wbFMweUo3QmFDUVJuZXNLN2Z1QVMxU2FVVE9RNXNZT09LeTJ2MEwK";
    const TEST_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTMzcrd29zNDRweXI2K2hqTms1dUhUMHRHZkdoQUFlanRNY2ErQVNVM29tT044OFhPMHdDcVFBYS9IYlJ4TEF4c0FFYnBXRS9MZnNsRHZjRlFKK2Q0TE1zVjhqNmY2TlFvPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkyOTI0Mzc5CWZpbGU6ZGF0YS5iaW4KY3R3QmUrWGNGUHNaYVViVGN2S2t4L3IwYXhWSnlsbWUybE1YbTRsZVd4c1lLTnlGbUU1a0pGU3ZSMFBwWVBKU2xPM28rZ3BVcndMTHdWeUlYOHAzQnc9PQo=";

    #[test]
    fn newer_versions() {
        assert!(is_newer("0.3.0", "0.3.1"));
        assert!(is_newer("0.3.9", "0.10.0"));
        assert!(is_newer("0.3.0", "v0.4.0"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.3.1", "0.3.0"));
        assert!(!is_newer("0.3.0", "rotto"));
    }

    #[test]
    fn update_for_this_platform() {
        let feed = json!({ "version": "0.4.0", "notes": "Novità", "platforms": {
            "windows-x86_64": { "url": "https://x/Merlin-RustDesk.exe", "signature": "S" } } });
        let u = pick(&feed, "windows-x86_64").unwrap();
        assert_eq!((u.version.as_str(), u.url.as_str(), u.signature.as_str(), u.notes.as_str()), ("0.4.0", "https://x/Merlin-RustDesk.exe", "S", "Novità"));
        assert!(pick(&feed, "linux-x86_64").is_none());
    }

    #[test]
    fn personalization_follows_the_new_file() {
        assert_eq!(with_tail(b"NUOVO".to_vec(), Some(b"CODA".to_vec())), b"NUOVOCODA");
        assert_eq!(with_tail(b"NUOVO".to_vec(), None), b"NUOVO");
    }

    #[test]
    fn the_release_key_is_valid() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let text = String::from_utf8(STANDARD.decode(PUBLIC_KEY).unwrap()).unwrap();
        assert!(minisign_verify::PublicKey::decode(&text).is_ok());
    }

    #[test]
    fn signature_is_checked() {
        assert!(verify(b"ciao merlin", TEST_SIG, TEST_PUB).is_ok());
        assert!(verify(b"ciao merlin!", TEST_SIG, TEST_PUB).is_err());
        assert!(verify(b"ciao merlin", TEST_SIG, PUBLIC_KEY).is_err());
    }
}
