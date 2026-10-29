// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

//! Federazione di Merlin server (D1, D9, D10): decisione del cambio di server
//! (logica pura, il tempo e gli esiti arrivano come parametri), firma
//! dell'heartbeat, sovrascrittura in memoria del server in uso letta dai ganci
//! di `common.rs` e `rendezvous_mediator.rs`, e thread di controllo.

use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex, RwLock};
use std::time::{Duration, Instant};

use hbb_common::config::{self, Config};
use hbb_common::log;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Versione del client Merlin-RustDesk (campo `merlin.v` dell'heartbeat).
pub const MERLIN_CLIENT_VERSION: &str = "0.4.0";

/// Fallimenti di seguito del server in uso prima di provare gli altri.
pub const FALLIMENTI_PER_CAMBIO: u32 = 3;
/// Risposte di seguito di casa, da ospite, prima di tornarci.
pub const OK_PER_RITORNO: u32 = 2;
/// Per quanto tempo un server escluso non viene riprovato.
pub const DURATA_ESCLUSIONE: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerFed {
    pub name: String,
    pub api: String,
    pub hbbs: String,
    pub relay: String,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Elenco {
    pub home: String,
    pub servers: Vec<ServerFed>,
}

/// Estrae l'elenco da `merlin.federation` della risposta all'heartbeat.
/// I campi sconosciuti si ignorano; `None` se manca o è malformato.
pub fn da_risposta(v: &Value) -> Option<Elenco> {
    let f = v.get("merlin")?.get("federation")?;
    serde_json::from_value(f.clone()).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Esito {
    Ok,
    Fallito,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Azione {
    Niente,
    ProvaAltri,
    /// Usa il server con questa `api`. Non la produce `decidi`: la produce il chiamante
    /// dopo `ProvaAltri`, con il risultato di `scegli_primo` sulle risposte delle prove.
    Usa(String),
    TornaACasa,
}

#[derive(Debug, Default)]
pub struct Stato {
    /// `api` del server ospitante; `None` = si è a casa.
    pub in_uso: Option<String>,
    pub fallimenti: u32,
    pub ok_casa: u32,
    /// Server esclusi (per `api`) e istante dell'esclusione.
    pub esclusi: HashMap<String, Instant>,
}

impl Stato {
    /// Esclude un server per un'ora.
    pub fn escludi(&mut self, api: &str, adesso: Instant) {
        self.esclusi.insert(api.to_owned(), adesso);
    }

    /// Il server è escluso adesso? Le esclusioni scadute si rimuovono.
    pub fn escluso(&mut self, api: &str, adesso: Instant) -> bool {
        self.esclusi
            .retain(|_, t| adesso.saturating_duration_since(*t) < DURATA_ESCLUSIONE);
        self.esclusi.contains_key(api)
    }

    /// Si passa a usare il server ospitante `api` (contatori azzerati).
    pub fn usa(&mut self, api: &str) {
        self.in_uso = Some(api.to_owned());
        self.fallimenti = 0;
        self.ok_casa = 0;
    }

    /// Si torna a casa (contatori azzerati).
    pub fn a_casa(&mut self) {
        self.in_uso = None;
        self.fallimenti = 0;
        self.ok_casa = 0;
    }

    /// UUID_MISMATCH (D10): da ospite il server in uso si esclude per un'ora e
    /// si deve passare a un altro (`true`); a casa non si fa niente.
    pub fn uuid_mismatch(&mut self, adesso: Instant) -> bool {
        match self.in_uso.clone() {
            Some(api) => {
                self.escludi(&api, adesso);
                true
            }
            None => false,
        }
    }
}

/// Un giro di controllo. `esito_in_uso`: server in uso (ogni 20 s);
/// `esito_casa`: casa, solo da ospite (ogni 60 s), altrimenti `None`.
pub fn decidi(
    stato: &mut Stato,
    esito_in_uso: Esito,
    esito_casa: Option<Esito>,
) -> Azione {
    // Da ospite: casa di nuovo raggiungibile per due volte di seguito.
    if stato.in_uso.is_some() {
        match esito_casa {
            Some(Esito::Ok) => {
                stato.ok_casa += 1;
                if stato.ok_casa >= OK_PER_RITORNO {
                    stato.a_casa();
                    return Azione::TornaACasa;
                }
            }
            Some(Esito::Fallito) => stato.ok_casa = 0,
            None => {}
        }
    }
    match esito_in_uso {
        Esito::Ok => {
            stato.fallimenti = 0;
            Azione::Niente
        }
        Esito::Fallito => {
            stato.fallimenti += 1;
            if stato.fallimenti >= FALLIMENTI_PER_CAMBIO {
                // Nuovo giro: servono altri 3 fallimenti per riprovare.
                stato.fallimenti = 0;
                Azione::ProvaAltri
            } else {
                Azione::Niente
            }
        }
    }
}

/// Il primo server che ha risposto (`true`) nell'ordine d'arrivo.
pub fn scegli_primo(risposte: &[(String, bool)]) -> Option<String> {
    risposte.iter().find(|(_, ok)| *ok).map(|(api, _)| api.clone())
}

fn norma(api: &str) -> &str {
    api.trim().trim_end_matches('/')
}

/// I server da provare quando quello in uso non risponde: tutti quelli dell'elenco
/// tranne quello in uso e gli esclusi, casa compresa quando si è ospiti.
pub fn candidati(elenco: &Elenco, stato: &mut Stato, adesso: Instant) -> Vec<ServerFed> {
    let in_uso = stato.in_uso.clone().unwrap_or_else(|| elenco.home.clone());
    elenco
        .servers
        .iter()
        .filter(|s| !s.hbbs.trim().is_empty() && norma(&s.api) != norma(&in_uso))
        .filter(|s| !stato.escluso(&s.api, adesso))
        .cloned()
        .collect()
}

/// Applica allo stato la scelta di `Azione::Usa(api)`: `Some(None)` se `api` è
/// casa, `Some(Some(server))` se è un ospitante, `None` se non è nell'elenco.
pub fn applica_scelta(stato: &mut Stato, elenco: &Elenco, api: &str) -> Option<Option<ServerFed>> {
    if norma(api) == norma(&elenco.home) {
        stato.a_casa();
        return Some(None);
    }
    let s = elenco.servers.iter().find(|s| norma(&s.api) == norma(api))?;
    stato.usa(&s.api);
    Some(Some(s.clone()))
}

/// Cosa fare della risposta a un heartbeat o a un sysinfo.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Risposta {
    /// `Some(Some(e))`: salva l'elenco; `Some(None)`: il server di casa non è
    /// (più) federato, l'elenco si cancella; `None`: non toccare l'elenco.
    pub elenco: Option<Option<Elenco>>,
    /// ID nuovo assegnato dal server (D6).
    pub change_id: Option<String>,
}

/// Interpreta la risposta. Da ospite l'elenco non si salva: il suo `home` è il
/// server ospitante, non casa. Le risposte d'errore (`error`) non toccano niente.
pub fn interpreta(v: &Value, ospite: bool) -> Risposta {
    let Some(o) = v.as_object() else { return Risposta::default() };
    if o.contains_key("error") {
        return Risposta::default();
    }
    let change_id = v
        .get("merlin")
        .and_then(|m| m.get("change_id"))
        .and_then(Value::as_str)
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let elenco = if ospite || change_id.is_some() {
        None
    } else if v.get("merlin").and_then(|m| m.get("federation")).is_some() {
        da_risposta(v).map(Some)
    } else {
        Some(None)
    };
    Risposta { elenco, change_id }
}

/// Byte firmati (D1): `"merlin-hb-v1\0" + id + "\0" + uuid + "\0" + ts`.
pub fn messaggio_heartbeat(id: &str, uuid: &str, ts: i64) -> Vec<u8> {
    let mut m = b"merlin-hb-v1\0".to_vec();
    m.extend_from_slice(id.as_bytes());
    m.push(0);
    m.extend_from_slice(uuid.as_bytes());
    m.push(0);
    m.extend_from_slice(ts.to_string().as_bytes());
    m
}

/// Il blocco `merlin` di D1 firmato con la chiave `sk` (64 byte) e `pk` (32 byte).
pub fn firma_con(sk: &[u8], pk: &[u8], id: &str, uuid: &str, ts: i64) -> Option<Value> {
    use hbb_common::sodiumoxide::crypto::sign;
    let sk = sign::SecretKey::from_slice(sk)?;
    if pk.len() != sign::PUBLICKEYBYTES {
        return None;
    }
    let sig = sign::sign_detached(&messaggio_heartbeat(id, uuid, ts), &sk);
    Some(json!({
        "v": MERLIN_CLIENT_VERSION,
        "pk": crate::encode64(pk),
        "ts": ts,
        "sig": crate::encode64(sig.to_bytes()),
    }))
}

/// Il blocco `merlin` di D1 firmato con la chiave del dispositivo; `Null` se la
/// chiave manca.
pub fn firma_heartbeat(id: &str, uuid: &str, ts: i64) -> Value {
    let (sk, pk) = Config::get_key_pair();
    firma_con(&sk, &pk, id, uuid, ts).unwrap_or_else(|| {
        hbb_common::throttled_log!(LOG_INTERVALLO, error, "federazione: chiave del dispositivo non valida, heartbeat non firmato");
        Value::Null
    })
}

/// Aggiunge il blocco `merlin` al corpo di un heartbeat o di un sysinfo (R6),
/// firmando `id` e `uuid` del corpo stesso.
pub fn aggiungi_firma(v: &mut Value) {
    let id = v["id"].as_str().unwrap_or_default().to_owned();
    let uuid = v["uuid"].as_str().unwrap_or_default().to_owned();
    let m = firma_heartbeat(&id, &uuid, hbb_common::get_time());
    if !m.is_null() {
        v["merlin"] = m;
    }
}

// ---------------------------------------------------------------------------
// Sovrascrittura in memoria (D9) e ganci.

/// Intervallo dei log che possono ripetersi a ogni giro.
const LOG_INTERVALLO: Duration = Duration::from_secs(600);

/// Opzione con l'elenco della federazione (JSON di `Elenco`), scritto dalle
/// risposte di casa.
pub const OPZIONE_ELENCO: &str = "merlin-federation";
/// Porta di serie dell'hbbs.
pub const PORTA_HBBS: u16 = 21116;

lazy_static::lazy_static! {
    static ref SOVRASCRITTURA: RwLock<Option<ServerFed>> = RwLock::new(None);
    /// Valore di `EXE_RENDEZVOUS_SERVER` prima della sovrascrittura.
    static ref EXE_ORIGINALE: Mutex<Option<String>> = Mutex::new(None);
    static ref STATO: Mutex<Stato> = Mutex::new(Stato::default());
}
static AVVIATO: AtomicBool = AtomicBool::new(false);
static CAMBIA_SUBITO: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static TEST_GANCI: Mutex<()> = Mutex::new(());

/// Il server ospitante in uso, `None` a casa.
pub fn server_attivo() -> Option<ServerFed> {
    SOVRASCRITTURA.read().unwrap().clone()
}

/// Imposta (o toglie) la sovrascrittura, compresa quella dell'hbbs in
/// `EXE_RENDEZVOUS_SERVER`. Le opzioni salvate non si toccano.
pub fn imposta_sovrascrittura(s: Option<ServerFed>) {
    {
        let mut exe = config::EXE_RENDEZVOUS_SERVER.write().unwrap();
        let mut orig = EXE_ORIGINALE.lock().unwrap();
        match &s {
            Some(srv) => {
                if orig.is_none() {
                    *orig = Some(exe.clone());
                }
                *exe = con_porta(&srv.hbbs);
            }
            None => {
                if let Some(o) = orig.take() {
                    *exe = o;
                }
            }
        }
    }
    *SOVRASCRITTURA.write().unwrap() = s;
}

/// Gancio di `get_key`: la chiave dell'ospitante (anche vuota: allora vale quella di serie).
pub fn gancio_chiave(chiave: String) -> String {
    server_attivo().map(|s| s.key).unwrap_or(chiave)
}

/// Gancio di `get_relay_server`: il relay dell'ospitante (anche vuoto: allora
/// vale quello dato dall'hbbs o la porta accanto).
pub fn gancio_relay(relay: String) -> String {
    server_attivo().map(|s| s.relay).unwrap_or(relay)
}

/// Gancio di `get_api_server`: l'API dell'ospitante.
pub fn api_sovrascritta() -> Option<String> {
    server_attivo().map(|s| norma(&s.api).to_owned())
}

/// `host` con la porta di serie dell'hbbs se manca.
pub fn con_porta(host: &str) -> String {
    let h = host.trim();
    if h.contains(':') {
        h.to_owned()
    } else {
        format!("{h}:{PORTA_HBBS}")
    }
}

fn applica(s: Option<ServerFed>) {
    match &s {
        Some(srv) => log::info!("federazione: casa non risponde, mi registro su {} ({})", srv.name, srv.hbbs),
        None => log::info!("federazione: torno al server di casa"),
    }
    imposta_sovrascrittura(s);
    crate::rendezvous_mediator::RendezvousMediator::restart();
}

// ---------------------------------------------------------------------------
// Risposte del server, UUID_MISMATCH e thread di controllo.

fn elenco_salvato() -> Option<Elenco> {
    let s = Config::get_option(OPZIONE_ELENCO);
    if s.is_empty() {
        return None;
    }
    serde_json::from_str(&s).ok()
}

/// Gestisce la risposta a un heartbeat o a un sysinfo: elenco della federazione
/// e `change_id` (D3, D10).
pub async fn gestisci_risposta(corpo: &str) {
    let Ok(v) = serde_json::from_str::<Value>(corpo) else { return };
    let r = interpreta(&v, server_attivo().is_some());
    match r.elenco {
        Some(Some(e)) => match serde_json::to_string(&e) {
            Ok(s) if s != Config::get_option(OPZIONE_ELENCO) => {
                log::info!("federazione: elenco aggiornato ({} server)", e.servers.len());
                Config::set_option(OPZIONE_ELENCO.to_owned(), s);
            }
            Ok(_) => {}
            Err(e) => log::error!("federazione: elenco non salvato: {e}"),
        },
        Some(None) => {
            if !Config::get_option(OPZIONE_ELENCO).is_empty() {
                log::info!("federazione: il server di casa non è più federato");
                Config::set_option(OPZIONE_ELENCO.to_owned(), String::new());
            }
        }
        None => {}
    }
    if let Some(id) = r.change_id {
        cambia_id(id).await;
    }
}

/// D10: ID nuovo indicato dalla federazione, applicato come il cambio di ID
/// dell'utente (IPC `set_config("id")`), poi riavvio del mediatore.
async fn cambia_id(id: String) {
    if !hbb_common::is_valid_custom_id(&id) {
        log::error!("federazione: ID nuovo dal server non valido: {id:?}");
        return;
    }
    let vecchio = Config::get_id();
    if id == vecchio {
        return;
    }
    log::info!("federazione: il server ha cambiato l'ID {vecchio} in {id}");
    if let Err(e) = crate::ipc::set_config_async("id", id.clone()).await {
        log::error!("federazione: cambio di ID via IPC non riuscito ({e}), lo applico qui");
        Config::set_key_confirmed(false);
        Config::set_id(&id);
    }
    crate::rendezvous_mediator::RendezvousMediator::restart();
}

/// Gancio di `handle_uuid_mismatch` (D10): l'ID non si cambia; da ospite il
/// server si esclude per un'ora e si passa al successivo.
pub fn su_uuid_mismatch(host: &str) {
    hbb_common::throttled_log!(
        LOG_INTERVALLO,
        error,
        "UUID_MISMATCH da {host}: l'ID non si cambia da solo, lo cambia solo la federazione"
    );
    if STATO.lock().unwrap().uuid_mismatch(Instant::now()) {
        hbb_common::throttled_log!(LOG_INTERVALLO, error, "federazione: {host} rifiuta l'ID, lo escludo per un'ora");
        CAMBIA_SUBITO.store(true, Ordering::SeqCst);
    }
}

/// Intervallo del controllo del server in uso.
const GIRO: Duration = Duration::from_secs(20);
/// Da ospite, casa si controlla ogni `GIRI_PER_CASA` giri (60 s).
const GIRI_PER_CASA: u32 = 3;
/// Timeout della connessione di prova all'hbbs.
const TIMEOUT_PROVA: Duration = Duration::from_secs(5);

/// Connessione TCP di prova all'hbbs `host[:porta]`.
fn prova(hbbs: &str) -> Esito {
    let indirizzi = match con_porta(hbbs).to_socket_addrs() {
        Ok(a) => a.collect::<Vec<_>>(),
        Err(_) => return Esito::Fallito,
    };
    for a in indirizzi {
        if TcpStream::connect_timeout(&a, TIMEOUT_PROVA).is_ok() {
            return Esito::Ok;
        }
    }
    Esito::Fallito
}

/// Prova i server in parallelo; le risposte arrivano nell'ordine di completamento.
fn prova_in_parallelo(server: &[ServerFed]) -> Vec<(String, bool)> {
    let (tx, rx) = mpsc::channel();
    for s in server {
        let (tx, api, hbbs) = (tx.clone(), s.api.clone(), s.hbbs.clone());
        std::thread::spawn(move || {
            let _ = tx.send((api, prova(&hbbs) == Esito::Ok));
        });
    }
    drop(tx);
    let fine = Instant::now() + TIMEOUT_PROVA + Duration::from_secs(2);
    let mut risposte = Vec::new();
    while risposte.len() < server.len() {
        let resto = fine.saturating_duration_since(Instant::now());
        match rx.recv_timeout(resto) {
            Ok(r) => risposte.push(r),
            Err(_) => break,
        }
    }
    risposte
}

fn hbbs_casa(elenco: &Elenco) -> String {
    let opz = Config::get_option("custom-rendezvous-server");
    if !opz.trim().is_empty() {
        return opz;
    }
    elenco
        .servers
        .iter()
        .find(|s| norma(&s.api) == norma(&elenco.home))
        .map(|s| s.hbbs.clone())
        .unwrap_or_default()
}

fn prova_altri(elenco: &Elenco) {
    let candidati = candidati(elenco, &mut STATO.lock().unwrap(), Instant::now());
    if candidati.is_empty() {
        hbb_common::throttled_log!(LOG_INTERVALLO, warn, "federazione: nessun altro server da provare");
        return;
    }
    let azione = match scegli_primo(&prova_in_parallelo(&candidati)) {
        Some(api) => Azione::Usa(api),
        None => {
            hbb_common::throttled_log!(LOG_INTERVALLO, warn, "federazione: nessun server della federazione risponde");
            return;
        }
    };
    if let Azione::Usa(api) = azione {
        let scelta = applica_scelta(&mut STATO.lock().unwrap(), elenco, &api);
        if let Some(s) = scelta {
            applica(s);
        }
    }
}

fn controllo() {
    let mut secondi = Duration::ZERO;
    let mut giro: u32 = 0;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        secondi += Duration::from_secs(1);
        let subito = CAMBIA_SUBITO.swap(false, Ordering::SeqCst);
        if !subito && secondi < GIRO {
            continue;
        }
        secondi = Duration::ZERO;
        if config::option2bool("stop-service", &Config::get_option("stop-service")) {
            continue;
        }
        let Some(elenco) = elenco_salvato() else {
            // Nessuna federazione: se si era ospiti si torna a casa.
            if server_attivo().is_some() {
                STATO.lock().unwrap().a_casa();
                applica(None);
            }
            continue;
        };
        if subito {
            prova_altri(&elenco);
            continue;
        }
        giro = giro.wrapping_add(1);
        let attivo = server_attivo();
        let esito_in_uso = match &attivo {
            Some(s) => prova(&s.hbbs),
            None => prova(&hbbs_casa(&elenco)),
        };
        let esito_casa = (attivo.is_some() && giro % GIRI_PER_CASA == 0).then(|| prova(&hbbs_casa(&elenco)));
        let azione = decidi(&mut STATO.lock().unwrap(), esito_in_uso, esito_casa);
        match azione {
            Azione::TornaACasa => applica(None),
            Azione::ProvaAltri => prova_altri(&elenco),
            Azione::Niente | Azione::Usa(_) => {}
        }
    }
}

/// Avvia (una volta sola) il thread di controllo del server in uso (D9).
pub fn avvia() {
    if AVVIATO.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Err(e) = std::thread::Builder::new().name("merlin-federazione".into()).spawn(controllo) {
        log::error!("federazione: controllo non avviato: {e}");
        AVVIATO.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ospite() -> Stato {
        let mut s = Stato::default();
        s.usa("https://b");
        s
    }

    #[test]
    fn decisione_resta_con_due_fallimenti() {
        let mut s = Stato::default();
        assert_eq!(decidi(&mut s, Esito::Fallito, None), Azione::Niente);
        assert_eq!(decidi(&mut s, Esito::Fallito, None), Azione::Niente);
        assert_eq!(decidi(&mut s, Esito::Ok, None), Azione::Niente);
        assert_eq!(s.fallimenti, 0);
    }

    #[test]
    fn decisione_cambia_al_terzo() {
        let mut s = Stato::default();
        decidi(&mut s, Esito::Fallito, None);
        decidi(&mut s, Esito::Fallito, None);
        assert_eq!(decidi(&mut s, Esito::Fallito, None), Azione::ProvaAltri);
    }

    #[test]
    fn torna_a_casa_con_due_ok() {
        let mut s = ospite();
        assert_eq!(decidi(&mut s, Esito::Ok, Some(Esito::Ok)), Azione::Niente);
        assert_eq!(decidi(&mut s, Esito::Ok, Some(Esito::Ok)), Azione::TornaACasa);
        assert_eq!(s.in_uso, None);
    }

    #[test]
    fn nessuna_oscillazione() {
        let mut s = ospite();
        for i in 0..10 {
            let e = if i % 2 == 0 { Esito::Ok } else { Esito::Fallito };
            assert_eq!(decidi(&mut s, Esito::Ok, Some(e)), Azione::Niente);
        }
        assert!(s.in_uso.is_some());
    }

    #[test]
    fn tutti_giu() {
        let mut s = Stato::default();
        let mut prova = 0;
        for _ in 0..3 {
            if decidi(&mut s, Esito::Fallito, None) == Azione::ProvaAltri {
                prova += 1;
            }
        }
        assert_eq!(prova, 1);
        // Nessun server risponde: nessuna scelta, e il giro dopo non scatta subito.
        assert_eq!(scegli_primo(&[("a".into(), false), ("b".into(), false)]), None);
        assert_eq!(decidi(&mut s, Esito::Fallito, None), Azione::Niente);
        assert_eq!(scegli_primo(&[("a".into(), false), ("b".into(), true), ("c".into(), true)]), Some("b".into()));
    }

    #[test]
    fn escluso_per_un_ora() {
        let mut s = Stato::default();
        let t = Instant::now();
        s.escludi("https://b", t);
        assert!(s.escluso("https://b", t + Duration::from_secs(3599)));
        assert!(!s.escluso("https://b", t + Duration::from_secs(3600)));
        assert!(!s.escluso("https://altro", t));
    }

    #[test]
    fn elenco_da_risposta_ignora_campi_ignoti() {
        let v = json!({"x": 1, "merlin": {"v": 2, "change_id": "123456789", "federation": {
            "home": "https://a", "extra": true,
            "servers": [{"name":"A","api":"https://a","hbbs":"a:21116","relay":"a:21117","key":"K","nuovo":1}]}}});
        let e = da_risposta(&v).unwrap();
        assert_eq!(e.home, "https://a");
        assert_eq!(e.servers.len(), 1);
        assert_eq!(e.servers[0].hbbs, "a:21116");
        assert!(da_risposta(&json!({"merlin": {"change_id": "1"}})).is_none());
        assert!(da_risposta(&json!({})).is_none());
    }

    #[test]
    fn versione_client_come_cargo_toml() {
        let toml = include_str!("../client/src-tauri/Cargo.toml");
        let v = toml
            .lines()
            .find_map(|l| l.trim().strip_prefix("version = \""))
            .and_then(|r| r.split('"').next())
            .unwrap();
        assert_eq!(MERLIN_CLIENT_VERSION, v);
    }

    fn srv(n: &str) -> ServerFed {
        ServerFed {
            name: n.to_uppercase(),
            api: format!("https://{n}"),
            hbbs: format!("{n}:21116"),
            relay: format!("{n}:21117"),
            key: format!("K{n}"),
        }
    }

    fn elenco() -> Elenco {
        Elenco { home: "https://a".into(), servers: vec![srv("a"), srv("b"), srv("c")] }
    }

    #[test]
    fn firma_heartbeat_verificabile() {
        use hbb_common::sodiumoxide::crypto::sign;
        assert_eq!(messaggio_heartbeat("123", "dXU=", 170), b"merlin-hb-v1\0123\0dXU=\0170".to_vec());
        let (pk, sk) = sign::gen_keypair();
        let v = firma_con(&sk.0, &pk.0, "123456789", "dXVpZA==", 1_700_000_000_123).unwrap();
        assert_eq!(v["v"], MERLIN_CLIENT_VERSION);
        assert_eq!(v["ts"], 1_700_000_000_123i64);
        assert_eq!(crate::decode64(v["pk"].as_str().unwrap()).unwrap(), pk.0.to_vec());
        let sig = crate::decode64(v["sig"].as_str().unwrap()).unwrap();
        assert_eq!(sig.len(), 64);
        let sig = sign::Signature::try_from(&sig[..]).unwrap();
        let m = messaggio_heartbeat("123456789", "dXVpZA==", 1_700_000_000_123);
        assert!(sign::verify_detached(&sig, &m, &pk));
        // Un altro ID non verifica.
        assert!(!sign::verify_detached(&sig, &messaggio_heartbeat("123456780", "dXVpZA==", 1_700_000_000_123), &pk));
        assert!(firma_con(&[1, 2], &pk.0, "1", "u", 1).is_none());
    }

    #[test]
    fn ganci_senza_sovrascrittura_come_prima() {
        let _g = TEST_GANCI.lock().unwrap();
        imposta_sovrascrittura(None);
        assert_eq!(server_attivo(), None);
        assert_eq!(gancio_chiave("K".into()), "K");
        assert_eq!(gancio_relay("r:21117".into()), "r:21117");
        assert_eq!(gancio_relay("".into()), "");
        assert_eq!(api_sovrascritta(), None);
        assert_eq!(*hbb_common::config::EXE_RENDEZVOUS_SERVER.read().unwrap(), "");
    }

    #[test]
    fn ganci_con_sovrascrittura_usano_il_server_ospitante() {
        let _g = TEST_GANCI.lock().unwrap();
        let b = srv("b");
        imposta_sovrascrittura(Some(b.clone()));
        assert_eq!(server_attivo(), Some(b.clone()));
        assert_eq!(gancio_chiave("K".into()), "Kb");
        assert_eq!(gancio_relay("a:21117".into()), "b:21117");
        assert_eq!(api_sovrascritta(), Some("https://b".into()));
        assert_eq!(hbb_common::config::Config::get_rendezvous_servers(), vec!["b:21116".to_string()]);
        // Ritorno a casa: valori di prima.
        imposta_sovrascrittura(None);
        assert_eq!(server_attivo(), None);
        assert_eq!(gancio_chiave("K".into()), "K");
        assert_eq!(*hbb_common::config::EXE_RENDEZVOUS_SERVER.read().unwrap(), "");
        // Chiave e relay vuoti dell'ospite: si usano i valori di serie, non quelli di casa.
        let mut c = srv("c");
        c.relay.clear();
        c.key.clear();
        imposta_sovrascrittura(Some(c));
        assert_eq!(gancio_chiave("K".into()), "");
        assert_eq!(gancio_relay("a:21117".into()), "");
        imposta_sovrascrittura(None);
    }

    #[test]
    fn candidati_escludono_in_uso_ed_esclusi() {
        let t = Instant::now();
        let mut s = Stato::default();
        // A casa: casa non è un candidato.
        let c: Vec<_> = candidati(&elenco(), &mut s, t).into_iter().map(|x| x.api).collect();
        assert_eq!(c, vec!["https://b", "https://c"]);
        // Da ospite su b con c escluso: resta solo casa.
        s.usa("https://b");
        s.escludi("https://c", t);
        let c: Vec<_> = candidati(&elenco(), &mut s, t).into_iter().map(|x| x.api).collect();
        assert_eq!(c, vec!["https://a"]);
    }

    #[test]
    fn server_scelto_casa_o_ospite() {
        let mut s = ospite();
        let e = elenco();
        assert_eq!(applica_scelta(&mut s, &e, "https://a/"), Some(None));
        assert_eq!(s.in_uso, None);
        assert_eq!(applica_scelta(&mut s, &e, "https://c"), Some(Some(srv("c"))));
        assert_eq!(s.in_uso.as_deref(), Some("https://c"));
        assert_eq!(applica_scelta(&mut s, &e, "https://zz"), None);
    }

    #[test]
    fn uuid_mismatch_esclude_solo_da_ospite() {
        let t = Instant::now();
        let mut s = Stato::default();
        assert!(!s.uuid_mismatch(t));
        assert!(s.esclusi.is_empty());
        let mut s = ospite();
        assert!(s.uuid_mismatch(t));
        assert!(s.escluso("https://b", t));
    }

    #[test]
    fn risposta_heartbeat_interpretata() {
        let fed = json!({"merlin": {"federation": {"home": "https://a", "servers": []}}});
        assert_eq!(interpreta(&fed, false).elenco, Some(Some(Elenco { home: "https://a".into(), servers: vec![] })));
        // Da ospite l'elenco è quello del server ospitante: non si salva.
        assert_eq!(interpreta(&fed, true).elenco, None);
        // A casa, risposta senza federazione: il server non è più federato.
        assert_eq!(interpreta(&json!({}), false).elenco, Some(None));
        // Errore (403): non si tocca niente.
        assert_eq!(interpreta(&json!({"error": "Firma del dispositivo non valida"}), false), Risposta::default());
        let r = interpreta(&json!({"merlin": {"change_id": "987654321"}}), true);
        assert_eq!(r.change_id.as_deref(), Some("987654321"));
        assert_eq!(r.elenco, None);
    }

    #[test]
    fn indirizzo_hbbs_con_porta_di_serie() {
        assert_eq!(con_porta("a.example"), "a.example:21116");
        assert_eq!(con_porta("a.example:1234"), "a.example:1234");
        assert_eq!(con_porta(" a "), "a:21116");
    }
}
