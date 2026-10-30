//! Sprache der vom Backend erzeugten Texte (Fehler, Hinweise, Protokoll).
//! Das Frontend setzt sie beim Start (Windows-Sprache) und beim Umschalten in den Einstellungen.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISCH: AtomicBool = AtomicBool::new(false);

pub fn setzen(englisch: bool) {
    ENGLISCH.store(englisch, Ordering::Relaxed);
}

pub fn ist_englisch() -> bool {
    ENGLISCH.load(Ordering::Relaxed)
}

/// Deutscher oder englischer Text je nach eingestellter Sprache.
pub fn tr(de: &str, en: &str) -> String {
    tr_static(de, en).to_string()
}

pub fn tr_static<'a>(de: &'a str, en: &'a str) -> &'a str {
    if ist_englisch() {
        en
    } else {
        de
    }
}

/// "de-AT" -> Deutsch; jede andere Sprache -> Englisch. Unbekannt (leer) -> Deutsch.
pub fn ist_englischer_code(code: &str) -> bool {
    let c = code.trim().to_ascii_lowercase();
    !c.is_empty() && !c.starts_with("de")
}

/// Windows-Anzeigesprache als Code, z. B. "de-AT". Leer, wenn nicht ermittelbar.
pub fn systemsprache() -> String {
    sys_locale::get_locale().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprachcodes() {
        assert!(ist_englischer_code("en-US"));
        assert!(ist_englischer_code("EN"));
        assert!(ist_englischer_code("fr-FR"));
        assert!(!ist_englischer_code("de-AT"));
        assert!(!ist_englischer_code(""));
    }
}
