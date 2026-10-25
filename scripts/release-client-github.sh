#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Pubblica i client di base Merlin-RustDesk nelle GitHub Releases del repo
# pubblico (sorgente e programmi nello stesso posto, come chiede la AGPL).
# Merlin li scarica da .../releases/latest/download/<nome> (senza token).
#
#   GITHUB_TOKEN=xxxx scripts/release-client-github.sh <versione> client.exe client.AppImage [client.dmg] [client.app.tar.gz]
#
# Crea (o riusa) la release v<versione> e vi carica i file con nomi fissi:
# Merlin-RustDesk.exe, Merlin-RustDesk.AppImage, Merlin-RustDesk.dmg,
# Merlin-RustDesk.app.tar.gz. Un file già presente con lo stesso nome viene
# sostituito. Per l'aggiornamento automatico del client firma exe, AppImage e
# app.tar.gz con secrets/client-updater.key (solo sul Mac) e carica latest.json.
set -euo pipefail

REPO="imeandrea/Merlin-RustDesk"
API="https://api.github.com/repos/$REPO"
: "${GITHUB_TOKEN:?imposta GITHUB_TOKEN (token con scrittura sui contenuti del repo)}"
VERSION="${1:?uso: $0 <versione> file.exe file.AppImage [file.dmg]}"; shift
[ $# -gt 0 ] || { echo "nessun file da pubblicare" >&2; exit 1; }
auth=(-H "Authorization: Bearer $GITHUB_TOKEN" -H "Accept: application/vnd.github+json")
json() { python3 -c "import json,sys; d=json.load(sys.stdin); $1"; }

# Release già pubblicata (tag) o bozza con lo stesso nome; altrimenti una bozza nuova.
ID="$(curl -fsS "${auth[@]}" "$API/releases/tags/v$VERSION" 2>/dev/null | json 'print(d["id"])' 2>/dev/null || true)"
[ -n "$ID" ] || ID="$(curl -fsS "${auth[@]}" "$API/releases?per_page=100" | json "print(next((str(r['id']) for r in d if r['name']=='Merlin-RustDesk $VERSION'), ''))")"
if [ -z "$ID" ]; then
  BODY="$(VERSION="$VERSION" python3 -c 'import json,os; v=os.environ["VERSION"]; print(json.dumps({"tag_name": "v"+v, "target_commitish": "main", "name": "Merlin-RustDesk "+v, "body": "Client per i clienti Merlin-RustDesk "+v+" (versione modificata di RustDesk, AGPL-3.0). Sorgente: questo repository.", "draft": True}))')"
  ID="$(curl -fsS "${auth[@]}" -X POST "$API/releases" -d "$BODY" | json 'print(d["id"])')"
fi
UPLOAD="https://uploads.github.com/repos/$REPO/releases/$ID/assets"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KEY="$ROOT/secrets/client-updater.key"
[ -r "$KEY" ] || { echo "manca $KEY (chiave degli aggiornamenti)" >&2; exit 1; }
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
FEED='{}'
sign() {  # firma minisign (formato tauri) di $1, stampata in base64; si ferma se fallisce
  local abs; abs="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
  ( cd "$ROOT/client" && TAURI_SIGNING_PRIVATE_KEY_PATH="$KEY" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" npx --no-install tauri signer sign "$abs" >/dev/null ) || return 1
  [ -s "$abs.sig" ] || return 1
  cat "$abs.sig"; rm -f "$abs.sig"
}
upload() {  # $1 file, $2 nome nella release
  old="$(curl -fsS "${auth[@]}" "$API/releases/$ID/assets?per_page=100" | json "print(next((str(a['id']) for a in d if a['name']=='$2'), ''))")"
  [ -n "$old" ] && curl -fsS "${auth[@]}" -X DELETE "$API/releases/assets/$old" >/dev/null
  echo "==> $2 ($(du -h "$1" | cut -f1))"
  curl -fsS "${auth[@]}" -H "Content-Type: application/octet-stream" --data-binary "@$1" "$UPLOAD?name=$2" >/dev/null
}
for file in "$@"; do
  platform=""
  case "$file" in
    *.exe) name="Merlin-RustDesk.exe"; platform="windows-x86_64" ;;
    *.AppImage|*.appimage) name="Merlin-RustDesk.AppImage"; platform="linux-x86_64" ;;
    *.dmg) name="Merlin-RustDesk.dmg" ;;
    *.app.tar.gz) name="Merlin-RustDesk.app.tar.gz"; platform="darwin-aarch64" ;;
    *) echo "estensione non prevista: $file" >&2; exit 1 ;;
  esac
  if [ -n "$platform" ]; then
    sig="$(sign "$file")" || { echo "firma non riuscita: $file" >&2; exit 1; }
    FEED="$(FEED="$FEED" P="$platform" U="https://github.com/$REPO/releases/download/v$VERSION/$name" S="$sig" python3 -c 'import json,os; d=json.loads(os.environ["FEED"]); d[os.environ["P"]]={"url":os.environ["U"],"signature":os.environ["S"]}; print(json.dumps(d))')"
  fi
  upload "$file" "$name"
done
# Feed dell'aggiornamento automatico (il client legge releases/latest/download/latest.json).
FEED="$FEED" VERSION="$VERSION" python3 -c 'import json,os; print(json.dumps({"version": os.environ["VERSION"], "notes": "", "platforms": json.loads(os.environ["FEED"])}, indent=2))' > "$WORK/latest.json"
upload "$WORK/latest.json" "latest.json"
# Pubblicata (non più bozza) e indicata come ultima: è quella di releases/latest.
curl -fsS "${auth[@]}" -X PATCH "$API/releases/$ID" -d '{"draft": false, "make_latest": "true"}' >/dev/null
echo "Pubblicati in https://github.com/$REPO/releases/tag/v$VERSION"
