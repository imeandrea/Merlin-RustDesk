// SPDX-License-Identifier: AGPL-3.0-only
// Merlin-RustDesk — modified version of RustDesk (https://github.com/rustdesk/rustdesk).
// New file, Copyright (C) 2026 imeandrea.

//! Federazione di Merlin server: logica pura della decisione del cambio di
//! server (D9/D10). Nessuna rete e nessun thread: i controlli e il tempo
//! arrivano come parametri.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}

/// Un giro di controllo. `esito_in_uso`: server in uso (ogni 20 s);
/// `esito_casa`: casa, solo da ospite (ogni 60 s), altrimenti `None`.
pub fn decidi(
    stato: &mut Stato,
    esito_in_uso: Esito,
    esito_casa: Option<Esito>,
    _adesso: Instant,
) -> Azione {
    // Da ospite: casa di nuovo raggiungibile per due volte di seguito.
    if stato.in_uso.is_some() {
        match esito_casa {
            Some(Esito::Ok) => {
                stato.ok_casa += 1;
                if stato.ok_casa >= OK_PER_RITORNO {
                    stato.in_uso = None;
                    stato.fallimenti = 0;
                    stato.ok_casa = 0;
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
        let t = Instant::now();
        assert_eq!(decidi(&mut s, Esito::Fallito, None, t), Azione::Niente);
        assert_eq!(decidi(&mut s, Esito::Fallito, None, t), Azione::Niente);
        assert_eq!(decidi(&mut s, Esito::Ok, None, t), Azione::Niente);
        assert_eq!(s.fallimenti, 0);
    }

    #[test]
    fn decisione_cambia_al_terzo() {
        let mut s = Stato::default();
        let t = Instant::now();
        decidi(&mut s, Esito::Fallito, None, t);
        decidi(&mut s, Esito::Fallito, None, t);
        assert_eq!(decidi(&mut s, Esito::Fallito, None, t), Azione::ProvaAltri);
    }

    #[test]
    fn torna_a_casa_con_due_ok() {
        let mut s = ospite();
        let t = Instant::now();
        assert_eq!(decidi(&mut s, Esito::Ok, Some(Esito::Ok), t), Azione::Niente);
        assert_eq!(decidi(&mut s, Esito::Ok, Some(Esito::Ok), t), Azione::TornaACasa);
        assert_eq!(s.in_uso, None);
    }

    #[test]
    fn nessuna_oscillazione() {
        let mut s = ospite();
        let t = Instant::now();
        for i in 0..10 {
            let e = if i % 2 == 0 { Esito::Ok } else { Esito::Fallito };
            assert_eq!(decidi(&mut s, Esito::Ok, Some(e), t), Azione::Niente);
        }
        assert!(s.in_uso.is_some());
    }

    #[test]
    fn tutti_giu() {
        let mut s = Stato::default();
        let t = Instant::now();
        let mut prova = 0;
        for _ in 0..3 {
            match decidi(&mut s, Esito::Fallito, None, t) {
                Azione::ProvaAltri => prova += 1,
                Azione::Usa(_) => panic!("Usa senza risposte"),
                _ => {}
            }
        }
        assert_eq!(prova, 1);
        // Nessun server risponde: nessuna scelta, e il giro dopo non scatta subito.
        assert_eq!(scegli_primo(&[("a".into(), false), ("b".into(), false)]), None);
        assert_eq!(decidi(&mut s, Esito::Fallito, None, t), Azione::Niente);
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
}
