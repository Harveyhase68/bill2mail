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
