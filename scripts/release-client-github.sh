#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Pubblica i client di base Merlin-RustDesk nelle GitHub Releases del repo
# pubblico (sorgente e programmi nello stesso posto, come chiede la AGPL).
# Merlin li scarica da .../releases/latest/download/<nome> (senza token).
#
#   GITHUB_TOKEN=xxxx scripts/release-client-github.sh <versione> client.exe client.AppImage [client.dmg]
#
# Crea (o riusa) la release v<versione> e vi carica i file con nomi fissi:
# Merlin-RustDesk.exe, Merlin-RustDesk.AppImage, Merlin-RustDesk.dmg.
# Un file già presente con lo stesso nome viene sostituito.
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
for file in "$@"; do
  case "$file" in
    *.exe) name="Merlin-RustDesk.exe" ;;
    *.AppImage|*.appimage) name="Merlin-RustDesk.AppImage" ;;
    *.dmg) name="Merlin-RustDesk.dmg" ;;
    *) echo "estensione non prevista: $file" >&2; exit 1 ;;
  esac
  old="$(curl -fsS "${auth[@]}" "$API/releases/$ID/assets?per_page=100" | json "print(next((str(a['id']) for a in d if a['name']=='$name'), ''))")"
  [ -n "$old" ] && curl -fsS "${auth[@]}" -X DELETE "$API/releases/assets/$old" >/dev/null
  echo "==> $name ($(du -h "$file" | cut -f1))"
  curl -fsS "${auth[@]}" -H "Content-Type: application/octet-stream" --data-binary "@$file" "$UPLOAD?name=$name" >/dev/null
done
# Pubblicata (non più bozza) e indicata come ultima: è quella di releases/latest.
curl -fsS "${auth[@]}" -X PATCH "$API/releases/$ID" -d '{"draft": false, "make_latest": "true"}' >/dev/null
echo "Pubblicati in https://github.com/$REPO/releases/tag/v$VERSION"
