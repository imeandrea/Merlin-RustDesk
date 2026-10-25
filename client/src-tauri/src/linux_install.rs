// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

// «Avvia con il computer» su Linux, dall'AppImage: il client viene estratto
// in /usr/lib/merlinrustdesk (il nucleo RustDesk considera installato un
// eseguibile sotto /usr) con la personalizzazione in fondo all'eseguibile, e
// gira come servizio systemd `merlinrustdesk` (come il pacchetto di RustDesk).
// I comandi da amministratore passano da pkexec (finestra della password del
// sistema).

use std::{io::Write, path::Path, process::Command};

const DIR: &str = "/usr/lib/merlinrustdesk";
const BIN: &str = "/usr/lib/merlinrustdesk/app/usr/bin/merlin-rustdesk-client";

const UNIT: &str = "[Unit]
Description=Merlin-RustDesk
Requires=network.target
After=systemd-user-sessions.service

[Service]
Type=simple
ExecStart=/usr/lib/merlinrustdesk/app/usr/bin/merlin-rustdesk-client --service
PIDFile=/run/merlinrustdesk.pid
KillMode=mixed
TimeoutStopSec=30
User=root
LimitNOFILE=100000

[Install]
WantedBy=multi-user.target
";

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn run_root(script: &str) -> Result<(), String> {
    let out = Command::new("pkexec")
        .args(["sh", "-c", script])
        .output()
        .map_err(|e| format!("pkexec non disponibile: {e}"))?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(126) | Some(127) => Err("Autorizzazione annullata".into()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

pub fn install() -> Result<(), String> {
    let appimage = std::env::var("APPIMAGE").map_err(|_| "Si installa dall'AppImage del client".to_string())?;
    install_from(&appimage)
}

/// Installa (o aggiorna) dall'AppImage `appimage`, con la sua personalizzazione.
pub fn install_from(appimage: &str) -> Result<(), String> {
    let appimage = appimage.to_owned();
    // Personalizzazione: la stessa coda dell'AppImage, aggiunta all'eseguibile installato.
    let tail_file = std::env::temp_dir().join("merlinrustdesk-brand.tail");
    let tail = crate::brand::raw_tail(Path::new(&appimage)).unwrap_or_default();
    std::fs::File::create(&tail_file)
        .and_then(|mut f| f.write_all(&tail))
        .map_err(|e| e.to_string())?;
    let script = format!(
        r#"set -e
T=$(mktemp -d)
cp {app} "$T/client.AppImage"
chmod 755 "$T/client.AppImage"
cd "$T" && ./client.AppImage --appimage-extract >/dev/null
cat {tail} >> "$T/squashfs-root/usr/bin/merlin-rustdesk-client"
systemctl stop merlinrustdesk 2>/dev/null || true
rm -rf {dir}
mkdir -p {dir}
mv "$T/squashfs-root" {dir}/app
chmod -R u+rwX,go+rX,go-w {dir}
rm -rf "$T"
cat > /etc/systemd/system/merlinrustdesk.service <<'UNIT_EOF'
{unit}UNIT_EOF
cat > /usr/share/applications/merlinrustdesk.desktop <<'DESKTOP_EOF'
[Desktop Entry]
Type=Application
Name=Merlin-RustDesk
Exec={bin}
Icon={dir}/app/Merlin-RustDesk.png
Terminal=false
Categories=Network;RemoteAccess;
DESKTOP_EOF
systemctl daemon-reload
systemctl enable merlinrustdesk
systemctl restart merlinrustdesk
"#,
        app = quote(&appimage),
        tail = quote(&tail_file.to_string_lossy()),
        dir = DIR,
        bin = BIN,
        unit = UNIT,
    );
    let res = run_root(&script);
    let _ = std::fs::remove_file(&tail_file);
    res
}

pub fn uninstall() -> Result<(), String> {
    run_root(&format!(
        "systemctl disable --now merlinrustdesk 2>/dev/null || true
rm -f /etc/systemd/system/merlinrustdesk.service /usr/share/applications/merlinrustdesk.desktop
systemctl daemon-reload
pkill -f {dir}/ || true
rm -rf {dir}
",
        dir = DIR
    ))
}

/// Dopo l'installazione: apre il client installato (fra qualche secondo,
/// quando questa copia si è chiusa).
pub fn open_installed() {
    // Senza le variabili dell'AppImage: il client installato è un'altra copia.
    let _ = Command::new("sh")
        .args(["-c", &format!("sleep 2; exec {BIN}")])
        .env_remove("APPIMAGE")
        .env_remove("APPDIR")
        .env_remove("OWD")
        .env_remove("ARGV0")
        .env_remove("LD_LIBRARY_PATH")
        .spawn();
}

/// Client installato (AppImage estratta): prepara l'ambiente che l'AppImage
/// imposta da sola (AppRun): cartella di lavoro `usr` (WebKit cerca i suoi
/// processi in `./lib/...`) e variabili di GTK. Va chiamata per prima.
pub fn prepare_extracted_env() {
    if std::env::var_os("APPIMAGE").is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(usr) = exe.parent().and_then(|b| b.parent()) else { return };
    let Some(appdir) = usr.parent() else { return };
    if !appdir.join("AppRun").exists() || !usr.join("lib").exists() {
        return;
    }
    let a = appdir.to_string_lossy().to_string();
    let data = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    let vars = [
        ("APPDIR", a.clone()),
        ("GTK_DATA_PREFIX", a.clone()),
        ("GTK_THEME", std::env::var("GTK_THEME").unwrap_or_else(|_| "Adwaita".into())),
        ("XDG_DATA_DIRS", format!("{a}/usr/share:{data}")),
        ("GSETTINGS_SCHEMA_DIR", format!("{a}/usr/share/glib-2.0/schemas")),
        ("GI_TYPELIB_PATH", format!("{a}/usr/lib/girepository-1.0")),
        ("GIO_MODULE_DIR", format!("{a}/usr/lib/gio/modules")),
        ("GTK_EXE_PREFIX", format!("{a}/usr")),
        ("GTK_PATH", format!("{a}/usr/lib/gtk-3.0")),
        ("GTK_IM_MODULE_FILE", format!("{a}/usr/lib/gtk-3.0/3.0.0/immodules.cache")),
        ("GDK_PIXBUF_MODULE_FILE", format!("{a}/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache")),
    ];
    for (k, v) in vars {
        std::env::set_var(k, v);
    }
    let _ = std::env::set_current_dir(usr);
}
