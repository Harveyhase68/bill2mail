//! XLSX-Protokoll im selben Ordner und mit demselben Namen wie das Sammel-PDF.

use rust_xlsxwriter::{Format, Workbook, XlsxError};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Gesendet,
    Uebersprungen,
    Fehler,
}

impl Status {
    pub fn text(self) -> &'static str {
        match self {
            Status::Gesendet => crate::i18n::tr_static("Gesendet", "Sent"),
            Status::Uebersprungen => crate::i18n::tr_static("Übersprungen", "Skipped"),
            Status::Fehler => crate::i18n::tr_static("Fehler", "Error"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Zeile {
    pub nr: usize,
    pub kundennummer: String,
    pub kundenname: String,
    pub rechnungsnummer: String,
    pub seiten: String,
    pub email: String,
    pub status: Status,
    pub fehler: String,
    pub datei: String,
}

fn schreiben(pfad: &Path, quelle: &Path, zeilen: &[Zeile]) -> Result<(), XlsxError> {
    let kopf = Format::new().set_bold();
    let mut wb = Workbook::new();

    let ws = wb.add_worksheet();
    ws.set_name(crate::i18n::tr_static("Protokoll", "Log"))?;
    let spalten = if crate::i18n::ist_englisch() {
        ["No.", "Customer no.", "Customer name", "Invoice no.", "Pages", "Email", "Status", "Error / note", "File"]
    } else {
        ["Nr.", "Kundennummer", "Kundenname", "Rechnungsnummer", "Seiten", "E-Mail", "Status", "Fehler / Hinweis", "Datei"]
    };
    let breiten = [6.0, 14.0, 32.0, 16.0, 9.0, 36.0, 14.0, 50.0, 32.0];
    for (i, (s, b)) in spalten.iter().zip(breiten).enumerate() {
        ws.write_string_with_format(0, i as u16, *s, &kopf)?;
        ws.set_column_width(i as u16, b)?;
    }
    for (i, z) in zeilen.iter().enumerate() {
        let r = i as u32 + 1;
        ws.write_number(r, 0, z.nr as f64)?;
        // Kundennummer als Text, damit führende Nullen erhalten bleiben
        ws.write_string(r, 1, &z.kundennummer)?;
        ws.write_string(r, 2, &z.kundenname)?;
        ws.write_string(r, 3, &z.rechnungsnummer)?;
        ws.write_string(r, 4, &z.seiten)?;
        ws.write_string(r, 5, z.email.replace(';', "; "))?;
        ws.write_string(r, 6, z.status.text())?;
        ws.write_string(r, 7, &z.fehler)?;
        ws.write_string(r, 8, &z.datei)?;
    }
    ws.set_freeze_panes(1, 0)?;
    if !zeilen.is_empty() {
        ws.autofilter(0, 0, zeilen.len() as u32, spalten.len() as u16 - 1)?;
    }

    let zaehle = |s: Status| zeilen.iter().filter(|z| z.status == s).count() as f64;
    let ws = wb.add_worksheet();
    ws.set_name(crate::i18n::tr_static("Zusammenfassung", "Summary"))?;
    ws.set_column_width(0, 22.0)?;
    ws.set_column_width(1, 60.0)?;
    let jetzt = chrono::Local::now().format("%d.%m.%Y %H:%M:%S").to_string();
    ws.write_string(0, 0, crate::i18n::tr_static("Quelle", "Source"))?;
    ws.write_string(0, 1, quelle.display().to_string())?;
    ws.write_string(1, 0, crate::i18n::tr_static("Versendet am", "Sent on"))?;
    ws.write_string(1, 1, &jetzt)?;
    ws.write_string(2, 0, crate::i18n::tr_static("Rechnungen gesamt", "Invoices total"))?;
    ws.write_number(2, 1, zeilen.len() as f64)?;
    ws.write_string(3, 0, crate::i18n::tr_static("Gesendet", "Sent"))?;
    ws.write_number(3, 1, zaehle(Status::Gesendet))?;
    ws.write_string(4, 0, crate::i18n::tr_static("Übersprungen", "Skipped"))?;
    ws.write_number(4, 1, zaehle(Status::Uebersprungen))?;
    ws.write_string(5, 0, crate::i18n::tr_static("Fehler", "Error"))?;
    ws.write_number(5, 1, zaehle(Status::Fehler))?;

    wb.save(pfad)
}

/// Schreibt `<name>.xlsx` neben die PDF. Ist die Datei gesperrt (z. B. in Excel geöffnet),
/// wird ein Zeitstempel angehängt statt abzubrechen.
pub fn speichern(quelle: &Path, zeilen: &[Zeile]) -> Result<PathBuf, String> {
    let ziel = quelle.with_extension("xlsx");
    match schreiben(&ziel, quelle, zeilen) {
        Ok(()) => Ok(ziel),
        Err(_) => {
            let stem = quelle.file_stem().unwrap_or_default().to_string_lossy();
            let alt = ziel.with_file_name(format!("{stem}_{}.xlsx", chrono::Local::now().format("%Y%m%d_%H%M%S")));
            schreiben(&alt, quelle, zeilen).map_err(|e| crate::i18n::tr(&format!("Protokoll konnte nicht gespeichert werden: {e}"), &format!("Log could not be saved: {e}")))?;
            Ok(alt)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protokoll_wird_neben_die_pdf_geschrieben() {
        let dir = std::env::temp_dir().join(format!("b2m_xlsx_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("Sammel Jänner.pdf");
        let z = vec![Zeile {
            nr: 1,
            kundennummer: "007".into(),
            kundenname: "Müller & Söhne".into(),
            rechnungsnummer: "100".into(),
            seiten: "1–2".into(),
            email: "a@x.at;b@x.at".into(),
            status: Status::Gesendet,
            fehler: String::new(),
            datei: "rechnung_kunde_007.pdf".into(),
        }];
        let pfad = speichern(&pdf, &z).unwrap();
        assert_eq!(pfad, dir.join("Sammel Jänner.xlsx"));
        assert!(std::fs::metadata(&pfad).unwrap().len() > 1000);
        // Zweiter Lauf überschreibt dieselbe Datei
        assert_eq!(speichern(&pdf, &[]).unwrap(), pfad);
        std::fs::remove_dir_all(dir).ok();
    }
}
