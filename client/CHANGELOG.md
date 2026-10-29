# Novità del client Merlin-RustDesk

## [0.4.0] - 2026-10-29

- Federazione di Merlin server: quando il server di casa non risponde il client
  si registra, con lo stesso ID, sul primo server della federazione che
  risponde, e torna a casa quando casa è di nuovo raggiungibile.
- L'heartbeat e le informazioni di sistema mandate al server sono firmati con
  la chiave del dispositivo: nessun altro può presentarsi con lo stesso ID.
- Il client non cambia più ID da solo quando il server lo rifiuta: lo cambia
  solo su indicazione della federazione.
