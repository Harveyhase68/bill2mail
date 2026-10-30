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
