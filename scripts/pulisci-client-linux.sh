#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Rimuove del tutto il client Merlin-RustDesk da un PC Linux, per le prove:
# chiude il client, toglie il servizio systemd, il programma installato (anche il
# pacchetto .deb) e le cartelle dei dati di tutti gli utenti (configurazione con
# server e chiave compresa). Il file .AppImage scaricato resta dov'è.
# Non tocca MerlinHost (il componente di Merlin) né RustDesk originale.
#
#   sudo sh pulisci-client-linux.sh [--prova]
#
# Con --prova elenca cosa farebbe senza toccare niente.
APP=MerlinRustDesk            # nome dell'app nel client (APP_NAME)
ID=eu.raapp.merlinremote      # identificatore Tauri del client
SVC=merlinrustdesk            # servizio systemd e cartella in /usr/lib
PROVA=0
[ "$1" = "--prova" ] && PROVA=1

if [ "$PROVA" = 0 ] && [ "$(id -u)" != 0 ]; then
  echo "Avvialo con sudo (o con --prova per vedere cosa farebbe)." >&2
  exit 1
fi

fai() {  # fai "descrizione" comando...
  desc=$1; shift
  if [ "$PROVA" = 1 ]; then echo "[prova] $desc"; else echo "$desc"; "$@"; fi
}

# 1. Servizio (accesso non presidiato).
if [ -f /etc/systemd/system/$SVC.service ] || systemctl list-unit-files 2>/dev/null | grep -q "^$SVC.service"; then
  fai "Fermo e disattivo il servizio $SVC" systemctl disable --now $SVC
fi

# 2. Processi del client (installato, AppImage avviata a mano, servizio).
if pgrep -f "merlin-rustdesk-client|/usr/lib/$SVC/" >/dev/null 2>&1; then
  fai "Chiudo i processi del client" pkill -f "merlin-rustdesk-client|/usr/lib/$SVC/"
fi

# 3. Pacchetto .deb, se installato.
if command -v dpkg >/dev/null 2>&1 && dpkg -s merlin-rustdesk >/dev/null 2>&1; then
  fai "Disinstallo il pacchetto merlin-rustdesk" apt-get purge -y merlin-rustdesk
fi

# 4. File di sistema.
for f in /etc/systemd/system/$SVC.service /usr/share/applications/$SVC.desktop /run/$SVC.pid /usr/lib/$SVC; do
  [ -e "$f" ] && fai "Elimino $f" rm -rf "$f"
done
[ "$PROVA" = 0 ] && systemctl daemon-reload 2>/dev/null

# 5. Dati di tutti gli utenti e di root (il servizio gira come root).
for h in /root /home/*; do
  [ -d "$h" ] || continue
  for d in "$h/.config/$APP" "$h/.config/$ID" "$h/.local/share/$ID" "$h/.local/share/logs/$APP" "$h/.cache/$ID"; do
    [ -e "$d" ] && fai "Elimino $d" rm -rf "$d"
  done
done
for t in /tmp/$APP-*; do
  [ -e "$t" ] && fai "Elimino $t" rm -rf "$t"
done

if [ "$PROVA" = 1 ]; then echo "Fine della prova: niente è stato toccato."; else echo "Fatto: il client Merlin-RustDesk è stato rimosso."; fi
