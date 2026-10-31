# Bill2Mail

Zerlegt Sammel-PDFs mit Rechnungen anhand der Kundennummer und versendet sie per E-Mail.

Proprietäre Software. Alle Rechte vorbehalten, © 2026 Alexander Predl. Keine Lizenz zur Nutzung, Weitergabe oder Veränderung.

## Entwicklung

Voraussetzungen: Rust, Node.js, Inno Setup 6 (nur für das Setup).

```bash
npm install
npm run tauri dev                      # Entwicklung
npm run tauri build -- --no-bundle     # Release-Exe: src-tauri/target/release/bill2mail.exe
cargo test --lib --manifest-path src-tauri/Cargo.toml
```

Setup erstellen: `setup/bill2mail.iss` mit Inno Setup kompilieren. Die Version kommt aus
`version` in `src-tauri/tauri.conf.json` (und `Cargo.toml`), Ergebnis: `setup/Output/`.

Beim Testen niemals echte Kunden-PDFs oder Zugangsdaten einchecken.

## Lizenz (für den Hersteller)

Ohne Lizenzdatei läuft die Demo (alles außer Versenden und Ordnerüberwachung). Lizenzen sind mit
Ed25519 signierte Dateien (`.lic`), die offline geprüft werden; im Programm steckt nur der öffentliche Schlüssel.

```bash
cd tools/lizenz-tool && cargo build --release
./target/release/lizenz-tool schluessel                  # einmalig: privater Schlüssel unter %USERPROFILE%\.bill2mail\
./target/release/lizenz-tool ausstellen "Firma GmbH"     # unbefristete Lizenz -> bill2mail_Firma_GmbH.lic
./target/release/lizenz-tool ausstellen "Firma GmbH" --bis 2027-12-31
```

Der Kunde lädt die Datei unter Einstellungen → Lizenz. Der private Schlüssel gehört **nie** ins Repository
und muss gesichert werden (Verlust = keine neuen Lizenzen). Wird er neu erzeugt, muss der öffentliche Schlüssel
in `src-tauri/src/lizenz.rs` ersetzt werden; alte Lizenzen werden dann ungültig.

## Sprache

Beim Start gilt die Windows-Sprache (Deutsch, sonst Englisch). Umschalten in den Einstellungen gilt nur bis zum
Beenden. Neue Texte: deutscher Text im Code, englische Übersetzung in `src/i18n.ts` (Frontend) bzw. `crate::i18n::tr` (Rust).

## Anmeldung per OAuth (Gmail, Outlook.com)

Zusätzlich zu Benutzer/Passwort kann sich Bill2Mail per OAuth 2.0 (Browser-Anmeldung, PKCE, SMTP XOAUTH2) bei
Google und Microsoft anmelden. Das Refresh-Token liegt im Windows-Anmeldeinformationsspeicher.
Ohne eingetragene Client-ID ist die jeweilige Schaltfläche in den Einstellungen deaktiviert.

Die Client-IDs werden beim Bauen über Umgebungsvariablen eingesetzt (nicht im Repository):

```bash
set B2M_GOOGLE_CLIENT_ID=...apps.googleusercontent.com
set B2M_GOOGLE_CLIENT_SECRET=...
set B2M_MS_CLIENT_ID=...
npm run tauri build -- --no-bundle
```

**Google** (console.cloud.google.com): Projekt anlegen → OAuth-Zustimmungsbildschirm → Anmeldedaten →
„OAuth-Client-ID“, Typ **Desktop-App**. Der Scope `https://mail.google.com/` ist „eingeschränkt“: Für beliebige
Kunden verlangt Google die App-Überprüfung (inkl. Sicherheitsprüfung). Im Status „Testing“ funktionieren nur eingetragene
Testnutzer (max. 100), und die Anmeldung läuft nach 7 Tagen ab. Für Gmail geht alternativ ein **App-Passwort**
(Konto mit 2-Faktor-Anmeldung) als normales Passwort.

**Microsoft** (entra.microsoft.com): App-Registrierung → Kontotypen „Konten in einem beliebigen Organisationsverzeichnis
und persönliche Microsoft-Konten“ → Plattform „Mobil- und Desktopanwendungen“ mit Umleitungs-URI
`http://localhost` → „Öffentlichen Clientflow zulassen“: Ja → API-Berechtigung (delegiert)
`SMTP.Send` (Office 365 Exchange Online). Server: `smtp-mail.outlook.com` (Outlook.com) bzw. `smtp.office365.com`
(Microsoft 365; dort muss SMTP AUTH für das Postfach aktiviert sein).
