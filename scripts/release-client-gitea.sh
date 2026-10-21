#!/usr/bin/env bash
# Pubblica i client di base Merlin-RustDesk nella release «latest» del repo
# Gitea: Merlin li scarica da lì (Impostazioni → Merlin RustDesk → Scarica i
# Client) con nomi fissi, uno per piattaforma.
#
#   GITEA_TOKEN=xxxx scripts/release-client-gitea.sh client.exe client.AppImage [client.dmg]
#
# Il nome pubblicato dipende dall'estensione: .exe → Merlin-RustDesk.exe,
# .AppImage → Merlin-RustDesk.AppImage, .dmg → Merlin-RustDesk.dmg.
# Un file già presente con lo stesso nome viene sostituito.
set -euo pipefail

GITEA="https://git.difonzo.net"
OWNER="andrea"
REPO="merlin-rustdesk"
: "${GITEA_TOKEN:?imposta GITEA_TOKEN (token con scrittura sul repo)}"
# Percorso attuale del repo (se è stato spostato Gitea reindirizza, ma una POST
# reindirizzata diventa GET: si usa direttamente il nome attuale).
FULL="$(curl -fsSL -H "Authorization: token $GITEA_TOKEN" "$GITEA/api/v1/repos/$OWNER/$REPO" | python3 -c 'import json,sys; print(json.load(sys.stdin)["full_name"])')"
API="$GITEA/api/v1/repos/$FULL"
[ $# -gt 0 ] || { echo "uso: $0 file.exe file.AppImage [file.dmg]" >&2; exit 1; }
auth=(-H "Authorization: token $GITEA_TOKEN")

json() { python3 -c "import json,sys; d=json.load(sys.stdin); $1"; }

release_id() {
  curl -fsSL "${auth[@]}" "$API/releases/tags/latest" 2>/dev/null | json 'print(d["id"])' || true
}

ID="$(release_id)"
if [ -z "$ID" ]; then
  ID="$(curl -fsSL "${auth[@]}" -H 'Content-Type: application/json' \
    -d '{"tag_name":"latest","name":"Client Merlin-RustDesk","body":"Client di base usati da Merlin (Impostazioni → Merlin RustDesk → Scarica i Client).","draft":false,"prerelease":false}' \
    "$API/releases" | json 'print(d["id"])')"
fi

for file in "$@"; do
  case "$file" in
    *.exe) name="Merlin-RustDesk.exe" ;;
    *.AppImage|*.appimage) name="Merlin-RustDesk.AppImage" ;;
    *.dmg) name="Merlin-RustDesk.dmg" ;;
    *) echo "estensione non prevista: $file" >&2; exit 1 ;;
  esac
  old="$(curl -fsSL "${auth[@]}" "$API/releases/$ID/assets" | json "print(next((str(a['id']) for a in d or [] if a['name']=='$name'), ''))")"
  [ -n "$old" ] && curl -fsSL "${auth[@]}" -X DELETE "$API/releases/$ID/assets/$old" >/dev/null
  echo "==> $name ($(du -h "$file" | cut -f1))"
  curl -fsSL "${auth[@]}" -X POST -F "attachment=@$file;filename=$name" "$API/releases/$ID/assets?name=$name" >/dev/null
done
echo "Pubblicati in $GITEA/$FULL/releases/tag/latest"
