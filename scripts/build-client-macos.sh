#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Build del client Merlin-RustDesk per macOS sul Mac, firmato e notarizzato.
#   scripts/build-client-macos.sh pubblico|privato
# pubblico: nessun server compilato (MERLIN_RD_PUBLIC=1); privato: server,
# chiave e URL da client/.env (solo per l'utente, mai pubblicato).
# Notarizzazione: APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID da .env.notary.
# Il .dmg finisce in ../build-rustdesk-client/<versione>-<variante>/.
set -euo pipefail

VARIANT="${1:?uso: $0 pubblico|privato}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(node -p "require('$ROOT/client/package.json').version")"
case "$VARIANT" in
  pubblico) export MERLIN_RD_PUBLIC=1; SUFFIX="" ;;
  privato)
    unset MERLIN_RD_PUBLIC; SUFFIX="_privato"
    grep -q '^MERLIN_RD_SERVER=..*' "$ROOT/client/.env" || { echo "client/.env senza MERLIN_RD_SERVER" >&2; exit 1; } ;;
  *) echo "variante: pubblico o privato" >&2; exit 1 ;;
esac
[ -r "$ROOT/.env.notary" ] || { echo "manca .env.notary (credenziali di notarizzazione)" >&2; exit 1; }
set -a; . "$ROOT/.env.notary"; set +a
export APPLE_SIGNING_IDENTITY="Developer ID Application: andrea di fonzo (87VS9Y893U)" CI=true
# LTO thin: con la LTO piena le due versioni di muda (Tauri e tray di RustDesk)
# definiscono la stessa classe Objective-C e il link fallisce.
export CARGO_PROFILE_RELEASE_LTO=thin

cd "$ROOT/client"
npm ci
npx tauri build --bundles app,dmg
DMG="$ROOT/target/release/bundle/dmg/Merlin-RustDesk_${VERSION}_aarch64.dmg"
OUT="$ROOT/../build-rustdesk-client/$VERSION-$VARIANT"
mkdir -p "$OUT"
cp "$DMG" "$OUT/Merlin-RustDesk_${VERSION}${SUFFIX}_aarch64.dmg"
APP="$ROOT/target/release/bundle/macos/Merlin-RustDesk.app"
spctl -a -vv "$APP"
# Il server privato deve esserci solo nella versione privata (valore mai stampato).
SRV="$(sed -n 's/^MERLIN_RD_SERVER=//p' "$ROOT/client/.env" 2>/dev/null | tr -d '[:space:]')"
if [ -n "$SRV" ]; then
  found="$(LC_ALL=C grep -c -a -F -- "$SRV" "$APP/Contents/MacOS/merlin-rustdesk-client" || true)"
  if [ "$VARIANT" = pubblico ] && [ "$found" != 0 ]; then echo "ERRORE: server privato nella versione pubblica" >&2; exit 1; fi
  if [ "$VARIANT" = privato ] && [ "$found" = 0 ]; then echo "ERRORE: server mancante nella versione privata" >&2; exit 1; fi
fi
echo "ARTEFATTO: $OUT/Merlin-RustDesk_${VERSION}${SUFFIX}_aarch64.dmg"
