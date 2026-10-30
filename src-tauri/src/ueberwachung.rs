//! Überwacht einen Ordner auf neue PDFs. Gemeldet wird eine Datei erst, wenn sie nicht mehr
//! von einem anderen Programm gesperrt ist und ihre Größe sich nicht mehr ändert.

use notify::{recommended_watcher, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Meldung {
    /// Die PDF ist fertig geschrieben und lesbar.
    Bereit(PathBuf),
    /// Die PDF blieb zu lange gesperrt.
    Fehler(PathBuf, String),
}

#[derive(Debug, Clone, Copy)]
pub struct Zeiten {
    /// So lange muss es nach dem letzten Dateiereignis ruhig sein
    pub ruhe: Duration,
    /// Abstand der Prüfungen (Sperre, Größe)
    pub takt: Duration,
    /// Danach wird aufgegeben
    pub max_warten: Duration,
}

impl Default for Zeiten {
    fn default() -> Self {
        Zeiten { ruhe: Duration::from_millis(1500), takt: Duration::from_secs(1), max_warten: Duration::from_secs(600) }
    }
}

/// Der Watcher läuft, solange dieses Objekt lebt.
pub struct Ueberwachung {
    _watcher: RecommendedWatcher,
}

/// Windows meldet eine von einem anderen Programm exklusiv geöffnete Datei als
/// Sharing-/Lock-Violation (32/33).
fn gesperrt(pfad: &Path) -> bool {
    match OpenOptions::new().read(true).write(true).open(pfad) {
        Ok(_) => false,
        Err(e) => matches!(e.raw_os_error(), Some(32) | Some(33)),
    }
}

fn ist_kandidat(pfad: &Path) -> bool {
    let name = pfad.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    pfad.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) && !name.starts_with('~') && !name.starts_with('.')
}

struct Kandidat {
    letztes_ereignis: Instant,
    erstmals: Instant,
    letzte_pruefung: Instant,
    groesse: Option<u64>,
}

pub fn starten(
    ordner: &Path,
    zeiten: Zeiten,
    melden: impl Fn(Meldung) + Send + 'static,
) -> Result<Ueberwachung, String> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            let _ = tx.send(ev);
        }
    })
    .map_err(|e| format!("Ordnerüberwachung konnte nicht gestartet werden: {e}"))?;
    watcher
        .watch(ordner, RecursiveMode::NonRecursive)
        .map_err(|e| format!("Ordner kann nicht überwacht werden: {e}"))?;

    std::thread::spawn(move || {
        let mut offen: HashMap<PathBuf, Kandidat> = HashMap::new();
        let mut erledigt: HashSet<(PathBuf, u64, SystemTime)> = HashSet::new();
        loop {
            let ereignis = match rx.recv_timeout(zeiten.takt / 2) {
                Ok(ev) => Some(ev),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            let jetzt = Instant::now();
            for ev in ereignis.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
                if !matches!(ev.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    continue;
                }
                for p in ev.paths.into_iter().filter(|p| ist_kandidat(p)) {
                    offen
                        .entry(p)
                        .and_modify(|k| k.letztes_ereignis = jetzt)
                        .or_insert(Kandidat {
                            letztes_ereignis: jetzt,
                            erstmals: jetzt,
                            letzte_pruefung: jetzt - zeiten.takt,
                            groesse: None,
                        });
                }
            }

            let mut fertig = Vec::new();
            for (pfad, k) in offen.iter_mut() {
                let jetzt = Instant::now();
                if jetzt.duration_since(k.letztes_ereignis) < zeiten.ruhe || jetzt.duration_since(k.letzte_pruefung) < zeiten.takt {
                    continue;
                }
                k.letzte_pruefung = jetzt;
                let Ok(meta) = std::fs::metadata(pfad) else {
                    fertig.push((pfad.clone(), None)); // inzwischen gelöscht oder umbenannt
                    continue;
                };
                if !meta.is_file() {
                    fertig.push((pfad.clone(), None));
                    continue;
                }
                let laenge = meta.len();
                let frei = !gesperrt(pfad);
                let stabil = frei && laenge > 0 && k.groesse == Some(laenge);
                k.groesse = if frei { Some(laenge) } else { None };
                if stabil {
                    let key = (pfad.clone(), laenge, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));
                    let neu = erledigt.insert(key);
                    fertig.push((pfad.clone(), neu.then_some(Meldung::Bereit(pfad.clone()))));
                } else if jetzt.duration_since(k.erstmals) > zeiten.max_warten {
                    let grund = if frei { "Die Datei hat sich nicht stabilisiert." } else { "Die Datei blieb gesperrt (von einem anderen Programm geöffnet)." };
                    fertig.push((pfad.clone(), Some(Meldung::Fehler(pfad.clone(), grund.into()))));
                }
            }
            for (pfad, meldung) in fertig {
                offen.remove(&pfad);
                if let Some(m) = meldung {
                    melden(m);
                }
            }
        }
    });

    Ok(Ueberwachung { _watcher: watcher })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::mpsc::Receiver;

    fn schnell() -> Zeiten {
        Zeiten { ruhe: Duration::from_millis(200), takt: Duration::from_millis(100), max_warten: Duration::from_secs(20) }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("b2m_watch_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn start(dir: &Path, zeiten: Zeiten) -> (Ueberwachung, Receiver<(Meldung, Instant)>) {
        let (tx, rx) = mpsc::channel();
        let u = starten(dir, zeiten, move |m| {
            let _ = tx.send((m, Instant::now()));
        })
        .unwrap();
        (u, rx)
    }

    #[test]
    fn wachsende_datei_wird_erst_nach_dem_schreiben_gemeldet() {
        let dir = temp("wachsen");
        let (_u, rx) = start(&dir, schnell());
        let pdf = dir.join("Sammel.pdf");
        let mut f = std::fs::File::create(&pdf).unwrap();
        for _ in 0..6 {
            f.write_all(&[b'x'; 1000]).unwrap();
            f.flush().unwrap();
            std::thread::sleep(Duration::from_millis(250));
        }
        let ende = Instant::now();
        drop(f);
        let (m, wann) = rx.recv_timeout(Duration::from_secs(10)).expect("keine Meldung");
        assert_eq!(m, Meldung::Bereit(pdf));
        assert!(wann >= ende, "wurde gemeldet, bevor die Datei fertig war");
        assert!(rx.recv_timeout(Duration::from_secs(1)).is_err(), "doppelt gemeldet");
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(windows)]
    #[test]
    fn gesperrte_datei_wird_erst_nach_freigabe_gemeldet() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = temp("sperre");
        let (_u, rx) = start(&dir, schnell());
        let pdf = dir.join("gesperrt.pdf");
        // wie ein Druckprogramm: schreibt, teilt nur zum Lesen, Größe bleibt zwischendurch gleich
        let mut f = OpenOptions::new().write(true).create(true).share_mode(1).open(&pdf).unwrap();
        f.write_all(&[b'x'; 5000]).unwrap();
        f.flush().unwrap();
        assert!(gesperrt(&pdf));
        assert!(rx.recv_timeout(Duration::from_millis(1500)).is_err(), "gemeldet, obwohl gesperrt");
        let frei = Instant::now();
        drop(f);
        let (m, wann) = rx.recv_timeout(Duration::from_secs(10)).expect("nach Freigabe keine Meldung");
        assert_eq!(m, Meldung::Bereit(pdf));
        assert!(wann >= frei);
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(windows)]
    #[test]
    fn dauerhaft_gesperrte_datei_wird_als_fehler_gemeldet() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = temp("timeout");
        let z = Zeiten { max_warten: Duration::from_secs(1), ..schnell() };
        let (_u, rx) = start(&dir, z);
        let pdf = dir.join("haengt.pdf");
        let mut f = OpenOptions::new().write(true).create(true).share_mode(1).open(&pdf).unwrap();
        f.write_all(b"x").unwrap();
        let (m, _) = rx.recv_timeout(Duration::from_secs(10)).expect("keine Fehlermeldung");
        assert!(matches!(m, Meldung::Fehler(p, g) if p == pdf && g.contains("gesperrt")));
        drop(f);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn andere_dateien_und_unterordner_werden_ignoriert() {
        let dir = temp("ignorieren");
        let (_u, rx) = start(&dir, schnell());
        std::fs::write(dir.join("notiz.txt"), b"x").unwrap();
        std::fs::write(dir.join("~$temp.pdf"), b"x").unwrap();
        std::fs::write(dir.join("Protokoll.xlsx"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("Sammel_gesendet")).unwrap();
        std::fs::write(dir.join("Sammel_gesendet").join("rechnung_kunde_1.pdf"), b"x").unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_err());
        // eine echte PDF danach wird gefunden
        let pdf = dir.join("Neu.PDF");
        std::fs::write(&pdf, b"%PDF").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap().0, Meldung::Bereit(pdf));
        std::fs::remove_dir_all(dir).ok();
    }
}
