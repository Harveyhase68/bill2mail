mod db;
mod i18n;
mod lizenz;
mod mail;
mod parser;
mod pdf_text;
mod protokoll;
mod settings;
mod splitter;
mod ueberwachung;
mod versand;

use db::{Database, Kunde};
use i18n::tr;
use parser::{Analyse, FrameDef};
use serde::Serialize;
use settings::Einstellungen;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{ipc::Response, AppHandle, Emitter, Manager, State};
use versand::{Ergebnis, GeplanteMail};

const STANDARD_VORLAGE: &str = "Standard";

struct AppState {
    db: Mutex<Database>,
    abbrechen: Arc<AtomicBool>,
    ueberwachung: Mutex<Option<ueberwachung::Ueberwachung>>,
    ueberwachung_status: Mutex<UeberwachungsStatus>,
    datenordner: std::path::PathBuf,
}

#[derive(Serialize, Clone, Default)]
struct UeberwachungsStatus {
    aktiv: bool,
    meldung: String,
}

#[derive(Serialize, Clone)]
struct NeuePdf {
    pfad: String,
    /// Nur bei Fehlern gesetzt (z. B. Datei blieb gesperrt)
    fehler: Option<String>,
}

/// Holt das Hauptfenster in den Vordergrund, damit neue Rechnungen nicht übersehen werden.
fn fenster_nach_vorne(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Startet bzw. stoppt die Ordnerüberwachung passend zu den Einstellungen.
fn ueberwachung_anwenden(app: &AppHandle, e: &Einstellungen) -> UeberwachungsStatus {
    let state = app.state::<AppState>();
    let mut slot = state.ueberwachung.lock().unwrap();
    *slot = None; // beendet einen laufenden Watcher

    let status = if !e.ueberwachung_aktiv {
        UeberwachungsStatus::default()
    } else if let Err(grund) = lizenz::laden(&state.datenordner) {
        UeberwachungsStatus { aktiv: false, meldung: format!("{grund} {}", lizenz::erforderlich()) }
    } else {
        let ordner = std::path::PathBuf::from(e.ueberwachung_ordner.trim());
        if e.ueberwachung_ordner.trim().is_empty() {
            UeberwachungsStatus { aktiv: false, meldung: tr("Kein Ordner gewählt – die Überwachung ist aus.", "No folder selected – monitoring is off.") }
        } else if !ordner.is_dir() {
            UeberwachungsStatus { aktiv: false, meldung: tr(&format!("Ordner nicht gefunden: {}", ordner.display()), &format!("Folder not found: {}", ordner.display())) }
        } else {
            let app2 = app.clone();
            let gestartet = ueberwachung::starten(&ordner, ueberwachung::Zeiten::default(), move |m| {
                let ereignis = match m {
                    ueberwachung::Meldung::Bereit(p) => NeuePdf { pfad: p.display().to_string(), fehler: None },
                    ueberwachung::Meldung::Fehler(p, f) => NeuePdf { pfad: p.display().to_string(), fehler: Some(f) },
                };
                fenster_nach_vorne(&app2);
                let _ = app2.emit("neue-pdf", ereignis);
            });
            match gestartet {
                Ok(u) => {
                    *slot = Some(u);
                    UeberwachungsStatus { aktiv: true, meldung: tr(&format!("Überwache {}", ordner.display()), &format!("Monitoring {}", ordner.display())) }
                }
                Err(f) => UeberwachungsStatus { aktiv: false, meldung: f },
            }
        }
    };
    *state.ueberwachung_status.lock().unwrap() = status.clone();
    status
}

fn fehler<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Liefert die rohen PDF-Bytes an die Webview (pdf.js rendert dort).
#[tauri::command]
fn pdf_lesen(path: String) -> Result<Response, String> {
    std::fs::read(&path)
        .map(Response::new)
        .map_err(|e| tr(&format!("PDF konnte nicht gelesen werden: {e}"), &format!("PDF could not be read: {e}")))
}

fn pdf_analysieren(bytes: &[u8], frames: &[FrameDef], kunden: &[Kunde], toleranz_pt: f64) -> Result<Analyse, String> {
    let mut analyse = parser::analysiere_pdf(bytes, frames, toleranz_pt)?;
    parser::mit_kunden_abgleichen(&mut analyse.rechnungen, kunden);
    Ok(analyse)
}

/// Wendet die (ggf. noch ungespeicherten) Rahmen auf das PDF an und zerlegt es in Rechnungen.
#[tauri::command]
async fn analysiere(state: State<'_, AppState>, path: String, frames: Vec<FrameDef>) -> Result<Analyse, String> {
    let (kunden, toleranz) = {
        let db = state.db.lock().unwrap();
        (db.alle_kunden().map_err(fehler)?, db.einstellungen_laden().map_err(fehler)?.toleranz_pt)
    };
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("PDF konnte nicht gelesen werden: {e}"))?;
        pdf_analysieren(&bytes, &frames, &kunden, toleranz)
    })
    .await
    .map_err(fehler)?
}

// ------------------------------------------------------------- Einstellungen

#[derive(Serialize)]
struct EinstellungenDto {
    #[serde(flatten)]
    einstellungen: Einstellungen,
    passwort_gesetzt: bool,
    ueberwachung: UeberwachungsStatus,
}

#[tauri::command]
fn einstellungen_laden(state: State<AppState>) -> Result<EinstellungenDto, String> {
    let einstellungen = state.db.lock().unwrap().einstellungen_laden().map_err(fehler)?;
    Ok(EinstellungenDto {
        einstellungen,
        passwort_gesetzt: settings::passwort_lesen().is_some(),
        ueberwachung: state.ueberwachung_status.lock().unwrap().clone(),
    })
}

/// `passwort`: `None` lässt das gespeicherte Passwort unverändert.
#[tauri::command]
fn einstellungen_speichern(
    app: AppHandle,
    state: State<AppState>,
    mut einstellungen: Einstellungen,
    passwort: Option<String>,
) -> Result<UeberwachungsStatus, String> {
    einstellungen.toleranz_pt = if einstellungen.toleranz_pt.is_finite() {
        einstellungen.toleranz_pt.clamp(0.0, parser::TOLERANZ_MAX_PT)
    } else {
        parser::TOLERANZ_STANDARD_PT
    };
    if let Some(p) = passwort {
        settings::passwort_speichern(&p)?;
    }
    state.db.lock().unwrap().einstellungen_speichern(&einstellungen).map_err(fehler)?;
    Ok(ueberwachung_anwenden(&app, &einstellungen))
}

/// Schickt eine Test-Mail (echter Betreff/Text, Platzhalter-Name) an die Test-Adresse.
#[tauri::command]
async fn test_mail_senden(state: State<'_, AppState>) -> Result<String, String> {
    let e = state.db.lock().unwrap().einstellungen_laden().map_err(fehler)?;
    let empfaenger = e.test_empfaenger.trim().to_string();
    if empfaenger.is_empty() {
        return Err(tr("Bitte zuerst eine Test-Empfängeradresse in den Einstellungen eintragen.", "Please enter a test recipient address in the settings first."));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let m = mail::Mailer::neu(&e, settings::passwort_lesen().as_deref())?;
        m.test(&empfaenger)?;
        Ok(format!("Test-Mail an {empfaenger} gesendet."))
    })
    .await
    .map_err(fehler)?
}

// ------------------------------------------------------------------ Versand

#[derive(Serialize)]
struct Vorschau {
    analyse: Analyse,
    mails: Vec<GeplanteMail>,
    betreff: String,
    /// So sieht der Text der ersten Mail aus
    text_html: String,
    absender: String,
    /// Gesetzt, wenn genau dieser PDF-Inhalt schon einmal versendet wurde
    bereits_gesendet: Option<String>,
}

/// Zeigt, was versendet würde, ohne etwas zu senden oder Dateien zu erzeugen.
#[tauri::command]
async fn versand_vorschau(state: State<'_, AppState>, path: String, frames: Vec<FrameDef>) -> Result<Vorschau, String> {
    let (kunden, e) = {
        let db = state.db.lock().unwrap();
        (db.alle_kunden().map_err(fehler)?, db.einstellungen_laden().map_err(fehler)?)
    };
    let (mut vorschau, hash) = tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("PDF konnte nicht gelesen werden: {e}"))?;
        let analyse = pdf_analysieren(&bytes, &frames, &kunden, e.toleranz_pt)?;
        let mails = versand::plan(&analyse.rechnungen, &e);
        let name = mails.iter().find(|m| m.senden).map(|m| m.kundenname.clone()).unwrap_or_else(|| "Max Mustermann".into());
        let absender = if e.absender_name.trim().is_empty() {
            e.absender.clone()
        } else {
            format!("{} <{}>", e.absender_name.trim(), e.absender)
        };
        let vorschau = Vorschau {
            betreff: mail::betreff_mit_name(&e.betreff, &name),
            text_html: mail::text_mit_name(&e.text, &name),
            absender,
            analyse,
            mails,
            bereits_gesendet: None,
        };
        Ok::<_, String>((vorschau, versand::pdf_hash(&bytes)))
    })
    .await
    .map_err(fehler)??;
    vorschau.bereits_gesendet = state
        .db
        .lock()
        .unwrap()
        .versand_gefunden(&hash)
        .map_err(fehler)?
        .map(|(pfad, zeit)| tr(&format!("Dieses PDF wurde bereits am {zeit} versendet (Datei: {pfad})."), &format!("This PDF was already sent on {zeit} (file: {pfad}).")));
    Ok(vorschau)
}

#[derive(Serialize, Clone)]
struct Fortschritt {
    aktuell: usize,
    gesamt: usize,
    text: String,
}

/// Zerlegt das PDF, sendet pro Kunde eine Mail und schreibt das XLSX-Protokoll.
/// Fortschritt kommt als Event `versand-fortschritt`.
#[tauri::command]
async fn versenden(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    frames: Vec<FrameDef>,
) -> Result<Ergebnis, String> {
    lizenz::laden(&state.datenordner).map_err(|grund| format!("{grund} {}", lizenz::erforderlich()))?;
    let (kunden, e) = {
        let db = state.db.lock().unwrap();
        (db.alle_kunden().map_err(fehler)?, db.einstellungen_laden().map_err(fehler)?)
    };
    let abbrechen = state.abbrechen.clone();
    abbrechen.store(false, Ordering::SeqCst);
    let pfad = path.clone();
    let (ergebnis, hash) = tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("PDF konnte nicht gelesen werden: {e}"))?;
        let analyse = pdf_analysieren(&bytes, &frames, &kunden, e.toleranz_pt)?;
        let passwort = settings::passwort_lesen();
        let ergebnis = versand::ausfuehren(std::path::Path::new(&path), &bytes, &analyse, &e, passwort.as_deref(), &abbrechen, &mut |aktuell, gesamt, text| {
            let _ = app.emit("versand-fortschritt", Fortschritt { aktuell, gesamt, text: text.to_string() });
        })?;
        Ok::<_, String>((ergebnis, versand::pdf_hash(&bytes)))
    })
    .await
    .map_err(fehler)??;
    // Nur vermerken, wenn wirklich etwas rausging; ein reiner Abbruch sperrt das PDF nicht.
    if ergebnis.mails_gesendet > 0 {
        let zeit = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
        let _ = state.db.lock().unwrap().versand_vermerken(&hash, &pfad, &zeit, ergebnis.mails_gesendet);
    }
    Ok(ergebnis)
}

#[tauri::command]
fn versand_abbrechen(state: State<AppState>) {
    state.abbrechen.store(true, Ordering::SeqCst);
}

// ------------------------------------------------------------ Kunden/Vorlage

#[tauri::command]
fn kunden_laden(state: State<AppState>) -> Result<Vec<Kunde>, String> {
    state.db.lock().unwrap().alle_kunden().map_err(fehler)
}

#[tauri::command]
fn kunde_speichern(state: State<AppState>, kunde: Kunde) -> Result<i64, String> {
    if kunde.kundennummer.trim().is_empty() {
        return Err(tr("Die Kundennummer darf nicht leer sein.", "The customer number must not be empty."));
    }
    state.db.lock().unwrap().kunde_speichern(&kunde).map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            tr(&format!("Die Kundennummer {} existiert bereits.", kunde.kundennummer.trim()), &format!("Customer number {} already exists.", kunde.kundennummer.trim()))
        }
        other => other.to_string(),
    })
}

#[tauri::command]
fn kunde_loeschen(state: State<AppState>, id: i64) -> Result<(), String> {
    state.db.lock().unwrap().kunde_loeschen(id).map_err(fehler)
}

#[tauri::command]
fn vorlage_laden(state: State<AppState>) -> Result<Option<String>, String> {
    state.db.lock().unwrap().vorlage_laden(STANDARD_VORLAGE).map_err(fehler)
}

#[tauri::command]
fn vorlage_speichern(state: State<AppState>, json: String) -> Result<(), String> {
    state.db.lock().unwrap().vorlage_speichern(STANDARD_VORLAGE, &json).map_err(fehler)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = Database::open(&dir.join("bill2mail.db"))?;
            let einstellungen = db.einstellungen_laden().unwrap_or_default();
            app.manage(AppState {
                db: Mutex::new(db),
                abbrechen: Arc::new(AtomicBool::new(false)),
                ueberwachung: Mutex::new(None),
                ueberwachung_status: Mutex::new(UeberwachungsStatus::default()),
                datenordner: dir.clone(),
            });
            ueberwachung_anwenden(app.handle(), &einstellungen);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pdf_lesen,
            analysiere,
            einstellungen_laden,
            einstellungen_speichern,
            test_mail_senden,
            versand_vorschau,
            versenden,
            versand_abbrechen,
            kunden_laden,
            kunde_speichern,
            kunde_loeschen,
            vorlage_laden,
            vorlage_speichern,
            lizenz_status,
            lizenz_installieren,
            sprache_system,
            sprache_setzen,
        ])
        .run(tauri::generate_context!())
        .expect("Fehler beim Starten von Bill2Mail");
}

// ------------------------------------------------------- Lizenz und Sprache

#[tauri::command]
fn lizenz_status(state: State<AppState>) -> lizenz::LizenzStatus {
    lizenz::status(&state.datenordner)
}

/// Prueft die gewaehlte Lizenzdatei, installiert sie und startet ggf. die Ueberwachung.
#[tauri::command]
fn lizenz_installieren(app: AppHandle, state: State<AppState>, pfad: String) -> Result<lizenz::LizenzStatus, String> {
    lizenz::installieren(std::path::Path::new(&pfad), &state.datenordner)?;
    let e = state.db.lock().unwrap().einstellungen_laden().map_err(fehler)?;
    ueberwachung_anwenden(&app, &e);
    Ok(lizenz::status(&state.datenordner))
}

/// Windows-Anzeigesprache, z. B. "de-AT"
#[tauri::command]
fn sprache_system() -> String {
    i18n::systemsprache()
}

/// Sprache der Backend-Texte (Fehlermeldungen, Protokoll)
#[tauri::command]
fn sprache_setzen(englisch: bool) {
    i18n::setzen(englisch);
}
