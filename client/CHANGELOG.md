# Novità del client Merlin-RustDesk

## [0.4.0] - 2026-10-29

- Federazione di Merlin server: quando il server di casa non risponde il client
  si registra, con lo stesso ID, sul primo server della federazione che
  risponde, e torna a casa quando casa è di nuovo raggiungibile.
- L'heartbeat e le informazioni di sistema mandate al Merlin server sono
  firmati con la chiave del dispositivo: nessun altro può prendere il suo posto
  sul Merlin server (dispositivi, informazioni e federazione).
- In una federazione il client non cambia più ID da solo quando il server lo
  rifiuta: lo cambia solo su indicazione del server di casa (e solo se l'API è
  in https). Fuori da una federazione si comporta come prima.
- Quando il server di casa non fa più parte di una federazione il client
  dimentica l'elenco dei server.
