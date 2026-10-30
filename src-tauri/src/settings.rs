use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sicherheit {
    /// Unverschluesselt, nur fuer lokale Testserver
    Keine,
    Starttls,
    Ssl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Einstellungen {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub sicherheit: Sicherheit,
    pub smtp_user: String,
    pub absender: String,
    pub absender_name: String,
    pub betreff: String,
    /// HTML; {KUNDENNAME} wird ersetzt
    pub text: String,
    pub test_empfaenger: String,
    /// Ordner, in dem neue Rechnungs-PDFs automatisch erkannt werden
    pub ueberwachung_aktiv: bool,
    pub ueberwachung_ordner: String,
    /// Wie weit (in pt, vertikal) ein Rahmen vom Text abweichen darf und trotzdem erkannt wird
    pub toleranz_pt: f64,
}

impl Default for Einstellungen {
    fn default() -> Self {
        Einstellungen {
            smtp_host: String::new(),
            smtp_port: 587,
            sicherheit: Sicherheit::Starttls,
            smtp_user: String::new(),
            absender: String::new(),
            absender_name: String::new(),
            betreff: "Ihre Rechnung".into(),
            text: "<p>Sehr geehrte Damen und Herren,</p><p>anbei finden Sie die aktuelle Rechnung für <b>{KUNDENNAME}</b> als PDF im Anhang.</p><p>Bei Fragen stehen wir Ihnen gerne zur Verfügung.</p><p>Mit freundlichen Grüßen</p>".into(),
            test_empfaenger: String::new(),
            ueberwachung_aktiv: false,
            ueberwachung_ordner: String::new(),
            toleranz_pt: crate::parser::TOLERANZ_STANDARD_PT,
        }
    }
}

const DIENST: &str = "Bill2Mail";
const KONTO: &str = "smtp-passwort";

fn eintrag() -> Result<keyring::Entry, String> {
    keyring::Entry::new(DIENST, KONTO).map_err(|e| format!("Passwortspeicher nicht verfügbar: {e}"))
}

/// Das SMTP-Passwort liegt im Windows-Anmeldeinformationsspeicher, nicht in der Datenbank.
pub fn passwort_lesen() -> Option<String> {
    eintrag().ok()?.get_password().ok()
}

pub fn passwort_speichern(passwort: &str) -> Result<(), String> {
    let e = eintrag()?;
    if passwort.is_empty() {
        let _ = e.delete_credential();
        Ok(())
    } else {
        e.set_password(passwort).map_err(|e| format!("Passwort konnte nicht gespeichert werden: {e}"))
    }
}

#[cfg(test)]
mod tests {
    /// Prueft den Windows-Anmeldeinformationsspeicher mit einem eigenen Test-Eintrag:
    /// cargo test passwortspeicher -- --ignored
    #[test]
    #[ignore]
    fn passwortspeicher_roundtrip() {
        let e = keyring::Entry::new("Bill2Mail-test", "roundtrip").unwrap();
        e.set_password("geheim äöü").unwrap();
        assert_eq!(e.get_password().unwrap(), "geheim äöü");
        e.delete_credential().unwrap();
        assert!(e.get_password().is_err());
    }
}
