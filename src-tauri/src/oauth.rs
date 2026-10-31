//! OAuth 2.0 (Authorization Code + PKCE, Loopback-Weiterleitung) für Gmail und Outlook.com.
//!
//! Der Browser öffnet die Anmeldeseite des Anbieters, die Weiterleitung landet auf einem kurzzeitig
//! lauschenden Port auf 127.0.0.1. Das Refresh-Token liegt im Windows-Anmeldeinformationsspeicher;
//! beim Senden wird daraus ein frisches Access-Token geholt und per SMTP XOAUTH2 verwendet.
//!
//! Die Client-IDs sind keine Geheimnisse des Nutzers, sondern gehören zur Anwendung. Sie werden beim
//! Bauen über Umgebungsvariablen eingesetzt (siehe README), damit sie nicht im Repository stehen.

use crate::i18n::tr;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Anbieter {
    Google,
    Microsoft,
}

/// Endpunkte und Zugangsdaten eines Anbieters. Eigene Struktur, damit Tests einen Mock-Server nutzen können.
#[derive(Debug, Clone)]
pub struct Konfig {
    pub auth_url: String,
    pub token_url: String,
    pub scope: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    /// Hostname in der Weiterleitung. Microsoft verlangt "localhost", Google akzeptiert beides.
    pub redirect_host: &'static str,
    /// Zusätzliche Parameter der Anmeldeanfrage
    pub extra: Vec<(&'static str, &'static str)>,
}

impl Anbieter {
    pub fn konto(self) -> &'static str {
        match self {
            Anbieter::Google => "oauth-google",
            Anbieter::Microsoft => "oauth-microsoft",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Anbieter::Google => "Google",
            Anbieter::Microsoft => "Microsoft",
        }
    }

    /// `None`, wenn beim Bauen keine Client-ID eingetragen wurde.
    pub fn konfig(self) -> Option<Konfig> {
        match self {
            Anbieter::Google => {
                let id = option_env!("B2M_GOOGLE_CLIENT_ID").filter(|s| !s.is_empty())?;
                Some(Konfig {
                    auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
                    token_url: "https://oauth2.googleapis.com/token".into(),
                    scope: "https://mail.google.com/ openid email".into(),
                    client_id: id.into(),
                    client_secret: option_env!("B2M_GOOGLE_CLIENT_SECRET").filter(|s| !s.is_empty()).map(String::from),
                    redirect_host: "127.0.0.1",
                    extra: vec![("access_type", "offline"), ("prompt", "consent")],
                })
            }
            Anbieter::Microsoft => {
                let id = option_env!("B2M_MS_CLIENT_ID").filter(|s| !s.is_empty())?;
                Some(Konfig {
                    auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
                    token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
                    scope: "https://outlook.office.com/SMTP.Send offline_access openid email profile".into(),
                    client_id: id.into(),
                    client_secret: None,
                    redirect_host: "localhost",
                    extra: vec![("prompt", "select_account")],
                })
            }
        }
    }

    pub fn konfiguriert(self) -> bool {
        self.konfig().is_some()
    }
}

// ------------------------------------------------------------------ Hilfsfunktionen

fn zufall(n: usize) -> Result<Vec<u8>, String> {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).map_err(|e| e.to_string())?;
    Ok(b)
}

/// (code_verifier, code_challenge) nach RFC 7636 (S256)
pub fn pkce() -> Result<(String, String), String> {
    let verifier = B64.encode(zufall(48)?);
    let challenge = B64.encode(Sha256::digest(verifier.as_bytes()));
    Ok((verifier, challenge))
}

fn prozent_kodieren(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn prozent_dekodieren(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                if let Some(v) = hex {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub fn auth_adresse(k: &Konfig, redirect: &str, state: &str, challenge: &str, hinweis: Option<&str>) -> String {
    let mut p: Vec<(&str, String)> = vec![
        ("client_id", k.client_id.clone()),
        ("response_type", "code".into()),
        ("redirect_uri", redirect.into()),
        ("scope", k.scope.clone()),
        ("state", state.into()),
        ("code_challenge", challenge.into()),
        ("code_challenge_method", "S256".into()),
    ];
    for (a, b) in &k.extra {
        p.push((a, (*b).into()));
    }
    if let Some(h) = hinweis.filter(|h| !h.is_empty()) {
        p.push(("login_hint", h.into()));
    }
    let q: Vec<String> = p.iter().map(|(a, b)| format!("{a}={}", prozent_kodieren(b))).collect();
    format!("{}?{}", k.auth_url, q.join("&"))
}

/// Liest `code`/`state`/`error` aus der ersten Zeile einer HTTP-Anfrage ("GET /?code=… HTTP/1.1").
fn anfrage_parameter(anfrage: &str) -> Vec<(String, String)> {
    let pfad = anfrage.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("");
    let query = pfad.split_once('?').map(|(_, q)| q).unwrap_or("");
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (prozent_dekodieren(k), prozent_dekodieren(v)))
        .collect()
}

fn seite(titel: &str, text: &str) -> String {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Bill2Mail</title><body style=\"font:16px Segoe UI,sans-serif;text-align:center;margin-top:18vh;color:#1b2230\"><h2>{titel}</h2><p>{text}</p>"
    );
    format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

/// Wartet auf die Weiterleitung des Browsers und gibt den Autorisierungscode zurück.
fn code_abwarten(listener: &[TcpListener], state: &str, frist: Duration, abbruch: &AtomicBool) -> Result<String, String> {
    let ende = Instant::now() + frist;
    for l in listener {
        l.set_nonblocking(true).map_err(|e| e.to_string())?;
    }
    loop {
        if abbruch.load(Ordering::SeqCst) {
            return Err(tr("Anmeldung abgebrochen.", "Sign-in cancelled."));
        }
        if Instant::now() > ende {
            return Err(tr("Zeitüberschreitung: Die Anmeldung im Browser wurde nicht abgeschlossen.", "Timeout: the sign-in in the browser was not completed."));
        }
        for l in listener {
            let Ok((mut s, _)) = l.accept() else { continue };
            let _ = s.set_nonblocking(false);
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let anfrage = String::from_utf8_lossy(&buf[..n]).to_string();
            let p = anfrage_parameter(&anfrage);
            let hole = |k: &str| p.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
            if let Some(fehler) = hole("error") {
                let beschr = hole("error_description").unwrap_or_default();
                let _ = s.write_all(seite(&tr("Anmeldung nicht möglich", "Sign-in failed"), &beschr).as_bytes());
                return Err(format!("{fehler}: {beschr}"));
            }
            let Some(code) = hole("code") else {
                // z. B. /favicon.ico – ignorieren
                let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                continue;
            };
            if hole("state").as_deref() != Some(state) {
                let _ = s.write_all(seite(&tr("Ungültige Anfrage", "Invalid request"), "").as_bytes());
                continue;
            }
            let _ = s.write_all(
                seite(
                    &tr("Anmeldung erfolgreich", "Sign-in successful"),
                    &tr("Sie können dieses Fenster schließen und zu Bill2Mail zurückkehren.", "You can close this window and return to Bill2Mail."),
                )
                .as_bytes(),
            );
            return Ok(code);
        }
        std::thread::sleep(Duration::from_millis(80));
    }
}

#[derive(Debug, Deserialize)]
pub struct TokenAntwort {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
}

fn token_anfordern(k: &Konfig, felder: &[(&str, &str)]) -> Result<TokenAntwort, String> {
    let agent: ureq::Agent =
        ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(30))).build().into();
    let mut form: Vec<(&str, &str)> = felder.to_vec();
    form.push(("client_id", &k.client_id));
    if let Some(s) = &k.client_secret {
        form.push(("client_secret", s));
    }
    let mut antwort = agent
        .post(&k.token_url)
        .send_form(form)
        .map_err(|e| tr(&format!("Verbindung zum Anmeldeserver fehlgeschlagen: {e}"), &format!("Connection to the sign-in server failed: {e}")))?;
    let status = antwort.status().as_u16();
    let text = antwort.body_mut().read_to_string().unwrap_or_default();
    if status != 200 {
        let beschr = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                let d = v.get("error_description").or_else(|| v.get("error")).and_then(|x| x.as_str()).map(String::from);
                d
            })
            .unwrap_or(text);
        return Err(tr(&format!("Anmeldeserver lehnt ab ({status}): {beschr}"), &format!("Sign-in server rejected the request ({status}): {beschr}")));
    }
    serde_json::from_str(&text).map_err(|e| tr(&format!("Unerwartete Antwort des Anmeldeservers: {e}"), &format!("Unexpected response from the sign-in server: {e}")))
}

/// E-Mail-Adresse aus dem (nicht weiter geprüften, direkt vom Anbieter empfangenen) id_token.
fn konto_aus_id_token(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let json: serde_json::Value = serde_json::from_slice(&B64.decode(payload).ok()?).ok()?;
    ["email", "preferred_username"].iter().find_map(|k| json.get(k).and_then(|v| v.as_str()).map(String::from))
}

pub struct Anmeldung {
    pub refresh_token: String,
    pub email: Option<String>,
}

/// Kompletter Browser-Ablauf. `oeffnen` startet den Browser mit der Anmelde-Adresse.
pub fn anmelden_mit(k: &Konfig, hinweis: Option<&str>, oeffnen: &dyn Fn(&str) -> Result<(), String>, abbruch: &AtomicBool, frist: Duration) -> Result<Anmeldung, String> {
    let primaer = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = primaer.local_addr().map_err(|e| e.to_string())?.port();
    let mut lauscher = vec![primaer];
    // "localhost" kann der Browser als ::1 auflösen
    if let Ok(v6) = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port))) {
        lauscher.push(v6);
    }
    let redirect = format!("http://{}:{port}", k.redirect_host);
    let (verifier, challenge) = pkce()?;
    let state = B64.encode(zufall(16)?);
    oeffnen(&auth_adresse(k, &redirect, &state, &challenge, hinweis))?;
    let code = code_abwarten(&lauscher, &state, frist, abbruch)?;
    let t = token_anfordern(
        k,
        &[("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", &redirect), ("code_verifier", &verifier)],
    )?;
    let refresh_token = t.refresh_token.ok_or_else(|| {
        tr(
            "Der Anbieter hat kein dauerhaftes Anmelde-Token geliefert. Bitte die Anmeldung wiederholen und alle Berechtigungen erteilen.",
            "The provider did not return a long-lived sign-in token. Please sign in again and grant all permissions.",
        )
    })?;
    Ok(Anmeldung { refresh_token, email: t.id_token.as_deref().and_then(konto_aus_id_token) })
}

pub fn access_token_mit(k: &Konfig, refresh_token: &str) -> Result<String, String> {
    token_anfordern(k, &[("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("scope", &k.scope)]).map(|t| t.access_token)
}

// ------------------------------------------------------------------ Speicher & Hauptfunktionen

fn eintrag(a: Anbieter) -> Result<keyring::Entry, String> {
    keyring::Entry::new("Bill2Mail", a.konto()).map_err(|e| tr(&format!("Passwortspeicher nicht verfügbar: {e}"), &format!("Password store not available: {e}")))
}

pub fn angemeldet(a: Anbieter) -> bool {
    eintrag(a).ok().and_then(|e| e.get_password().ok()).is_some()
}

pub fn abmelden(a: Anbieter) {
    if let Ok(e) = eintrag(a) {
        let _ = e.delete_credential();
    }
}

fn nicht_konfiguriert(a: Anbieter) -> String {
    tr(
        &format!("Die Anmeldung mit {} ist in dieser Programmversion nicht eingerichtet. Bitte stattdessen Server und Passwort (bzw. ein App-Passwort) verwenden.", a.name()),
        &format!("Sign-in with {} is not set up in this version of the program. Please use server and password (or an app password) instead.", a.name()),
    )
}

/// Meldet im Browser an und speichert das Refresh-Token. Gibt die E-Mail-Adresse des Kontos zurück (falls bekannt).
pub fn anmelden(a: Anbieter, hinweis: Option<&str>, oeffnen: &dyn Fn(&str) -> Result<(), String>, abbruch: &AtomicBool) -> Result<Option<String>, String> {
    let k = a.konfig().ok_or_else(|| nicht_konfiguriert(a))?;
    let erg = anmelden_mit(&k, hinweis, oeffnen, abbruch, Duration::from_secs(300))?;
    eintrag(a)?
        .set_password(&erg.refresh_token)
        .map_err(|e| tr(&format!("Anmeldung konnte nicht gespeichert werden: {e}"), &format!("Sign-in could not be saved: {e}")))?;
    Ok(erg.email)
}

/// Frisches Access-Token für SMTP XOAUTH2.
pub fn access_token(a: Anbieter) -> Result<String, String> {
    let k = a.konfig().ok_or_else(|| nicht_konfiguriert(a))?;
    let refresh = eintrag(a)?.get_password().map_err(|_| {
        tr(
            &format!("Nicht bei {} angemeldet. Bitte in den Einstellungen anmelden.", a.name()),
            &format!("Not signed in to {}. Please sign in under Settings.", a.name()),
        )
    })?;
    access_token_mit(&k, &refresh).map_err(|e| {
        tr(
            &format!("{e}\nMöglicherweise ist die Anmeldung abgelaufen – bitte in den Einstellungen erneut anmelden."),
            &format!("{e}\nThe sign-in may have expired – please sign in again under Settings."),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn pkce_challenge_passt_zum_verifier() {
        let (v, c) = pkce().unwrap();
        assert_eq!(c, B64.encode(Sha256::digest(v.as_bytes())));
        assert!(v.len() >= 43 && v.len() <= 128);
        assert_ne!(pkce().unwrap().0, v);
        // Beispiel aus RFC 7636, Anhang B
        let rfc = B64.encode(Sha256::digest(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"));
        assert_eq!(rfc, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn prozentkodierung_roundtrip() {
        let s = "a b/c?d=é&f";
        assert_eq!(prozent_dekodieren(&prozent_kodieren(s)), s);
        assert_eq!(prozent_dekodieren("4%2F0Aeanx%2Bq"), "4/0Aeanx+q");
        assert_eq!(prozent_dekodieren("100%"), "100%");
    }

    fn testkonfig(token_url: &str) -> Konfig {
        Konfig {
            auth_url: "https://auth.example/authorize".into(),
            token_url: token_url.into(),
            scope: "mail offline_access".into(),
            client_id: "client-123".into(),
            client_secret: Some("geheim".into()),
            redirect_host: "127.0.0.1",
            extra: vec![("prompt", "consent")],
        }
    }

    #[test]
    fn auth_adresse_enthaelt_alle_parameter() {
        let u = auth_adresse(&testkonfig("x"), "http://127.0.0.1:5555", "st", "ch", Some("a@b.at"));
        for erwartet in [
            "client_id=client-123",
            "response_type=code",
            "redirect_uri=http%3A%2F%2F127.0.0.1%3A5555",
            "scope=mail%20offline_access",
            "state=st",
            "code_challenge=ch",
            "code_challenge_method=S256",
            "prompt=consent",
            "login_hint=a%40b.at",
        ] {
            assert!(u.contains(erwartet), "{erwartet} fehlt in {u}");
        }
    }

    /// Mini-HTTP-Server: beantwortet Token-Anfragen und merkt sich deren Körper.
    fn tokenserver(antworten: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/token", l.local_addr().unwrap());
        let anfragen = Arc::new(Mutex::new(Vec::new()));
        let a2 = anfragen.clone();
        std::thread::spawn(move || {
            for (status, body) in antworten {
                let (mut s, _) = l.accept().unwrap();
                let mut buf = vec![0u8; 8192];
                let mut n = 0;
                loop {
                    let m = s.read(&mut buf[n..]).unwrap();
                    n += m;
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    if let Some((kopf, rest)) = text.split_once("\r\n\r\n") {
                        let laenge = kopf.lines().find_map(|z| z.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
                        if rest.len() >= laenge {
                            break;
                        }
                    }
                    if m == 0 {
                        break;
                    }
                }
                a2.lock().unwrap().push(String::from_utf8_lossy(&buf[..n]).to_string());
                let antwort = format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                s.write_all(antwort.as_bytes()).unwrap();
            }
        });
        (url, anfragen)
    }

    fn jwt(payload: &str) -> String {
        format!("x.{}.y", B64.encode(payload))
    }

    /// Simuliert den Browser: ruft die Weiterleitungsadresse mit Code und State auf.
    fn browser(url: &str, state_ueberschreiben: Option<&str>, fehler: bool) -> Result<(), String> {
        let redirect = prozent_dekodieren(url.split("redirect_uri=").nth(1).unwrap().split('&').next().unwrap());
        let state = url.split("state=").nth(1).unwrap().split('&').next().unwrap().to_string();
        let state = state_ueberschreiben.map(String::from).unwrap_or(state);
        let adresse = redirect.trim_start_matches("http://").to_string();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            let mut s = std::net::TcpStream::connect(&adresse).unwrap();
            let pfad = if fehler { "/?error=access_denied&error_description=abgelehnt".to_string() } else { format!("/?code=der%2Fcode&state={state}") };
            s.write_all(format!("GET {pfad} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).unwrap();
            let mut antwort = String::new();
            let _ = s.read_to_string(&mut antwort);
        });
        Ok(())
    }

    #[test]
    fn kompletter_ablauf_gegen_mockserver() {
        let id_token = jwt(r#"{"email":"vater@gmail.com"}"#);
        let (url, anfragen) = tokenserver(vec![
            (200, format!(r#"{{"access_token":"AT1","refresh_token":"RT1","id_token":"{id_token}"}}"#)),
            (200, r#"{"access_token":"AT2"}"#.into()),
        ]);
        let k = testkonfig(&url);
        let abbruch = AtomicBool::new(false);
        let erg = anmelden_mit(&k, None, &|u| browser(u, None, false), &abbruch, Duration::from_secs(10)).unwrap();
        assert_eq!(erg.refresh_token, "RT1");
        assert_eq!(erg.email.as_deref(), Some("vater@gmail.com"));
        assert_eq!(access_token_mit(&k, "RT1").unwrap(), "AT2");

        let a = anfragen.lock().unwrap();
        assert!(a[0].contains("grant_type=authorization_code"), "{}", a[0]);
        assert!(a[0].contains("code=der%2Fcode"), "{}", a[0]);
        assert!(a[0].contains("code_verifier="));
        assert!(a[0].contains("client_secret=geheim"));
        assert!(a[1].contains("grant_type=refresh_token") && a[1].contains("refresh_token=RT1"), "{}", a[1]);
    }

    #[test]
    fn falscher_state_wird_ignoriert_und_fehler_gemeldet() {
        let (url, _) = tokenserver(vec![]);
        let k = testkonfig(&url);
        let abbruch = AtomicBool::new(false);
        // falscher State: es kommt nie ein gültiger Code -> Zeitüberschreitung
        let e = anmelden_mit(&k, None, &|u| browser(u, Some("falsch"), false), &abbruch, Duration::from_millis(900)).err().unwrap();
        assert!(e.contains("Zeitüberschreitung"), "{e}");
        // Ablehnung durch den Nutzer
        let e = anmelden_mit(&k, None, &|u| browser(u, None, true), &abbruch, Duration::from_secs(5)).err().unwrap();
        assert!(e.contains("access_denied") && e.contains("abgelehnt"), "{e}");
    }

    #[test]
    fn abbruch_beendet_das_warten() {
        let (url, _) = tokenserver(vec![]);
        let abbruch = AtomicBool::new(true);
        let e = anmelden_mit(&testkonfig(&url), None, &|_| Ok(()), &abbruch, Duration::from_secs(10)).err().unwrap();
        assert!(e.contains("abgebrochen"), "{e}");
    }

    #[test]
    fn serverfehler_wird_verstaendlich_gemeldet() {
        let (url, _) = tokenserver(vec![(400, r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#.into())]);
        let e = access_token_mit(&testkonfig(&url), "alt").unwrap_err();
        assert!(e.contains("400") && e.contains("expired or revoked"), "{e}");
    }

    #[test]
    fn ohne_refresh_token_gibt_es_eine_klare_meldung() {
        let (url, _) = tokenserver(vec![(200, r#"{"access_token":"AT"}"#.into())]);
        let abbruch = AtomicBool::new(false);
        let e = anmelden_mit(&testkonfig(&url), None, &|u| browser(u, None, false), &abbruch, Duration::from_secs(10)).err().unwrap();
        assert!(e.contains("dauerhaftes"), "{e}");
    }

    #[test]
    fn konto_aus_token() {
        assert_eq!(konto_aus_id_token(&jwt(r#"{"preferred_username":"x@outlook.com"}"#)).as_deref(), Some("x@outlook.com"));
        assert_eq!(konto_aus_id_token("kaputt"), None);
    }
}
