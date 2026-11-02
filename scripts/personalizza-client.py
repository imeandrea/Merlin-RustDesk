#!/usr/bin/env python3
"""Personalizza i client Merlin-RustDesk per Windows (.exe) e Linux (.AppImage)
come fa Merlin desktop (Impostazioni → Client per i clienti), chiedendo i dati
a terminale. Logo e colore restano quelli predefiniti.

Uso:
    python3 personalizza-client.py              # scarica i client da GitHub
    python3 personalizza-client.py FILE [...]   # parte da client già scaricati

Formato (uguale a merlinNewUI/src-tauri/src/rustdesk_client.rs): in fondo al
file [JSON][lunghezza u32 LE]["MERLINRD"]; negli eseguibili Windows firmati i
dati entrano nella tabella dei certificati (allineati a 8), così la firma
resta valida. Solo libreria standard di Python 3.
"""

import json
import os
import re
import struct
import sys
import unicodedata
import urllib.error
import urllib.request

MAGIC = b"MERLINRD"
SECURITY_DIR = 4
RELEASE = "https://github.com/imeandrea/Merlin-RustDesk/releases/latest/download"
ASSETS = {"windows": "Merlin-RustDesk.exe", "linux": "Merlin-RustDesk.AppImage"}
EXT = {"windows": ".exe", "linux": ".AppImage"}
MAX_CONTACTS = 4
KINDS = [
    ("phone", "Telefono", "+39 0123 456789"),
    ("email", "Email", "assistenza@esempio.it"),
    ("web", "Sito", "www.esempio.it"),
    ("whatsapp", "WhatsApp", "+39 333 1234567"),
    ("hours", "Orari", "Lun–Ven 9–18"),
    ("note", "Nota", "Testo breve"),
]


# ---------------------------------------------------------------- formato

def kind(d):
    if d[:2] == b"MZ":
        return "windows"
    if d[:4] == b"\x7fELF":
        return "linux"
    raise ValueError("il file non è un client Merlin-RustDesk per Windows (.exe) o per Linux (.AppImage)")


def u32_at(d, at):
    return struct.unpack_from("<I", d, at)[0] if at + 4 <= len(d) else None


def put_u32(d, at, v):
    struct.pack_into("<I", d, at, v)


def security_dir(d):
    """(posizione del campo dimensione, inizio, dimensione) della tabella dei
    certificati, se l'eseguibile è firmato e la tabella è in fondo al file."""
    pe = u32_at(d, 0x3C)
    if pe is None or d[pe:pe + 4] != b"PE\0\0":
        return None
    opt = pe + 24
    if opt + 2 > len(d):
        return None
    magic = struct.unpack_from("<H", d, opt)[0]
    dirs = {0x10B: opt + 96, 0x20B: opt + 112}.get(magic)
    if dirs is None:
        return None
    entry = dirs + SECURITY_DIR * 8
    start, size = u32_at(d, entry), u32_at(d, entry + 4)
    if start is None or size is None or size == 0 or start + size != len(d):
        return None
    return entry + 4, start, size


def last_certificate(d, start, size):
    at, last = start, None
    while at + 8 <= start + size:
        n = u32_at(d, at)
        if n is None or n < 8:
            break
        last = at
        at += (n + 7) & ~7
    return last


def existing_tail(d):
    zeros = 0
    for b in reversed(d[-7:]):
        if b != 0:
            break
        zeros += 1
    end = len(d) - zeros
    if end < 12 or d[end - 8:end] != MAGIC:
        return None
    n = u32_at(d, end - 12)
    return n + 12 + zeros if n + 12 <= end else None


def personalize(data, js):
    d = bytearray(data)
    k = kind(d)
    old = existing_tail(d)
    if old is not None:  # già personalizzato: si toglie la vecchia
        if k == "windows":
            sd = security_dir(d)
            if sd and old <= sd[2] - 8:
                size_at, start, size = sd
                cert = last_certificate(d, start, size)
                if cert is not None:
                    put_u32(d, cert, max(0, (u32_at(d, cert) or 0) - old))
                put_u32(d, size_at, size - old)
        del d[len(d) - old:]
    tail = bytearray(js) + struct.pack("<I", len(js)) + MAGIC
    if k == "windows":
        sd = security_dir(d)
        if sd:
            size_at, start, size = sd
            tail.extend(b"\0" * (((len(tail) + 7) & ~7) - len(tail)))
            cert = last_certificate(d, start, size)
            if cert is None:
                raise ValueError("firma dell'eseguibile non leggibile")
            cert_len = ((u32_at(d, cert) or 0) + 7) & ~7
            put_u32(d, cert, cert_len + len(tail))
            put_u32(d, size_at, size + len(tail))
    d.extend(tail)
    return bytes(d)


def read_back(d):
    """JSON della personalizzazione, letto come lo legge il client."""
    zeros = 0
    for b in reversed(d[-7:]):
        if b != 0:
            break
        zeros += 1
    end = len(d) - zeros
    if end < 12 or d[end - 8:end] != MAGIC:
        return None
    n = u32_at(d, end - 12)
    return json.loads(d[end - 12 - n:end - 12])


# ---------------------------------------------------------------- domande

def ask(label, default="", required=False, maxlen=None):
    hint = f" [{default}]" if default else ""
    while True:
        v = input(f"{label}{hint}: ").strip() or default
        if required and not v:
            print("  Obbligatorio.")
            continue
        if maxlen and len(v) > maxlen:
            print(f"  Al massimo {maxlen} caratteri.")
            continue
        return v


def ask_yes(label, default):
    d = "S/n" if default else "s/N"
    while True:
        v = input(f"{label} [{d}]: ").strip().lower()
        if not v:
            return default
        if v in ("s", "si", "sì", "y", "yes"):
            return True
        if v in ("n", "no"):
            return False


def ask_choice(label, options, default=1):
    print(label)
    for i, o in enumerate(options, 1):
        print(f"  {i}) {o}")
    while True:
        v = input(f"Scelta [{default}]: ").strip() or str(default)
        if v.isdigit() and 1 <= int(v) <= len(options):
            return int(v) - 1


def api_url(v, server_type):
    s = v.strip().rstrip("/")
    if not s:
        return ""
    if re.match(r"^https?://", s, re.I):
        return s
    return ("http" if server_type == "betterdesk" else "https") + "://" + s


def slug(name):
    s = unicodedata.normalize("NFD", name)
    s = "".join(c for c in s if not unicodedata.combining(c))
    s = re.sub(r"[^A-Za-z0-9]+", "-", s).strip("-")
    return s or "Assistenza"


def ask_brand():
    print("\n— Server —")
    types = ["merlin", "hbbs", "betterdesk"]
    stype = types[ask_choice("Tipo di server:", ["Merlin server", "hbbs (RustDesk semplice, senza API)", "BetterDesk"])]
    host = ask("Server ID (host o IP)", required=True)
    relay = ask("Relay (vuoto: come il server ID)")
    key = ask("Chiave del server", required=True)
    api = ""
    if stype != "hbbs":
        api = api_url(ask("URL del server API (es. https://api.esempio.it)", required=stype == "merlin"), stype)

    print("\n— Client —")
    name = ask("Nome nel client (vuoto: Merlin-RustDesk)", maxlen=40)
    if stype == "merlin":
        assistance = ask_yes("Pulsante «Chiedi assistenza»", True)
    else:
        assistance = False
        print("Pulsante «Chiedi assistenza»: non disponibile con questo tipo di server.")
    outgoing = ask_yes("Il cliente può controllare altri computer («Controlla un PC»)", True)
    editable = ask_yes("Il cliente può cambiare il server", False)

    contacts = []
    print(f"\n— Contatti (fino a {MAX_CONTACTS}, invio vuoto per finire) —")
    labels = [f"{l}" for _, l, _ in KINDS]
    while len(contacts) < MAX_CONTACTS:
        if not ask_yes(f"Aggiungere un contatto ({len(contacts)}/{MAX_CONTACTS})", len(contacts) == 0):
            break
        i = ask_choice("Tipo:", labels, 1 if not contacts else 2)
        value = ask(f"{KINDS[i][1]} (es. {KINDS[i][2]})", maxlen=60)
        if value:
            contacts.append({"kind": KINDS[i][0], "value": value})

    brand = {"contacts": contacts, "assistance": assistance}
    if name:
        brand["name"] = name
    if not outgoing:
        brand["outgoing"] = False
    if editable:
        brand["serverEditable"] = True
    brand["server"] = {"host": host, "relay": relay, "key": key, "api": api, "type": stype}
    brand["v"] = 1
    return brand


# ---------------------------------------------------------------- main

def download(platform):
    url = f"{RELEASE}/{ASSETS[platform]}"
    print(f"Scarico {ASSETS[platform]} da GitHub…")
    try:
        with urllib.request.urlopen(url, timeout=120) as r:
            return r.read()
    except urllib.error.HTTPError as e:
        raise SystemExit(f"GitHub ha risposto {e.code} per {url}")
    except urllib.error.URLError as e:
        raise SystemExit(f"GitHub non raggiungibile: {e.reason}")


def main():
    files = sys.argv[1:]
    if any(a in ("-h", "--help") for a in files):
        print(__doc__)
        return
    sources = []  # (piattaforma, dati)
    if files:
        for f in files:
            with open(f, "rb") as fh:
                data = fh.read()
            try:
                sources.append((kind(data), data))
            except ValueError as e:
                raise SystemExit(f"{f}: {e}")
    else:
        which = ask_choice("Client da personalizzare:", ["Windows e Linux", "Solo Windows", "Solo Linux"])
        platforms = [["windows", "linux"], ["windows"], ["linux"]][which]

    brand = ask_brand()

    if not files:
        sources = [(p, download(p)) for p in platforms]

    out_dir = ask("\nCartella in cui salvare i client", os.getcwd())
    os.makedirs(out_dir, exist_ok=True)
    base = slug(brand.get("name", "")) + "-assistenza"
    js = json.dumps(brand, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    for platform, data in sources:
        out = os.path.join(out_dir, base + EXT[platform])
        result = personalize(data, js)
        if read_back(result) != brand:
            raise SystemExit(f"Verifica fallita per {out}: file non scritto.")
        with open(out, "wb") as fh:
            fh.write(result)
        if platform == "linux":
            os.chmod(out, 0o755)
        print(f"Pronto: {out}")

    s = brand["server"]
    print(f"\nServer ID {s['host']} · relay {s['relay'] or 'come il server ID'} · chiave impostata · API {s['api'] or 'nessuna'}")


if __name__ == "__main__":
    try:
        main()
    except (KeyboardInterrupt, EOFError):
        print("\nAnnullato.")
        sys.exit(1)
