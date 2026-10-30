use crate::settings::{Einstellungen, Sicherheit};
use lettre::message::{header::ContentType, Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{Address, Message, SmtpTransport, Transport};
use std::path::PathBuf;
use std::time::Duration;

/// Mehrere Adressen werden mit ';' getrennt (ein Kunde will die Rechnung an zwei Postfächer).
pub fn empfaenger_pruefen(adressen: &str) -> Result<Vec<Mailbox>, String> {
    let liste: Result<Vec<Mailbox>, String> = adressen
        .split(';')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(|a| a.parse().map_err(|_| format!("Ungültige E-Mail-Adresse: {a}")))
        .collect();
    match liste {
        Ok(l) if l.is_empty() => Err("Keine E-Mail-Adresse hinterlegt".into()),
        other => other,
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// `{KUNDENNAME}` im HTML-Text; der Name wird maskiert (z. B. "Müller & Söhne").
pub fn text_mit_name(text: &str, name: &str) -> String {
    text.replace("{KUNDENNAME}", &html_escape(name))
}

pub fn betreff_mit_name(betreff: &str, name: &str) -> String {
    betreff.replace("{KUNDENNAME}", name)
}

fn html_zu_text(html: &str) -> String {
    let mut t = html.to_string();
    for tag in ["<br>", "<br/>", "<br />", "</p>", "</div>", "</li>"] {
        t = t.replace(tag, "\n");
    }
    let mut out = String::new();
    let mut in_tag = false;
    for c in t.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", " ");
    let mut out = out;
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out.trim().to_string()
}

#[derive(Debug)]
pub struct Mailer {
    transport: SmtpTransport,
    von: Mailbox,
    betreff: String,
    text: String,
}

impl Mailer {
    pub fn neu(e: &Einstellungen, passwort: Option<&str>) -> Result<Self, String> {
        if e.smtp_host.trim().is_empty() {
            return Err("Kein SMTP-Server eingetragen (Einstellungen).".into());
        }
        let adresse: Address = e
            .absender
            .trim()
            .parse()
            .map_err(|_| "Die Absender-Adresse in den Einstellungen ist ungültig.".to_string())?;
        let name = (!e.absender_name.trim().is_empty()).then(|| e.absender_name.trim().to_string());

        let host = e.smtp_host.trim();
        let verbindungsfehler = |err: lettre::transport::smtp::Error| format!("SMTP-Verbindungsfehler: {err}");
        let mut builder = match e.sicherheit {
            Sicherheit::Ssl => SmtpTransport::relay(host).map_err(verbindungsfehler)?,
            Sicherheit::Starttls => SmtpTransport::starttls_relay(host).map_err(verbindungsfehler)?,
            Sicherheit::Keine => SmtpTransport::builder_dangerous(host),
        }
        .port(e.smtp_port)
        .timeout(Some(Duration::from_secs(30)));
        if !e.smtp_user.trim().is_empty() {
            builder = builder
                .credentials(Credentials::new(e.smtp_user.trim().to_string(), passwort.unwrap_or_default().to_string()))
                .authentication(vec![Mechanism::Plain, Mechanism::Login]);
        }
        let transport = builder.build();
        match transport.test_connection() {
            Ok(true) => {}
            Ok(false) => return Err("SMTP-Server nicht erreichbar.".into()),
            Err(err) => return Err(format!("SMTP-Verbindung fehlgeschlagen: {err}")),
        }
        Ok(Mailer { transport, von: Mailbox::new(name, adresse), betreff: e.betreff.clone(), text: e.text.clone() })
    }

    fn nachricht(&self, an: Vec<Mailbox>, name: &str, anhaenge: &[PathBuf], testmail: bool) -> Result<Message, String> {
        let html = text_mit_name(&self.text, name);
        let alternativ = MultiPart::alternative()
            .singlepart(SinglePart::builder().header(ContentType::TEXT_PLAIN).body(html_zu_text(&html)))
            .singlepart(SinglePart::builder().header(ContentType::TEXT_HTML).body(html));
        let mut teile = MultiPart::mixed().multipart(alternativ);
        for pfad in anhaenge {
            let inhalt = std::fs::read(pfad).map_err(|e| format!("PDF konnte nicht gelesen werden ({}): {e}", pfad.display()))?;
            let dateiname = pfad.file_name().unwrap_or_default().to_string_lossy().to_string();
            teile = teile.singlepart(Attachment::new(dateiname).body(inhalt, ContentType::parse("application/pdf").unwrap()));
        }
        let mut b = Message::builder().from(self.von.clone()).reply_to(self.von.clone());
        for m in an {
            b = b.to(m);
        }
        let betreff = betreff_mit_name(&self.betreff, name);
        let betreff = if testmail { format!("TEST: {betreff}") } else { betreff };
        b.subject(betreff).multipart(teile).map_err(|e| format!("E-Mail konnte nicht erstellt werden: {e}"))
    }

    /// Eine E-Mail mit allen PDFs eines Kunden.
    pub fn senden(&self, an: &str, name: &str, anhaenge: &[PathBuf]) -> Result<(), String> {
        let msg = self.nachricht(empfaenger_pruefen(an)?, name, anhaenge, false)?;
        self.transport.send(&msg).map(|_| ()).map_err(|e| format!("Senden an {an} fehlgeschlagen: {e}"))
    }

    /// Test-Mail mit dem echten Betreff/Text und Platzhalter-Name, ohne Anhang.
    pub fn test(&self, an: &str) -> Result<(), String> {
        let msg = self.nachricht(empfaenger_pruefen(an)?, "Max Mustermann", &[], true)?;
        self.transport.send(&msg).map(|_| ()).map_err(|e| format!("Test-Mail an {an} fehlgeschlagen: {e}"))
    }
}

#[cfg(test)]
pub mod testserver {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Minimaler SMTP-Server auf 127.0.0.1, sammelt die empfangenen Nachrichten (DATA).
    pub fn start() -> (u16, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mails = Arc::new(Mutex::new(Vec::new()));
        let m = mails.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let m = m.clone();
                std::thread::spawn(move || {
                    let mut w = stream.try_clone().unwrap();
                    let mut r = BufReader::new(stream);
                    let _ = w.write_all(b"220 test ESMTP\r\n");
                    let mut line = String::new();
                    loop {
                        line.clear();
                        if r.read_line(&mut line).unwrap_or(0) == 0 {
                            break;
                        }
                        let cmd = line.trim().to_uppercase();
                        if cmd.starts_with("EHLO") || cmd.starts_with("HELO") {
                            let _ = w.write_all(b"250 test\r\n");
                        } else if cmd.starts_with("DATA") {
                            let _ = w.write_all(b"354 go\r\n");
                            let mut data = String::new();
                            loop {
                                line.clear();
                                if r.read_line(&mut line).unwrap_or(0) == 0 || line == ".\r\n" {
                                    break;
                                }
                                data.push_str(&line);
                            }
                            m.lock().unwrap().push(data);
                            let _ = w.write_all(b"250 queued\r\n");
                        } else if cmd.starts_with("QUIT") {
                            let _ = w.write_all(b"221 bye\r\n");
                            break;
                        } else {
                            let _ = w.write_all(b"250 OK\r\n");
                        }
                    }
                });
            }
        });
        (port, mails)
    }

    pub fn einstellungen(port: u16) -> crate::settings::Einstellungen {
        crate::settings::Einstellungen {
            smtp_host: "127.0.0.1".into(),
            smtp_port: port,
            sicherheit: crate::settings::Sicherheit::Keine,
            absender: "buchhaltung@firma.at".into(),
            absender_name: "Buchhaltung".into(),
            betreff: "Rechnung fuer {KUNDENNAME}".into(),
            text: "<p>Hallo <b>{KUNDENNAME}</b></p>".into(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empfaenger_mit_semikolon() {
        assert_eq!(empfaenger_pruefen("a@x.at; b@y.at;").unwrap().len(), 2);
        assert!(empfaenger_pruefen("a@x.at;kaputt").is_err());
        assert!(empfaenger_pruefen("  ").is_err());
    }

    #[test]
    fn platzhalter_maskieren_html() {
        assert_eq!(text_mit_name("<p>{KUNDENNAME}</p>", "Müller & <Söhne>"), "<p>Müller &amp; &lt;Söhne&gt;</p>");
        assert_eq!(betreff_mit_name("Re: {KUNDENNAME}", "A & B"), "Re: A & B");
        assert_eq!(html_zu_text("<p>Hallo <b>A &amp; B</b></p><p>Tschüss</p>"), "Hallo A & B\nTschüss");
    }

    #[test]
    fn senden_an_lokalen_server() {
        let (port, mails) = testserver::start();
        let e = testserver::einstellungen(port);
        let dir = std::env::temp_dir().join(format!("b2m_mail_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("rechnung_kunde_1.pdf");
        std::fs::write(&pdf, b"%PDF-1.4 test").unwrap();

        let m = Mailer::neu(&e, None).unwrap();
        m.senden("a@kunde.at; b@kunde.at", "Muster & Co", &[pdf]).unwrap();
        m.test("ich@firma.at").unwrap();

        let alle = mails.lock().unwrap();
        assert_eq!(alle.len(), 2);
        let echt = &alle[0];
        assert!(echt.contains("a@kunde.at") && echt.contains("b@kunde.at"), "{echt}");
        assert!(echt.contains("Subject: Rechnung fuer Muster & Co"), "{echt}");
        assert!(echt.contains("rechnung_kunde_1.pdf") && echt.contains("application/pdf"));
        assert!(echt.contains("Muster &amp; Co"), "{echt}");
        assert!(alle[1].contains("Subject: TEST: Rechnung fuer Max Mustermann"), "{}", alle[1]);
        assert!(!alle[1].contains("application/pdf"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn fehler_bei_nicht_erreichbarem_server_und_fehlenden_angaben() {
        let mut e = testserver::einstellungen(1); // Port 1: nichts da
        assert!(Mailer::neu(&e, None).is_err());
        e.smtp_host.clear();
        assert!(Mailer::neu(&e, None).unwrap_err().contains("SMTP-Server"));
        let mut e = testserver::einstellungen(25);
        e.absender = "kein-mail".into();
        assert!(Mailer::neu(&e, None).unwrap_err().contains("Absender"));
    }
}
