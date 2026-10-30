use crate::settings::Einstellungen;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kunde {
    pub id: i64,
    pub kundennummer: String,
    pub kundenname: String,
    /// Mehrere Empfaenger werden mit ';' getrennt.
    pub email: String,
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS kunden (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                kundennummer TEXT NOT NULL UNIQUE,
                kundenname   TEXT NOT NULL DEFAULT '',
                email        TEXT NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS einstellungen (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS vorlagen (
                name TEXT PRIMARY KEY,
                json TEXT NOT NULL
            );

            -- Schutz vor Doppelversand: Inhalts-Hash jeder versendeten Sammel-PDF
            CREATE TABLE IF NOT EXISTS versandlog (
                hash      TEXT PRIMARY KEY,
                pfad      TEXT NOT NULL,
                zeitpunkt TEXT NOT NULL,
                mails     INTEGER NOT NULL
            );
            "#,
        )?;
        Ok(Database { conn })
    }

    pub fn alle_kunden(&self) -> rusqlite::Result<Vec<Kunde>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kundennummer, kundenname, email FROM kunden
             ORDER BY CAST(kundennummer AS INTEGER), kundennummer",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Kunde {
                id: r.get(0)?,
                kundennummer: r.get(1)?,
                kundenname: r.get(2)?,
                email: r.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// Legt den Kunden an (id == 0) oder aktualisiert ihn.
    pub fn kunde_speichern(&self, k: &Kunde) -> rusqlite::Result<i64> {
        if k.id == 0 {
            self.conn.execute(
                "INSERT INTO kunden (kundennummer, kundenname, email) VALUES (?1, ?2, ?3)",
                params![k.kundennummer.trim(), k.kundenname.trim(), k.email.trim()],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE kunden SET kundennummer = ?1, kundenname = ?2, email = ?3 WHERE id = ?4",
                params![k.kundennummer.trim(), k.kundenname.trim(), k.email.trim(), k.id],
            )?;
            Ok(k.id)
        }
    }

    pub fn kunde_loeschen(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM kunden WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// (Pfad, Zeitpunkt) des früheren Versands dieses PDF-Inhalts
    pub fn versand_gefunden(&self, hash: &str) -> rusqlite::Result<Option<(String, String)>> {
        self.conn
            .query_row("SELECT pfad, zeitpunkt FROM versandlog WHERE hash = ?1", params![hash], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
    }

    pub fn versand_vermerken(&self, hash: &str, pfad: &str, zeitpunkt: &str, mails: usize) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO versandlog (hash, pfad, zeitpunkt, mails) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(hash) DO UPDATE SET pfad = excluded.pfad, zeitpunkt = excluded.zeitpunkt, mails = excluded.mails",
            params![hash, pfad, zeitpunkt, mails as i64],
        )?;
        Ok(())
    }

    pub fn einstellungen_laden(&self) -> rusqlite::Result<Einstellungen> {
        let json: Option<String> = self
            .conn
            .query_row("SELECT value FROM einstellungen WHERE key = 'main'", [], |r| r.get(0))
            .optional()?;
        Ok(json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
    }

    pub fn einstellungen_speichern(&self, e: &Einstellungen) -> rusqlite::Result<()> {
        let json = serde_json::to_string(e).unwrap_or_default();
        self.conn.execute(
            "INSERT INTO einstellungen (key, value) VALUES ('main', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    pub fn vorlage_laden(&self, name: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row("SELECT json FROM vorlagen WHERE name = ?1", params![name], |r| r.get(0))
            .optional()
    }

    pub fn vorlage_speichern(&self, name: &str, json: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO vorlagen (name, json) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET json = excluded.json",
            params![name, json],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kunde(nr: &str) -> Kunde {
        Kunde { id: 0, kundennummer: nr.into(), kundenname: "Test".into(), email: "a@x.at;b@x.at".into() }
    }

    #[test]
    fn kunden_anlegen_sortieren_und_duplikat() {
        let dir = std::env::temp_dir().join(format!("b2m_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("t.db")).unwrap();
        db.kunde_speichern(&kunde("10")).unwrap();
        let id = db.kunde_speichern(&kunde("9")).unwrap();
        let alle = db.alle_kunden().unwrap();
        assert_eq!(alle[0].kundennummer, "9"); // numerisch sortiert, nicht "10" < "9"
        assert!(db.kunde_speichern(&kunde("10")).is_err()); // UNIQUE
        db.kunde_loeschen(id).unwrap();
        assert_eq!(db.alle_kunden().unwrap().len(), 1);
    }

    #[test]
    fn vorlage_roundtrip() {
        let dir = std::env::temp_dir().join(format!("b2m_test_v_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("t.db")).unwrap();
        assert!(db.vorlage_laden("Standard").unwrap().is_none());
        db.vorlage_speichern("Standard", "{\"a\":1}").unwrap();
        db.vorlage_speichern("Standard", "{\"a\":2}").unwrap();
        assert_eq!(db.vorlage_laden("Standard").unwrap().as_deref(), Some("{\"a\":2}"));
    }

    #[test]
    fn versandlog_erkennt_bereits_versendete_pdf() {
        let dir = std::env::temp_dir().join(format!("b2m_test_log_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("t.db")).unwrap();
        let h = crate::versand::pdf_hash(b"%PDF inhalt");
        assert_eq!(h.len(), 64);
        assert_ne!(h, crate::versand::pdf_hash(b"%PDF anderer inhalt"));
        assert!(db.versand_gefunden(&h).unwrap().is_none());
        db.versand_vermerken(&h, "C:/a/Sammel.pdf", "30.10.2026 10:15", 3).unwrap();
        db.versand_vermerken(&h, "C:/b/Sammel.pdf", "31.10.2026 08:00", 4).unwrap();
        assert_eq!(db.versand_gefunden(&h).unwrap(), Some(("C:/b/Sammel.pdf".into(), "31.10.2026 08:00".into())));
    }
}
