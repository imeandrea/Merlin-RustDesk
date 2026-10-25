#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Build del client Merlin-RustDesk sulla VM Linux di build (root@192.168.40.95,
# sorgente sincronizzato con rsync in /root/merlin-rustdesk).
#   build-client-linux.sh pubblico|privato
# pubblico: nessun server compilato (MERLIN_RD_PUBLIC=1, anche se c'è client/.env);
# privato: server, chiave e URL da client/.env (solo per l'utente, mai pubblicato).
VARIANT="${1:?uso: $0 pubblico|privato}"
case "$VARIANT" in
  pubblico) export MERLIN_RD_PUBLIC=1 ;;
  privato) unset MERLIN_RD_PUBLIC; grep -q '^MERLIN_RD_SERVER=..*' /root/merlin-rustdesk/client/.env || { echo "client/.env senza MERLIN_RD_SERVER" >&2; exit 1; } ;;
  *) echo "variante: pubblico o privato" >&2; exit 1 ;;
esac
. /root/.cargo/env 2>/dev/null
cd /root/merlin-rustdesk/client && npm ci && VCPKG_ROOT=/opt/vcpkg npx tauri build --bundles appimage,deb
echo CLIENT_EXIT=$? VARIANT=$VARIANT
ls -la /root/merlin-rustdesk/target/release/bundle/appimage /root/merlin-rustdesk/target/release/bundle/deb 2>/dev/null
