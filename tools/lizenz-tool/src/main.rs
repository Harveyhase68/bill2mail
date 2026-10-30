//! Lizenz-Werkzeug fuer den Hersteller von Bill2Mail.
//!
//!   lizenz-tool schluessel                       neues Schluesselpaar anlegen (einmalig!)
//!   lizenz-tool oeffentlich                      oeffentlichen Schluessel als Rust-Array ausgeben
//!   lizenz-tool ausstellen "Firma" [--bis JJJJ-MM-TT] [--out datei.lic]
//!
//! Der private Schluessel liegt in %USERPROFILE%\.bill2mail\lizenz-privat.key (ausserhalb des
//! Projekts, NIE ins Repository, Sicherung anlegen!). Verlust = keine neuen Lizenzen mehr moeglich;
//! Weitergabe = jeder kann Lizenzen ausstellen.
//!
//! Dateiformat: "B2M1." + base64url(payload-JSON) + "." + base64url(Ed25519-Signatur ueber die Payload-Bytes)

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use ed25519_dalek::{Signer, SigningKey};
use std::path::PathBuf;

fn schluesselpfad() -> PathBuf {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).expect("Kein Benutzerordner gefunden");
    PathBuf::from(home).join(".bill2mail").join("lizenz-privat.key")
}

fn laden() -> SigningKey {
    let pfad = schluesselpfad();
    let hex = std::fs::read_to_string(&pfad).unwrap_or_else(|_| fehler(&format!("Kein privater Schluessel unter {}. Zuerst: lizenz-tool schluessel", pfad.display())));
    let bytes: Vec<u8> = (0..hex.trim().len() / 2).map(|i| u8::from_str_radix(&hex.trim()[2 * i..2 * i + 2], 16).unwrap_or_else(|_| fehler("Schluesseldatei beschaedigt"))).collect();
    let arr: [u8; 32] = bytes.try_into().unwrap_or_else(|_| fehler("Schluesseldatei hat falsche Laenge"));
    SigningKey::from_bytes(&arr)
}

fn fehler(msg: &str) -> ! {
    eprintln!("Fehler: {msg}");
    std::process::exit(1)
}

fn rust_array(key: &SigningKey) -> String {
    let b = key.verifying_key().to_bytes();
    let teile: Vec<String> = b.iter().map(|x| format!("0x{x:02x}")).collect();
    format!("[{}]", teile.join(", "))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("schluessel") => {
            let pfad = schluesselpfad();
            if pfad.exists() {
                fehler(&format!("{} existiert bereits. Nicht ueberschreiben, sonst werden alle bisherigen Lizenzen ungueltig.", pfad.display()));
            }
            let mut seed = [0u8; 32];
            getrandom::fill(&mut seed).expect("Zufallszahlen nicht verfuegbar");
            let key = SigningKey::from_bytes(&seed);
            std::fs::create_dir_all(pfad.parent().unwrap()).unwrap();
            let hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();
            std::fs::write(&pfad, hex).unwrap();
            println!("Privater Schluessel gespeichert: {}", pfad.display());
            println!("BITTE SICHERN (z. B. Passwortmanager) und niemals weitergeben.\n");
            println!("Oeffentlicher Schluessel fuer src-tauri/src/lizenz.rs (OEFFENTLICHER_SCHLUESSEL):\n{}", rust_array(&key));
        }
        Some("oeffentlich") => println!("{}", rust_array(&laden())),
        Some("ausstellen") => {
            let kunde = args.get(1).unwrap_or_else(|| fehler("Kundenname fehlt: lizenz-tool ausstellen \"Firma GmbH\"")).trim().to_string();
            if kunde.is_empty() {
                fehler("Kundenname darf nicht leer sein");
            }
            let mut bis: Option<String> = None;
            let mut out: Option<PathBuf> = None;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--bis" => {
                        let d = args.get(i + 1).unwrap_or_else(|| fehler("--bis braucht ein Datum JJJJ-MM-TT"));
                        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap_or_else(|_| fehler("Datum muss JJJJ-MM-TT sein"));
                        bis = Some(d.clone());
                        i += 2;
                    }
                    "--out" => {
                        out = Some(PathBuf::from(args.get(i + 1).unwrap_or_else(|| fehler("--out braucht einen Dateinamen"))));
                        i += 2;
                    }
                    other => fehler(&format!("Unbekannte Option {other}")),
                }
            }
            let payload = serde_json::json!({
                "kunde": kunde,
                "ausgestellt": chrono::Local::now().format("%Y-%m-%d").to_string(),
                "bis": bis,
            })
            .to_string();
            let sig = laden().sign(payload.as_bytes());
            let inhalt = format!("B2M1.{}.{}\n", B64.encode(payload.as_bytes()), B64.encode(sig.to_bytes()));
            let datei = out.unwrap_or_else(|| {
                let sauber: String = kunde.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
                PathBuf::from(format!("bill2mail_{sauber}.lic"))
            });
            std::fs::write(&datei, inhalt).unwrap();
            println!("Lizenz fuer \"{kunde}\"{} geschrieben: {}", bis.map(|b| format!(" (gueltig bis {b})")).unwrap_or_else(|| " (unbefristet)".into()), datei.display());
        }
        _ => {
            eprintln!("Verwendung:\n  lizenz-tool schluessel\n  lizenz-tool oeffentlich\n  lizenz-tool ausstellen \"Firma\" [--bis JJJJ-MM-TT] [--out datei.lic]");
            std::process::exit(2);
        }
    }
}
