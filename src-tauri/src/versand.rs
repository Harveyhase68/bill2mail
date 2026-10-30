//! Versandplan (Vorschau) und Ausführung: zerlegen, senden, protokollieren.

use crate::mail::{betreff_mit_name, empfaenger_pruefen, Mailer};
use crate::parser::{knr_norm, Analyse, Rechnung};
use crate::protokoll::{self, Status, Zeile};
use crate::settings::Einstellungen;
use crate::splitter::{self, Splitter};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Serialize)]
pub struct GeplanteMail {
    pub kundennummer: String,
    pub kundenname: String,
    pub email: String,
    /// `Rechnung::nr` der enthaltenen Rechnungen
    pub rechnungen: Vec<usize>,
    pub seiten: usize,
    pub betreff: String,
    pub senden: bool,
    /// Warum nicht gesendet wird (leer, wenn `senden`)
    pub grund: String,
}

/// Inhalts-Hash des Sammel-PDFs (SHA-256, hex) für den Doppelversand-Schutz.
pub fn pdf_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn seitentext(r: &Rechnung) -> String {
    if r.start_page == r.end_page {
        format!("{}", r.start_page + 1)
    } else {
        format!("{}–{}", r.start_page + 1, r.end_page + 1)
    }
}

pub fn anzeigename(r: &Rechnung) -> &str {
    if r.kundenname.trim().is_empty() { &r.kundennummer } else { &r.kundenname }
}

/// Eine E-Mail pro Kunde; hat ein Kunde mehrere Rechnungen, hängen alle an derselben Mail.
pub fn plan(rechnungen: &[Rechnung], e: &Einstellungen) -> Vec<GeplanteMail> {
    let mut mails: Vec<GeplanteMail> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();

    for r in rechnungen {
        let schluessel = if r.kundennummer.is_empty() { format!("#{}", r.nr) } else { knr_norm(&r.kundennummer) };
        let seiten = r.end_page - r.start_page + 1;
        if let Some(&i) = index.get(&schluessel) {
            mails[i].rechnungen.push(r.nr);
            mails[i].seiten += seiten;
            continue;
        }
        let grund = if r.kundennummer.is_empty() {
            crate::i18n::tr("Kundennummer nicht gefunden", "Customer number not found")
        } else if !r.in_liste {
            crate::i18n::tr("Kunde nicht in der Kundenliste", "Customer not in the customer list")
        } else {
            empfaenger_pruefen(&r.email).err().unwrap_or_default()
        };
        index.insert(schluessel, mails.len());
        mails.push(GeplanteMail {
            kundennummer: r.kundennummer.clone(),
            kundenname: anzeigename(r).to_string(),
            email: r.email.clone(),
            rechnungen: vec![r.nr],
            seiten,
            betreff: betreff_mit_name(&e.betreff, anzeigename(r)),
            senden: grund.is_empty(),
            grund,
        });
    }
    mails
}

#[derive(Debug, Serialize)]
pub struct Ergebnis {
    pub zeilen: Vec<Zeile>,
    pub mails_gesendet: usize,
    pub mails_fehler: usize,
    pub rechnungen_uebersprungen: usize,
    pub abgebrochen: bool,
    pub protokoll: String,
    pub ordner: String,
}

/// Führt den Versand aus. `fortschritt(aktuell, gesamt, text)` wird vor jeder Mail aufgerufen.
/// Schlägt die SMTP-Verbindung fehl, wird abgebrochen, bevor Dateien erzeugt werden.
pub fn ausfuehren(
    quelle: &Path,
    bytes: &[u8],
    analyse: &Analyse,
    e: &Einstellungen,
    passwort: Option<&str>,
    abbrechen: &AtomicBool,
    fortschritt: &mut dyn FnMut(usize, usize, &str),
) -> Result<Ergebnis, String> {
    let mails = plan(&analyse.rechnungen, e);
    let zu_senden = mails.iter().filter(|m| m.senden).count();
    let mailer = if zu_senden > 0 { Some(Mailer::neu(e, passwort)?) } else { None };

    // 1. Zerlegen
    let splitter = Splitter::laden(bytes)?;
    let ordner = splitter::ausgabeordner(quelle);
    std::fs::create_dir_all(&ordner).map_err(|err| crate::i18n::tr(&format!("Ordner konnte nicht angelegt werden: {err}"), &format!("Folder could not be created: {err}")))?;
    let mut benutzt = HashSet::new();
    let mut dateien: Vec<Result<PathBuf, String>> = Vec::new();
    for r in &analyse.rechnungen {
        let pfad = ordner.join(splitter::dateiname(r, &mut benutzt));
        let ergebnis = splitter
            .teil(r.start_page, r.end_page)
            .and_then(|mut d| d.save(&pfad).map(|_| pfad.clone()).map_err(|err| crate::i18n::tr(&format!("PDF konnte nicht gespeichert werden: {err}"), &format!("PDF could not be saved: {err}"))));
        dateien.push(ergebnis);
    }

    // 2. Zeilen vorbelegen
    let mut zeilen: Vec<Zeile> = analyse
        .rechnungen
        .iter()
        .zip(&dateien)
        .map(|(r, d)| Zeile {
            nr: r.nr,
            kundennummer: r.kundennummer.clone(),
            kundenname: anzeigename(r).to_string(),
            rechnungsnummer: r.rechnungsnummer.clone(),
            seiten: seitentext(r),
            email: r.email.clone(),
            status: Status::Uebersprungen,
            fehler: String::new(),
            datei: d.as_ref().ok().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        })
        .collect();
    let zeile_von = |nr: usize| nr - 1;
    for m in mails.iter().filter(|m| !m.senden) {
        for &nr in &m.rechnungen {
            zeilen[zeile_von(nr)].fehler = m.grund.clone();
        }
    }

    // 3. Senden
    let (mut gesendet, mut fehler, mut abgebrochen) = (0, 0, false);
    let mut nr_mail = 0;
    for m in mails.iter().filter(|m| m.senden) {
        if abbrechen.load(Ordering::SeqCst) {
            abgebrochen = true;
            for &nr in &m.rechnungen {
                zeilen[zeile_von(nr)].fehler = crate::i18n::tr("Abgebrochen", "Aborted");
            }
            continue;
        }
        nr_mail += 1;
        fortschritt(nr_mail, zu_senden, &format!("{} ({})", m.kundenname, m.email.replace(';', ", ")));

        let pdfs: Result<Vec<PathBuf>, String> =
            m.rechnungen.iter().map(|&nr| dateien[zeile_von(nr)].clone()).collect();
        let ergebnis = pdfs.and_then(|p| mailer.as_ref().unwrap().senden(&m.email, &m.kundenname, &p));
        let (status, text) = match ergebnis {
            Ok(()) => {
                gesendet += 1;
                (Status::Gesendet, String::new())
            }
            Err(err) => {
                fehler += 1;
                (Status::Fehler, err)
            }
        };
        for &nr in &m.rechnungen {
            let z = &mut zeilen[zeile_von(nr)];
            z.status = status;
            z.fehler = text.clone();
        }
    }

    // 4. Protokoll
    let protokoll = protokoll::speichern(quelle, &zeilen)?;
    let uebersprungen = zeilen.iter().filter(|z| z.status == Status::Uebersprungen).count();
    Ok(Ergebnis {
        zeilen,
        mails_gesendet: gesendet,
        mails_fehler: fehler,
        rechnungen_uebersprungen: uebersprungen,
        abgebrochen,
        protokoll: protokoll.display().to_string(),
        ordner: ordner.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::testserver;
    use crate::parser::{analysiere_pdf, mit_kunden_abgleichen, FrameDef, FrameType};
    use crate::db::Kunde;
    use crate::pdf_text::testutil::test_pdf;

    fn re(nr: usize, knr: &str, in_liste: bool, email: &str, start: usize, end: usize) -> Rechnung {
        Rechnung {
            nr,
            kundennummer: knr.into(),
            kundenname: format!("Kunde {knr}"),
            in_liste,
            email: email.into(),
            start_page: start,
            end_page: end,
            ..Default::default()
        }
    }

    #[test]
    fn plan_fasst_rechnungen_pro_kunde_zusammen() {
        let e = Einstellungen { betreff: "Re {KUNDENNAME}".into(), ..Default::default() };
        let r = vec![
            re(1, "1", true, "a@x.at", 0, 1),
            re(2, "2", true, "", 2, 2),
            re(3, "01", true, "a@x.at", 3, 3), // gleicher Kunde wie 1
            re(4, "9", false, "", 4, 4),
            re(5, "", false, "", 5, 5),
            re(6, "3", true, "kaputt", 6, 6),
        ];
        let p = plan(&r, &e);
        assert_eq!(p.len(), 5);
        assert!(p[0].senden && p[0].rechnungen == vec![1, 3] && p[0].seiten == 3);
        assert_eq!(p[0].betreff, "Re Kunde 1");
        assert!(!p[1].senden && p[1].grund.contains("E-Mail"));
        assert!(p[2].grund.contains("Kundenliste"));
        assert!(p[3].grund.contains("Kundennummer"));
        assert!(!p[4].senden && p[4].grund.contains("Ungültig"));
    }

    fn frame(kind: FrameType, x: f64, y: f64, w: f64, params: &str) -> FrameDef {
        FrameDef { kind, page: 0, x, y: y - 12.0, w, h: 16.0, params: params.into() }
    }

    #[test]
    fn kompletter_ablauf_gegen_lokalen_smtp_server() {
        let seite = |re: &'static str, kd: &'static str, s: &'static str| -> Vec<(f64, f64, &'static str)> {
            vec![(10.0, 50.0, re), (10.0, 80.0, kd), (400.0, 80.0, s)]
        };
        let (s1, s2, s3) = (
            seite("RECHNUNG 100", "Kundennummer: 1", "Seite: 1"),
            seite("RECHNUNG 100", "Kundennummer: 1", "Seite: 2"),
            seite("RECHNUNG 101", "Kundennummer: 2", "Seite: 1"),
        );
        let bytes = test_pdf(&[&s1, &s2, &s3]);
        let dir = std::env::temp_dir().join(format!("b2m_versand_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let quelle = dir.join("Sammel.pdf");
        std::fs::write(&quelle, &bytes).unwrap();

        let frames = vec![
            frame(FrameType::Rot, 5.0, 50.0, 200.0, "RECHNUNG*[RE]"),
            frame(FrameType::Blau, 5.0, 80.0, 200.0, "Kundennummer*[KU]"),
            frame(FrameType::Violett, 395.0, 80.0, 100.0, "Seite*[SE]"),
        ];
        let mut analyse = analysiere_pdf(&bytes, &frames, crate::parser::TOLERANZ_STANDARD_PT).unwrap();
        let kunden = vec![Kunde { id: 1, kundennummer: "1".into(), kundenname: "Muster GmbH".into(), email: "a@kunde.at".into() }];
        mit_kunden_abgleichen(&mut analyse.rechnungen, &kunden);
        assert_eq!(analyse.rechnungen.len(), 2, "{:?}", analyse.warnungen);
        let (port, mails) = testserver::start();
        let e = testserver::einstellungen(port);
        let mut schritte = Vec::new();
        let erg = ausfuehren(&quelle, &bytes, &analyse, &e, None, &AtomicBool::new(false), &mut |i, n, t| {
            schritte.push((i, n, t.to_string()))
        })
        .unwrap();
        assert_eq!((erg.mails_gesendet, erg.mails_fehler, erg.rechnungen_uebersprungen), (1, 0, 1));
        assert_eq!(schritte.len(), 1);
        assert_eq!(erg.zeilen[0].status, Status::Gesendet);
        assert_eq!(erg.zeilen[1].status, Status::Uebersprungen);
        assert!(erg.zeilen[1].fehler.contains("Kundenliste"));

        {
            let gesendet = mails.lock().unwrap();
            assert_eq!(gesendet.len(), 1);
            assert!(gesendet[0].contains("a@kunde.at") && gesendet[0].contains("rechnung_kunde_1.pdf"));
            assert!(!gesendet[0].contains("rechnung_kunde_2.pdf"));
        }

        // Ordner mit zerlegten PDFs und Protokoll neben der Quelle
        let ordner = dir.join("Sammel_gesendet");
        assert!(ordner.join("rechnung_kunde_1.pdf").exists() && ordner.join("rechnung_kunde_2.pdf").exists());
        let eins = Splitter::laden(&std::fs::read(ordner.join("rechnung_kunde_1.pdf")).unwrap()).unwrap();
        assert_eq!(eins.teil(0, 1).map(|d| d.get_pages().len()).unwrap(), 2);
        assert!(dir.join("Sammel.xlsx").exists());

        // Abbrechen: nichts wird gesendet, Rechnungen stehen als abgebrochen im Protokoll
        let erg = ausfuehren(&quelle, &bytes, &analyse, &e, None, &AtomicBool::new(true), &mut |_, _, _| {}).unwrap();
        assert!(erg.abgebrochen && erg.mails_gesendet == 0);
        assert_eq!(erg.zeilen[0].fehler, "Abgebrochen");
        assert_eq!(mails.lock().unwrap().len(), 1);

        // Nicht erreichbarer Server: Abbruch, bevor Dateien erzeugt werden
        std::fs::remove_dir_all(&ordner).unwrap();
        let schlecht = testserver::einstellungen(1);
        assert!(ausfuehren(&quelle, &bytes, &analyse, &schlecht, None, &AtomicBool::new(false), &mut |_, _, _| {}).is_err());
        assert!(!ordner.exists());
        std::fs::remove_dir_all(dir).ok();
    }
}
