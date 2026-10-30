//! Lizenzpruefung: signierte Lizenzdatei (Ed25519), offline, nicht an einen PC gebunden.
//!
//! Format: "B2M1." + base64url(Payload-JSON) + "." + base64url(Signatur ueber die Payload-Bytes).
//! Ausgestellt wird mit `tools/lizenz-tool`; hier ist nur der oeffentliche Schluessel eingebaut.
//! Ohne gueltige Lizenz laeuft die Demo: alles ausser Versenden und Ordnerueberwachung.

use crate::i18n::tr;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use chrono::NaiveDate;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const OEFFENTLICHER_SCHLUESSEL: [u8; 32] = [
    0xf4, 0xb0, 0xbb, 0xf4, 0xe1, 0xd4, 0xf9, 0xa6, 0x55, 0x72, 0x22, 0x7e, 0x16, 0x91, 0x57, 0xff, 0xdf, 0xe6, 0x58, 0xd4, 0xff, 0x43,
    0x58, 0x81, 0x2f, 0x74, 0x6b, 0xe6, 0xa8, 0x55, 0x56, 0x21,
];

pub const DATEINAME: &str = "lizenz.lic";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lizenz {
    pub kunde: String,
    pub ausgestellt: String,
    /// Letzter gueltiger Tag (JJJJ-MM-TT); `None` = unbefristet
    pub bis: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LizenzStatus {
    pub lizenziert: bool,
    pub kunde: String,
    pub bis: Option<String>,
    pub meldung: String,
}

fn datum(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| tr("Lizenzdatei ungültig (Datum).", "Invalid license file (date)."))
}

fn ungueltig() -> String {
    tr("Lizenzdatei ungültig oder manipuliert.", "License file invalid or tampered with.")
}

/// Prueft Signatur und Ablaufdatum.
pub fn pruefen_mit(inhalt: &str, schluessel: &[u8; 32], heute: NaiveDate) -> Result<Lizenz, String> {
    let teile: Vec<&str> = inhalt.trim().split('.').collect();
    let [kopf, payload_b64, sig_b64] = teile[..] else { return Err(ungueltig()) };
    if kopf != "B2M1" {
        return Err(ungueltig());
    }
    let payload = B64.decode(payload_b64).map_err(|_| ungueltig())?;
    let sig_bytes: [u8; 64] = B64.decode(sig_b64).map_err(|_| ungueltig())?.try_into().map_err(|_| ungueltig())?;
    let key = VerifyingKey::from_bytes(schluessel).map_err(|_| ungueltig())?;
    key.verify(&payload, &Signature::from_bytes(&sig_bytes)).map_err(|_| ungueltig())?;
    let lizenz: Lizenz = serde_json::from_slice(&payload).map_err(|_| ungueltig())?;
    if let Some(bis) = &lizenz.bis {
        if heute > datum(bis)? {
            return Err(tr(&format!("Die Lizenz ist am {bis} abgelaufen."), &format!("The license expired on {bis}.")));
        }
    }
    Ok(lizenz)
}

pub fn pruefen(inhalt: &str) -> Result<Lizenz, String> {
    pruefen_mit(inhalt, &OEFFENTLICHER_SCHLUESSEL, chrono::Local::now().date_naive())
}

pub fn pfad(datenordner: &Path) -> PathBuf {
    datenordner.join(DATEINAME)
}

/// Liest die installierte Lizenz; `Err` mit Erklaerung, wenn keine oder keine gueltige da ist.
pub fn laden(datenordner: &Path) -> Result<Lizenz, String> {
    let inhalt = std::fs::read_to_string(pfad(datenordner)).map_err(|_| tr("Keine Lizenz installiert (Demo-Version).", "No license installed (demo version)."))?;
    pruefen(&inhalt)
}

pub fn status(datenordner: &Path) -> LizenzStatus {
    match laden(datenordner) {
        Ok(l) => LizenzStatus {
            lizenziert: true,
            meldung: tr(&format!("Lizenziert für {}", l.kunde), &format!("Licensed to {}", l.kunde)),
            bis: l.bis,
            kunde: l.kunde,
        },
        Err(m) => LizenzStatus { lizenziert: false, kunde: String::new(), bis: None, meldung: m },
    }
}

/// Meldung fuer gesperrte Funktionen
pub fn erforderlich() -> String {
    tr(
        "Diese Funktion ist nur mit gültiger Lizenz verfügbar (Einstellungen → Lizenz). Die Demo-Version kann alles außer Versenden und Ordnerüberwachung.",
        "This feature requires a valid license (Settings → License). The demo version can do everything except sending and folder monitoring.",
    )
}

/// Prueft eine Lizenzdatei und kopiert sie in den Datenordner.
pub fn installieren(datei: &Path, datenordner: &Path) -> Result<Lizenz, String> {
    let inhalt = std::fs::read_to_string(datei).map_err(|e| tr(&format!("Datei konnte nicht gelesen werden: {e}"), &format!("File could not be read: {e}")))?;
    let lizenz = pruefen(&inhalt)?;
    std::fs::create_dir_all(datenordner).map_err(|e| e.to_string())?;
    std::fs::write(pfad(datenordner), inhalt.trim()).map_err(|e| tr(&format!("Lizenz konnte nicht gespeichert werden: {e}"), &format!("License could not be saved: {e}")))?;
    Ok(lizenz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn schluessel(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn lizenzdatei(key: &SigningKey, bis: Option<&str>) -> String {
        let payload = serde_json::json!({"kunde": "Muster GmbH", "ausgestellt": "2026-10-30", "bis": bis}).to_string();
        let sig = key.sign(payload.as_bytes());
        format!("B2M1.{}.{}\n", B64.encode(payload.as_bytes()), B64.encode(sig.to_bytes()))
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn gueltige_lizenz() {
        let key = schluessel(7);
        let pk = key.verifying_key().to_bytes();
        let l = pruefen_mit(&lizenzdatei(&key, None), &pk, d("2030-01-01")).unwrap();
        assert_eq!(l.kunde, "Muster GmbH");
        assert_eq!(l.bis, None);
    }

    #[test]
    fn befristete_lizenz_laeuft_ab() {
        let key = schluessel(7);
        let pk = key.verifying_key().to_bytes();
        let datei = lizenzdatei(&key, Some("2026-12-31"));
        assert!(pruefen_mit(&datei, &pk, d("2026-12-31")).is_ok()); // letzter Tag gilt noch
        assert!(pruefen_mit(&datei, &pk, d("2027-01-01")).unwrap_err().contains("2026-12-31"));
    }

    #[test]
    fn fremde_signatur_manipulation_und_muell_werden_abgelehnt() {
        let echt = schluessel(7);
        let fremd = schluessel(8);
        let pk = echt.verifying_key().to_bytes();
        assert!(pruefen_mit(&lizenzdatei(&fremd, None), &pk, d("2026-11-01")).is_err());
        // Payload austauschen (anderer Kunde / unbefristet), Signatur bleibt
        let datei = lizenzdatei(&echt, Some("2026-11-30"));
        let teile: Vec<&str> = datei.trim().split('.').collect();
        let payload = serde_json::json!({"kunde": "Pirat", "ausgestellt": "2026-10-30", "bis": null}).to_string();
        let gefaelscht = format!("B2M1.{}.{}", B64.encode(payload.as_bytes()), teile[2]);
        assert!(pruefen_mit(&gefaelscht, &pk, d("2026-11-01")).is_err());
        for muell in ["", "abc", "B2M1.a.b", "X.a.b.c", "B2M1..", "B2M1.e30.AAAA"] {
            assert!(pruefen_mit(muell, &pk, d("2026-11-01")).is_err(), "{muell}");
        }
    }

    /// Prueft eine mit tools/lizenz-tool ausgestellte Datei gegen den eingebauten Schluessel:
    /// B2M_LIC=<datei.lic> cargo test echte_lizenz -- --ignored --nocapture
    #[test]
    #[ignore]
    fn echte_lizenz() {
        let Ok(pfad) = std::env::var("B2M_LIC") else { return };
        let l = pruefen(&std::fs::read_to_string(pfad).unwrap()).unwrap();
        println!("{l:?}");
    }

    #[test]
    fn eingebauter_schluessel_ist_gueltig() {
        assert!(VerifyingKey::from_bytes(&OEFFENTLICHER_SCHLUESSEL).is_ok());
    }
}
