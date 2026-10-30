//! Zerlegt das Sammel-PDF in je eine PDF pro Rechnung.

use crate::parser::Rechnung;
use lopdf::{Document, ObjectId};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct Splitter {
    doc: Document,
    seiten: Vec<ObjectId>,
}

impl Splitter {
    pub fn laden(bytes: &[u8]) -> Result<Self, String> {
        let doc = Document::load_mem(bytes).map_err(|e| crate::i18n::tr(&format!("Fehler beim Laden der PDF: {e}"), &format!("Error loading the PDF: {e}")))?;
        let seiten = doc.get_pages().into_values().collect();
        Ok(Splitter { doc, seiten })
    }

    /// Seiten `start..=end` (0-basiert) als eigenes Dokument. Nicht mehr benötigte Objekte
    /// (Inhalt der entfernten Seiten) werden verworfen, damit die Dateien klein bleiben.
    pub fn teil(&self, start: usize, end: usize) -> Result<Document, String> {
        let n = self.seiten.len();
        if start > end || end >= n {
            return Err(crate::i18n::tr(&format!("Seitenbereich ungültig: {} bis {} (PDF hat {n} Seiten)", start + 1, end + 1), &format!("Invalid page range: {} to {} (PDF has {n} pages)", start + 1, end + 1)));
        }
        let weg: Vec<u32> = (0..n).filter(|i| *i < start || *i > end).map(|i| i as u32 + 1).collect();
        let mut d = self.doc.clone();
        if !weg.is_empty() {
            d.delete_pages(&weg);
        }
        d.prune_objects();
        Ok(d)
    }
}

/// `<Name der PDF>_gesendet` im selben Ordner wie die PDF.
pub fn ausgabeordner(quelle: &Path) -> PathBuf {
    let stem = quelle.file_stem().unwrap_or_default().to_string_lossy();
    quelle.parent().unwrap_or(Path::new(".")).join(format!("{stem}_gesendet"))
}

fn sauber(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// `rechnung_kunde_<Nr>.pdf`; hat ein Kunde mehrere Rechnungen, wird die Rechnungsnummer
/// (sonst ein Zähler) angehängt.
pub fn dateiname(r: &Rechnung, benutzt: &mut HashSet<String>) -> String {
    let knr = if r.kundennummer.is_empty() { format!("unbekannt_s{}", r.start_page + 1) } else { sauber(&r.kundennummer) };
    let mut name = format!("rechnung_kunde_{knr}.pdf");
    if benutzt.contains(&name.to_lowercase()) {
        let zusatz = if r.rechnungsnummer.is_empty() { format!("s{}", r.start_page + 1) } else { sauber(&r.rechnungsnummer) };
        name = format!("rechnung_kunde_{knr}_{zusatz}.pdf");
        let mut i = 2;
        while benutzt.contains(&name.to_lowercase()) {
            name = format!("rechnung_kunde_{knr}_{zusatz}_{i}.pdf");
            i += 1;
        }
    }
    benutzt.insert(name.to_lowercase());
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_text::testutil::test_pdf;

    fn r(knr: &str, re: &str) -> Rechnung {
        Rechnung { kundennummer: knr.into(), rechnungsnummer: re.into(), ..Default::default() }
    }

    #[test]
    fn zerlegen_behaelt_nur_den_bereich() {
        let pdf = test_pdf(&[&[(10.0, 50.0, "Seite eins")], &[(10.0, 50.0, "Seite zwei")], &[(10.0, 50.0, "Seite drei")]]);
        let s = Splitter::laden(&pdf).unwrap();
        let mut teil = s.teil(1, 2).unwrap();
        assert_eq!(teil.get_pages().len(), 2);
        let mut bytes = Vec::new();
        teil.save_to(&mut bytes).unwrap();
        let neu = Splitter::laden(&bytes).unwrap();
        assert_eq!(neu.seiten.len(), 2);
        let text = pdf_extract::extract_text_from_mem(&bytes).unwrap();
        assert!(text.contains("zwei") && text.contains("drei") && !text.contains("eins"), "{text}");
        assert!(s.teil(2, 5).is_err());
    }

    #[test]
    fn dateinamen_sind_eindeutig() {
        let mut b = HashSet::new();
        assert_eq!(dateiname(&r("12", "100"), &mut b), "rechnung_kunde_12.pdf");
        assert_eq!(dateiname(&r("12", "101"), &mut b), "rechnung_kunde_12_101.pdf");
        assert_eq!(dateiname(&r("12", "101"), &mut b), "rechnung_kunde_12_101_2.pdf");
        assert_eq!(dateiname(&r("A/B", ""), &mut b), "rechnung_kunde_A_B.pdf");
    }

    /// B2M_PDF=<datei> cargo test real_groesse -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_groesse() {
        let Ok(path) = std::env::var("B2M_PDF") else { return };
        let bytes = std::fs::read(path).unwrap();
        let s = Splitter::laden(&bytes).unwrap();
        let t = std::time::Instant::now();
        let mut d = s.teil(0, 0).unwrap();
        let mut out = Vec::new();
        d.save_to(&mut out).unwrap();
        println!("Original {} KB ({} Seiten) -> Seite 1: {} KB, {:?}", bytes.len() / 1024, s.seiten.len(), out.len() / 1024, t.elapsed());
        assert_eq!(Splitter::laden(&out).unwrap().seiten.len(), 1);
    }
}
