// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

//! WebView2 su Windows: l'interfaccia ne ha bisogno, e il client è un exe
//! portatile (nessun installer che lo controlli). Su Windows 10/11 c'è quasi
//! sempre; su Windows Server e sulle installazioni ridotte può mancare: in quel
//! caso si scarica il programma di installazione ufficiale di Microsoft e lo si
//! esegue in silenzio, prima di aprire qualsiasi finestra.

use std::os::windows::process::CommandExt;
use std::path::Path;

use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
use windows::{
    core::{w, HSTRING, PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, ERROR_CANCELLED},
        System::{
            Com::CoTaskMemFree,
            Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE},
        },
        UI::{
            Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW},
            WindowsAndMessaging::{MessageBoxW, IDOK, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_OKCANCEL, MB_SETFOREGROUND, MB_TOPMOST, MESSAGEBOX_STYLE, SW_HIDE},
        },
    },
};

/// Programma di installazione «Evergreen Bootstrapper» di Microsoft (circa
/// 2 MB): scarica e installa la versione aggiornata di WebView2.
const BOOTSTRAPPER_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
/// Pagina con il programma di installazione completo, per i server senza internet.
const DOWNLOAD_PAGE: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";

/// Versione di WebView2 installata, se c'è.
fn installed_version() -> Option<String> {
    let mut v = PWSTR::null();
    unsafe {
        GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut v).ok()?;
        if v.is_null() {
            return None;
        }
        let s = v.to_string().ok();
        CoTaskMemFree(Some(v.0 as *const _));
        s.filter(|s| !s.trim().is_empty())
    }
}

fn message(title: &str, text: &str, style: MESSAGEBOX_STYLE) -> bool {
    unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from(title), style | MB_TOPMOST | MB_SETFOREGROUND) == IDOK }
}

/// Controlla WebView2 e, se manca, lo installa. Restituisce `false` se
/// l'interfaccia non può partire (chi chiama esce).
pub fn ensure(title: &str) -> bool {
    if installed_version().is_some() {
        return true;
    }
    let ask = "Per mostrare l'interfaccia serve Microsoft Edge WebView2, un componente di Windows che su questo computer manca.\n\nOra lo scarico da Microsoft e lo installo (circa un minuto). Windows potrebbe chiedere il permesso di amministratore.";
    if !message(title, ask, MB_OKCANCEL | MB_ICONINFORMATION) {
        return false;
    }
    match install() {
        Ok(()) if installed_version().is_some() => true,
        Ok(()) => fail(title, "l'installazione è terminata ma WebView2 non risulta installato"),
        Err(e) => fail(title, &e),
    }
}

fn fail(title: &str, why: &str) -> bool {
    let text = format!(
        "Non è stato possibile installare WebView2: {why}.\n\nSe il computer non ha accesso a internet, scarica il programma di installazione completo («Evergreen Standalone Installer», x64) da un altro computer e installalo qui:\n{DOWNLOAD_PAGE}"
    );
    message(title, &text, MB_OK | MB_ICONERROR);
    false
}

fn install() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("merlin-webview2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("cartella temporanea: {e}"))?;
    let exe = dir.join("MicrosoftEdgeWebview2Setup.exe");
    let r = download(&exe).and_then(|()| run(&exe));
    let _ = std::fs::remove_dir_all(&dir);
    r
}

fn download(to: &Path) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(BOOTSTRAPPER_URL)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("download da Microsoft non riuscito ({e})"))?;
    let bytes = resp.bytes().map_err(|e| format!("download da Microsoft interrotto ({e})"))?;
    // Un exe Windows comincia con «MZ»: altrimenti è arrivata una pagina d'errore (proxy, portale…).
    if bytes.len() < 100_000 || !bytes.starts_with(b"MZ") {
        return Err("il file scaricato da Microsoft non è un programma di installazione".into());
    }
    std::fs::write(to, &bytes).map_err(|e| format!("salvataggio del programma di installazione: {e}"))
}

/// Installazione per tutto il computer (con la richiesta di amministratore);
/// se la richiesta viene rifiutata, solo per l'utente attuale.
fn run(exe: &Path) -> Result<(), String> {
    let file = HSTRING::from(exe.as_os_str());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: w!("/silent /install"),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    match unsafe { ShellExecuteExW(&mut info) } {
        Ok(()) => unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            let mut code = 0u32;
            let _ = GetExitCodeProcess(info.hProcess, &mut code);
            let _ = CloseHandle(info.hProcess);
            check_exit(code)
        },
        Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let status = std::process::Command::new(exe)
                .args(["/silent", "/install"])
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .map_err(|e| format!("avvio del programma di installazione: {e}"))?;
            check_exit(status.code().unwrap_or(-1) as u32)
        }
        Err(e) => Err(format!("avvio del programma di installazione: {e}")),
    }
}

fn check_exit(code: u32) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("il programma di installazione di Microsoft è terminato con il codice 0x{code:08X}"))
    }
}
