use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sicherheit {
    /// Unverschluesselt, nur fuer lokale Testserver
    Keine,
    Starttls,
    Ssl,
}

/// Wie sich Bill2Mail beim SMTP-Server anmeldet
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Anmeldung {
    /// Benutzer und Passwort (bzw. App-Passwort)
    #[default]
    Passwort,
    /// OAuth 2.0 mit Google (Gmail)
    Google,
    /// OAuth 2.0 mit Microsoft (Outlook.com, Microsoft 365)
    Microsoft,
}

impl Anmeldung {
    pub fn anbieter(self) -> Option<crate::oauth::Anbieter> {
        match self {
            Anmeldung::Passwort => None,
            Anmeldung::Google => Some(crate::oauth::Anbieter::Google),
            Anmeldung::Microsoft => Some(crate::oauth::Anbieter::Microsoft),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Einstellungen {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub sicherheit: Sicherheit,
    pub smtp_user: String,
    pub anmeldung: Anmeldung,
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
            anmeldung: Anmeldung::Passwort,
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
    keyring::Entry::new(DIENST, KONTO).map_err(|e| crate::i18n::tr(&format!("Passwortspeicher nicht verfügbar: {e}"), &format!("Password store not available: {e}")))
}

/// Das SMTP-Passwort liegt im Windows-Anmeldeinformationsspeicher, nicht in der Datenbank.
pub fn passwort_lesen() -> Option<String> {
    eintrag().ok()?.get_password().ok()
}

/// Geheimnis für den SMTP-Login: das gespeicherte Passwort oder ein frisches OAuth-Access-Token.
/// Macht bei OAuth einen Netzwerkaufruf, daher nur außerhalb des UI-Threads aufrufen.
pub fn zugang(e: &Einstellungen) -> Result<Option<String>, String> {
    match e.anmeldung.anbieter() {
        None => Ok(passwort_lesen()),
        Some(a) => crate::oauth::access_token(a).map(Some),
    }
}

pub fn passwort_speichern(passwort: &str) -> Result<(), String> {
    let e = eintrag()?;
    if passwort.is_empty() {
        let _ = e.delete_credential();
        Ok(())
    } else {
        e.set_password(passwort).map_err(|e| crate::i18n::tr(&format!("Passwort konnte nicht gespeichert werden: {e}"), &format!("Password could not be saved: {e}")))
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
