//! Wendet die Rahmen-Vorlage auf ein PDF an und zerlegt es in Rechnungen.
//!
//! Parametersyntax eines Rahmens: Alternativen mit ";" getrennt, die erste passende gilt.
//!   `RECHNUNG`           Text muss im Rahmen vorkommen (Gross-/Kleinschreibung und
//!                        Leerzeichen sind egal)
//!   `Kundennummer*[KU]`  Text nach "Kundennummer" wird als Kundennummer gelesen
//!   `*[RE]`              der gesamte Rahmeninhalt ist die Rechnungsnummer
//!   `Seite*[SE] von`     optionaler Text nach `]` begrenzt den Wert
//! Variablen: KU Kundennummer, RE Rechnungsnummer, SE Seitennummer, NA Kundenname.

use crate::db::Kunde;
use crate::pdf_text::{extract_pages, region_text, PageText};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Standard-Toleranz für verschobene Anker (±30 pt); einstellbar. Gesucht wird in 3-pt-Schritten.
pub const TOLERANZ_STANDARD_PT: f64 = 30.0;
pub const TOLERANZ_MAX_PT: f64 = 200.0;
const ANKER_SCHRITT_PT: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameType {
    Rot,
    Blau,
    Gruen,
    Violett,
    Grau,
}

impl FrameType {
    fn name(self) -> &'static str {
        match self {
            FrameType::Rot => crate::i18n::tr_static("Formulartyp (rot)", "Form type (red)"),
            FrameType::Blau => crate::i18n::tr_static("Kundennummer (blau)", "Customer number (blue)"),
            FrameType::Gruen => crate::i18n::tr_static("Rechnungsnummer (grün)", "Invoice number (green)"),
            FrameType::Violett => crate::i18n::tr_static("Seitennummer (violett)", "Page number (violet)"),
            FrameType::Grau => crate::i18n::tr_static("Kundenname (grau)", "Customer name (gray)"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameDef {
    #[serde(rename = "type")]
    pub kind: FrameType,
    pub page: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub params: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Var {
    Ku,
    Re,
    Se,
    Na,
}

#[derive(Debug, Clone)]
struct Muster {
    label: String,
    var: Option<Var>,
    suffix: String,
}

fn parse_muster(params: &str) -> Result<Vec<Muster>, String> {
    let mut out = Vec::new();
    for item in params.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let Some(star) = item.find("*[") else {
            out.push(Muster { label: item.to_string(), var: None, suffix: String::new() });
            continue;
        };
        let rest = &item[star + 2..];
        let Some(close) = rest.find(']') else {
            return Err(crate::i18n::tr(&format!("\"{item}\": schließende Klammer ] fehlt"), &format!("\"{item}\": closing bracket ] missing")));
        };
        let var = match rest[..close].trim().to_uppercase().as_str() {
            "KU" => Var::Ku,
            "RE" => Var::Re,
            "SE" => Var::Se,
            "NA" => Var::Na,
            other => return Err(format!("\"{item}\": unbekannte Variable [{other}] (erlaubt: KU, RE, SE, NA)")),
        };
        out.push(Muster {
            label: item[..star].trim().to_string(),
            var: Some(var),
            suffix: rest[close + 1..].trim().to_string(),
        });
    }
    if out.is_empty() {
        return Err("keine Parameter angegeben".into());
    }
    Ok(out)
}

#[derive(Debug, Default)]
struct Treffer {
    label: String,
    werte: Vec<(Var, String)>,
}

/// Kleinbuchstaben ohne Leerzeichen, mit Byte-Position jedes Zeichens im Original.
fn normalisiert(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::new();
    let mut pos = Vec::new();
    for (i, c) in text.char_indices() {
        if !c.is_whitespace() {
            chars.push(c.to_lowercase().next().unwrap_or(c));
            pos.push(i);
        }
    }
    (chars, pos)
}

fn find_sub(hay: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Text hinter den ersten `k` nicht-leeren Zeichen.
fn nach_zeichen(text: &str, k: usize) -> &str {
    let mut seen = 0;
    for (i, c) in text.char_indices() {
        if seen == k {
            return &text[i..];
        }
        if !c.is_whitespace() {
            seen += 1;
        }
    }
    ""
}

fn bereinigen(rest: &str) -> &str {
    rest.trim_start_matches(|c: char| c.is_whitespace() || ":.#-–=".contains(c))
}

fn bis_suffix<'a>(rest: &'a str, suffix: &str) -> &'a str {
    if suffix.is_empty() {
        return rest;
    }
    let (r, s) = (rest.to_lowercase(), suffix.to_lowercase());
    match r.find(&s).and_then(|i| rest.get(..i)) {
        Some(cut) => cut,
        None => rest,
    }
}

/// `eng`: Text mit grosszuegiger Wortlücke (zusammenhängende Kennungen), `weit`: normale
/// Wortlücke (für Namen). Beide enthalten dieselben Zeichen in derselben Reihenfolge.
fn auswerten(muster: &[Muster], eng: &str, weit: &str) -> Option<Treffer> {
    let (norm, _) = normalisiert(eng);
    for m in muster {
        let label: Vec<char> = m.label.chars().filter(|c| !c.is_whitespace()).flat_map(|c| c.to_lowercase()).collect();
        let Some(start) = find_sub(&norm, &label) else { continue };
        let k = start + label.len();
        let Some(var) = m.var else {
            return Some(Treffer { label: m.label.clone(), werte: vec![] });
        };
        let wert = match var {
            Var::Na => {
                let rest = bis_suffix(bereinigen(nach_zeichen(weit, k)), &m.suffix);
                rest.split_whitespace().collect::<Vec<_>>().join(" ")
            }
            _ => {
                let rest = bis_suffix(bereinigen(nach_zeichen(eng, k)), &m.suffix);
                let token = rest.split_whitespace().next().unwrap_or("");
                let token = token.trim_end_matches([',', ';', ':', '.']);
                if var == Var::Se {
                    token.chars().take_while(char::is_ascii_digit).collect()
                } else {
                    token.to_string()
                }
            }
        };
        if wert.is_empty() {
            continue;
        }
        return Some(Treffer { label: m.label.clone(), werte: vec![(var, wert)] });
    }
    None
}

// ------------------------------------------------------------------ Vorlage

struct Vorlage {
    frames: Vec<(FrameDef, Vec<Muster>)>,
    /// Maximale vertikale Abweichung der Rahmen in pt
    toleranz: f64,
}

impl Vorlage {
    fn hat(&self, kind: FrameType) -> bool {
        self.frames.iter().any(|(f, _)| f.kind == kind)
    }
}

fn vorlage_bauen(frames: &[FrameDef]) -> Result<(Vorlage, Vec<String>), String> {
    let mut v = Vorlage { frames: Vec::new(), toleranz: TOLERANZ_STANDARD_PT };
    let mut warnungen = Vec::new();
    for f in frames {
        if f.params.trim().is_empty() {
            warnungen.push(format!("{}: Rahmen ohne Parameter wird ignoriert.", f.kind.name()));
            continue;
        }
        let m = parse_muster(&f.params).map_err(|e| format!("{}: {e}", f.kind.name()))?;
        v.frames.push((f.clone(), m));
    }
    let blau = v.frames.iter().find(|(f, _)| f.kind == FrameType::Blau);
    match blau {
        None => return Err(crate::i18n::tr("Der blaue Kundennummer-Rahmen fehlt oder hat keine Parameter.", "The blue customer number frame is missing or has no parameters.")),
        Some((_, m)) if !m.iter().any(|m| m.var == Some(Var::Ku)) => {
            return Err(crate::i18n::tr("Blauer Rahmen: Der Parameter muss *[KU] enthalten, z. B. Kundennummer*[KU].", "Blue frame: the parameter must contain *[KU], e.g. Customer number*[KU]."));
        }
        _ => {}
    }
    Ok((v, warnungen))
}

// ------------------------------------------------------------------ Ergebnis

#[derive(Debug, Clone, Default, Serialize)]
pub struct SeitenProbe {
    /// 1-basiert
    pub seite: usize,
    pub texte: BTreeMap<FrameType, String>,
    pub treffer: BTreeMap<FrameType, bool>,
    /// Vertikale Verschiebung in Punkten, falls der Text nicht im Rahmen selbst gefunden wurde
    pub versatz: BTreeMap<FrameType, f64>,
    pub formulartyp: Option<String>,
    pub kundennummer: Option<String>,
    pub rechnungsnummer: Option<String>,
    pub seitennummer: Option<u32>,
    pub kundenname: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Rechnung {
    pub nr: usize,
    pub kundennummer: String,
    pub rechnungsnummer: String,
    pub formulartyp: String,
    pub kundenname_pdf: String,
    /// 0-basiert, einschließlich
    pub start_page: usize,
    pub end_page: usize,
    pub warnungen: Vec<String>,
    pub in_liste: bool,
    pub kundenname: String,
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct Analyse {
    pub seiten: usize,
    pub rechnungen: Vec<Rechnung>,
    pub proben: Vec<SeitenProbe>,
    pub warnungen: Vec<String>,
}

fn seite_auswerten(page: &PageText, v: &Vorlage, nr: usize) -> SeitenProbe {
    let mut p = SeitenProbe { seite: nr, ..Default::default() };
    let mut werte: BTreeMap<u8, String> = BTreeMap::new();
    let key = |v: Var| v as u8;

    for (f, muster) in &v.frames {
        let lesen = |dy: f64| {
            let eng = region_text(page, f.x, f.y + dy, f.w, f.h, 0.6);
            let weit = region_text(page, f.x, f.y + dy, f.w, f.h, 0.3);
            (auswerten(muster, &eng, &weit), weit)
        };
        let (mut t, mut weit) = lesen(0.0);
        // Verschiebt sich der Kopf (z. B. weil die Adresse eine Zeile länger ist), wird der
        // Rahmen als Anker behandelt und der Text in der Nähe gesucht. Das geht nur, wenn
        // der Parameter einen Text enthält, nach dem man suchen kann.
        if t.is_none() && muster.iter().any(|m| !m.label.is_empty()) {
            let schritte = (v.toleranz.clamp(0.0, TOLERANZ_MAX_PT) / ANKER_SCHRITT_PT).ceil() as usize;
            'suche: for schritt in 1..=schritte {
                for vorzeichen in [1.0, -1.0] {
                    let dy = vorzeichen * (schritt as f64 * ANKER_SCHRITT_PT).min(v.toleranz);
                    let (gefunden, text) = lesen(dy);
                    if gefunden.is_some() {
                        (t, weit) = (gefunden, text);
                        p.versatz.insert(f.kind, dy);
                        break 'suche;
                    }
                }
            }
        }
        p.texte.insert(f.kind, weit);
        p.treffer.insert(f.kind, t.is_some());
        if let Some(t) = t {
            if f.kind == FrameType::Rot {
                p.formulartyp = Some(t.label);
            }
            for (var, val) in t.werte {
                werte.entry(key(var)).or_insert(val);
            }
        }
    }
    p.kundennummer = werte.get(&key(Var::Ku)).cloned();
    p.rechnungsnummer = werte.get(&key(Var::Re)).cloned();
    p.seitennummer = werte.get(&key(Var::Se)).and_then(|s| s.parse().ok());
    p.kundenname = werte.get(&key(Var::Na)).cloned();
    p
}

fn gruppieren(proben: &[SeitenProbe], v: &Vorlage, warnungen: &mut Vec<String>) -> Vec<Rechnung> {
    struct Gruppe {
        r: Rechnung,
        fremd: bool,
    }
    let hat_rot = v.hat(FrameType::Rot);
    let hat_seite = v.hat(FrameType::Violett);
    let mut gruppen: Vec<Gruppe> = Vec::new();

    for (idx, p) in proben.iter().enumerate() {
        let aktuell = gruppen.last();
        let ist_start = match p.seitennummer {
            Some(n) => n == 1,
            None if hat_rot => {
                let wiederholt_kopf = !hat_seite
                    && aktuell.is_some_and(|g| {
                        p.kundennummer.is_some()
                            && p.rechnungsnummer.is_some()
                            && p.kundennummer.as_deref() == Some(g.r.kundennummer.as_str())
                            && p.rechnungsnummer.as_deref() == Some(g.r.rechnungsnummer.as_str())
                    });
                p.treffer.get(&FrameType::Rot) == Some(&true) && !wiederholt_kopf
            }
            None => {
                p.kundennummer.is_some()
                    && p.kundennummer.as_deref() != aktuell.map(|g| g.r.kundennummer.as_str())
            }
        };

        if ist_start {
            let fremd = hat_rot && p.treffer.get(&FrameType::Rot) != Some(&true);
            let mut r = Rechnung {
                kundennummer: p.kundennummer.clone().unwrap_or_default(),
                rechnungsnummer: p.rechnungsnummer.clone().unwrap_or_default(),
                formulartyp: p.formulartyp.clone().unwrap_or_default(),
                kundenname_pdf: p.kundenname.clone().unwrap_or_default(),
                start_page: idx,
                end_page: idx,
                ..Default::default()
            };
            if !fremd {
                if r.kundennummer.is_empty() {
                    r.warnungen.push(crate::i18n::tr("Kundennummer nicht gefunden", "Customer number not found"));
                }
                let re_erwartet = v.hat(FrameType::Gruen)
                    || v.frames.iter().any(|(_, m)| m.iter().any(|m| m.var == Some(Var::Re)));
                if re_erwartet && r.rechnungsnummer.is_empty() {
                    r.warnungen.push(crate::i18n::tr("Rechnungsnummer nicht gefunden", "Invoice number not found"));
                }
            }
            gruppen.push(Gruppe { r, fremd });
        } else if let Some(g) = gruppen.last_mut() {
            g.r.end_page = idx;
            if let Some(k) = &p.kundennummer {
                if !g.fremd && !g.r.kundennummer.is_empty() && *k != g.r.kundennummer {
                    g.r.warnungen.push(crate::i18n::tr(
                        &format!("Seite {}: abweichende Kundennummer {k} (erwartet {})", idx + 1, g.r.kundennummer),
                        &format!("Page {}: different customer number {k} (expected {})", idx + 1, g.r.kundennummer),
                    ));
                }
            }
        } else {
            warnungen.push(crate::i18n::tr(
                &format!("Seite {} gehört zu keiner Rechnung (vor der ersten erkannten Rechnung).", idx + 1),
                &format!("Page {} does not belong to any invoice (before the first detected invoice).", idx + 1),
            ));
        }
    }

    let mut out = Vec::new();
    for g in gruppen {
        if g.fremd {
            warnungen.push(crate::i18n::tr(
                &format!("Seite {}–{}: Formulartyp nicht erkannt, übersprungen.", g.r.start_page + 1, g.r.end_page + 1),
                &format!("Pages {}–{}: form type not recognized, skipped.", g.r.start_page + 1, g.r.end_page + 1),
            ));
        } else {
            out.push(g.r);
        }
    }
    for (i, r) in out.iter_mut().enumerate() {
        r.nr = i + 1;
    }
    out
}

/// "007" und "7" sind dieselbe Kundennummer, Text-Nummern werden ohne Rücksicht auf Case verglichen.
pub fn knr_norm(s: &str) -> String {
    let t = s.trim();
    if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
        let n = t.trim_start_matches('0');
        if n.is_empty() { "0".into() } else { n.into() }
    } else {
        t.to_lowercase()
    }
}

pub fn mit_kunden_abgleichen(rechnungen: &mut [Rechnung], kunden: &[Kunde]) {
    let map: BTreeMap<String, &Kunde> = kunden.iter().map(|k| (knr_norm(&k.kundennummer), k)).collect();
    for r in rechnungen {
        match map.get(&knr_norm(&r.kundennummer)) {
            Some(k) => {
                r.in_liste = true;
                r.kundenname = if k.kundenname.is_empty() { r.kundenname_pdf.clone() } else { k.kundenname.clone() };
                r.email = k.email.clone();
                if k.email.trim().is_empty() {
                    r.warnungen.push(crate::i18n::tr("Keine E-Mail-Adresse hinterlegt", "No email address on file"));
                }
            }
            None => {
                r.kundenname = r.kundenname_pdf.clone();
                if !r.kundennummer.is_empty() {
                    r.warnungen.push(crate::i18n::tr("Kunde nicht in der Kundenliste", "Customer not in the customer list"));
                }
            }
        }
    }
}

pub fn analysiere_pdf(bytes: &[u8], frames: &[FrameDef], toleranz_pt: f64) -> Result<Analyse, String> {
    let (mut vorlage, mut warnungen) = vorlage_bauen(frames)?;
    vorlage.toleranz = toleranz_pt;
    let extracted = extract_pages(bytes)?;
    warnungen.extend(extracted.warnings);

    if extracted.pages.iter().all(|p| p.glyphs.is_empty()) {
        return Err(crate::i18n::tr("Die PDF enthält keinen auswertbaren Text. Eingescannte Rechnungen werden in dieser Version nicht unterstützt.", "The PDF contains no analyzable text. Scanned invoices are not supported in this version."));
    }

    let proben: Vec<SeitenProbe> =
        extracted.pages.iter().enumerate().map(|(i, p)| seite_auswerten(p, &vorlage, i + 1)).collect();
    let rechnungen = gruppieren(&proben, &vorlage, &mut warnungen);
    if rechnungen.is_empty() {
        warnungen.push(crate::i18n::tr("Es wurde keine Rechnung erkannt. Bitte Rahmen und Parameter prüfen.", "No invoice was detected. Please check the frames and parameters."));
    }
    Ok(Analyse { seiten: proben.len(), rechnungen, proben, warnungen })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_text::Glyph;

    /// Seite aus Textstuecken (x, y, text); jedes Zeichen 6 pt breit bei Schriftgroesse 10.
    fn seite(teile: &[(f64, f64, &str)]) -> PageText {
        let mut glyphs = Vec::new();
        for (x, y, text) in teile {
            for (i, ch) in text.chars().enumerate() {
                glyphs.push(Glyph { cx: x + i as f64 * 6.0 + 3.0, cy: *y, w: 6.0, size: 10.0, ch: ch.to_string() });
            }
        }
        PageText { glyphs }
    }

    fn frame(kind: FrameType, x: f64, y: f64, w: f64, h: f64, params: &str) -> FrameDef {
        FrameDef { kind, page: 0, x, y, w, h, params: params.into() }
    }

    // ---- Muster

    #[test]
    fn muster_syntax() {
        let m = parse_muster("RECHNUNG*[RE];LIEFERSCHEIN; Seite *[se] von").unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].var, Some(Var::Re));
        assert_eq!(m[1].var, None);
        assert_eq!(m[2].label, "Seite");
        assert_eq!(m[2].suffix, "von");
        assert!(parse_muster("Nr*[XX]").is_err());
        assert!(parse_muster("Nr*[KU").is_err());
        assert!(parse_muster("  ;  ").is_err());
    }

    #[test]
    fn auswerten_label_und_kennung() {
        let m = parse_muster("Kundennummer*[KU]").unwrap();
        let t = auswerten(&m, "Kundennummer: 1525", "Kundennummer: 1525").unwrap();
        assert_eq!(t.werte, vec![(Var::Ku, "1525".to_string())]);
        // Gross-/Kleinschreibung und zerrissene Wörter sind egal
        let t = auswerten(&m, "KUNDEN NUMMER 77", "KUNDEN NUMMER 77").unwrap();
        assert_eq!(t.werte[0].1, "77");
        assert!(auswerten(&m, "Rechnung 5", "Rechnung 5").is_none());
    }

    #[test]
    fn auswerten_ohne_label_und_mit_suffix() {
        let m = parse_muster("*[KU]").unwrap();
        assert_eq!(auswerten(&m, "1525", "1525").unwrap().werte[0].1, "1525");
        assert_eq!(auswerten(&m, "1525 Loisl's", "1525 Loisl's").unwrap().werte[0].1, "1525");
        assert!(auswerten(&m, "", "").is_none());
        let m = parse_muster("Seite*[SE] von").unwrap();
        assert_eq!(auswerten(&m, "Seite 2 von 5", "Seite 2 von 5").unwrap().werte[0].1, "2");
    }

    #[test]
    fn auswerten_alternativen_und_typ() {
        let m = parse_muster("RECHNUNG*[RE];LIEFERSCHEIN").unwrap();
        let t = auswerten(&m, "Lieferschein Nr. L-42", "Lieferschein Nr. L-42").unwrap();
        assert_eq!(t.label, "LIEFERSCHEIN");
        assert!(t.werte.is_empty());
        let t = auswerten(&m, "RECHNUNG R-7", "RECHNUNG R-7").unwrap();
        assert_eq!((t.label.as_str(), t.werte[0].1.as_str()), ("RECHNUNG", "R-7"));
        let m = parse_muster("Nr*[RE]").unwrap();
        assert_eq!(auswerten(&m, "KOPIE (Ziel-Rechnung) Nr 18562", "KOPIE (Ziel-Rechnung) Nr 18562").unwrap().werte[0].1, "18562");
    }

    #[test]
    fn auswerten_name_mit_mehreren_woertern() {
        let m = parse_muster("*[NA]").unwrap();
        let t = auswerten(&m, "Loisl'sSchirmbar GmbH", "Loisl's Schirmbar GmbH").unwrap();
        assert_eq!(t.werte[0].1, "Loisl's Schirmbar GmbH");
    }

    // ---- Region

    #[test]
    fn region_zeilen_und_leerzeichen() {
        let p = seite(&[(10.0, 100.0, "Nr"), (40.0, 100.0, "18562"), (10.0, 120.0, "unten")]);
        let t = region_text(&p, 0.0, 90.0, 200.0, 20.0, 0.3);
        assert_eq!(t, "Nr 18562");
        let t = region_text(&p, 0.0, 90.0, 200.0, 40.0, 0.3);
        assert_eq!(t, "Nr 18562\nunten");
    }

    // ---- Gruppierung

    fn vorlage_standard() -> Vec<FrameDef> {
        vec![
            frame(FrameType::Rot, 10.0, 40.0, 150.0, 20.0, "RECHNUNG*[RE];LIEFERSCHEIN"),
            frame(FrameType::Blau, 10.0, 70.0, 150.0, 20.0, "Kundennummer*[KU]"),
            frame(FrameType::Violett, 400.0, 70.0, 100.0, 20.0, "Seite*[SE]"),
        ]
    }

    fn rechnungsseite(typ: &str, kunde: &str, seite_nr: Option<u32>) -> PageText {
        let mut teile = vec![(10.0, 50.0, typ), (10.0, 80.0, kunde)];
        let s;
        if let Some(n) = seite_nr {
            s = format!("Seite: {n}");
            teile.push((400.0, 80.0, &s));
            return seite(&teile);
        }
        seite(&teile)
    }

    fn lauf(frames: &[FrameDef], pages: Vec<PageText>) -> (Vec<Rechnung>, Vec<String>) {
        let (v, _) = vorlage_bauen(frames).unwrap();
        let proben: Vec<_> = pages.iter().enumerate().map(|(i, p)| seite_auswerten(p, &v, i + 1)).collect();
        let mut w = vec![];
        let r = gruppieren(&proben, &v, &mut w);
        (r, w)
    }

    #[test]
    fn gruppierung_mit_seitennummer() {
        let pages = vec![
            rechnungsseite("RECHNUNG 100", "Kundennummer: 1", Some(1)),
            rechnungsseite("RECHNUNG 100", "Kundennummer: 1", Some(2)),
            rechnungsseite("RECHNUNG 101", "Kundennummer: 2", Some(1)),
        ];
        let (r, w) = lauf(&vorlage_standard(), pages);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].kundennummer.as_str(), r[0].rechnungsnummer.as_str(), r[0].start_page, r[0].end_page), ("1", "100", 0, 1));
        assert_eq!((r[1].kundennummer.as_str(), r[1].start_page, r[1].end_page), ("2", 2, 2));
    }

    #[test]
    fn gruppierung_ohne_seitennummer_nur_mit_rot() {
        let frames = vec![
            frame(FrameType::Rot, 10.0, 40.0, 150.0, 20.0, "RECHNUNG*[RE]"),
            frame(FrameType::Blau, 10.0, 70.0, 150.0, 20.0, "Kundennummer*[KU]"),
        ];
        let folge = seite(&[(10.0, 200.0, "Übertrag")]); // Folgeseite ohne Kopf
        let pages = vec![
            rechnungsseite("RECHNUNG 100", "Kundennummer: 1", None),
            folge,
            rechnungsseite("RECHNUNG 101", "Kundennummer: 2", None),
        ];
        let (r, _) = lauf(&frames, pages);
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].start_page, r[0].end_page), (0, 1));
        assert_eq!((r[1].start_page, r[1].end_page), (2, 2));
    }

    #[test]
    fn wiederholter_kopf_ohne_seitennummer_gehoert_zur_selben_rechnung() {
        let frames = vec![
            frame(FrameType::Rot, 10.0, 40.0, 150.0, 20.0, "RECHNUNG*[RE]"),
            frame(FrameType::Blau, 10.0, 70.0, 150.0, 20.0, "Kundennummer*[KU]"),
        ];
        let pages = vec![
            rechnungsseite("RECHNUNG 100", "Kundennummer: 1", None),
            rechnungsseite("RECHNUNG 100", "Kundennummer: 1", None),
        ];
        let (r, _) = lauf(&frames, pages);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].end_page, 1);
    }

    #[test]
    fn fremder_formulartyp_wird_uebersprungen() {
        let pages = vec![
            rechnungsseite("RECHNUNG 100", "Kundennummer: 1", Some(1)),
            rechnungsseite("MAHNUNG 5", "Kundennummer: 1", Some(1)),
            rechnungsseite("LIEFERSCHEIN", "Kundennummer: 3", Some(1)),
        ];
        let (r, w) = lauf(&vorlage_standard(), pages);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].formulartyp, "LIEFERSCHEIN");
        assert!(w.iter().any(|m| m.contains("Seite 2–2")), "{w:?}");
    }

    #[test]
    fn verschobener_kopf_wird_als_anker_gefunden() {
        // Auf Seite 2 liegt der Kopf wegen einer längeren Adresse 12 pt tiefer.
        let p1 = rechnungsseite("RECHNUNG 100", "Kundennummer: 1", Some(1));
        let p2 = seite(&[(10.0, 62.0, "RECHNUNG 101"), (10.0, 80.0, "Kundennummer: 2"), (400.0, 80.0, "Seite: 1")]);
        let (r, _) = lauf(&vorlage_standard(), vec![p1, p2]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].rechnungsnummer, "101");
        // ohne Text zum Suchen (nur *[KU]) wird nicht verschoben
        let ohne_label = vec![frame(FrameType::Blau, 10.0, 70.0, 150.0, 20.0, "*[KU]")];
        let (v, _) = vorlage_bauen(&ohne_label).unwrap();
        let tief = seite(&[(10.0, 100.0, "7")]);
        assert!(seite_auswerten(&tief, &v, 1).kundennummer.is_none());
    }

    #[test]
    fn toleranz_ist_einstellbar() {
        // Kundennummer steht 45 pt tiefer als der Rahmen (Rahmen y=70..90, Text bei y=125).
        let seite_tief = seite(&[(10.0, 125.0, "Kundennummer: 5")]);
        let frames = vec![frame(FrameType::Blau, 10.0, 70.0, 200.0, 20.0, "Kundennummer*[KU]")];
        let (mut v, _) = vorlage_bauen(&frames).unwrap();
        assert!(seite_auswerten(&seite_tief, &v, 1).kundennummer.is_none()); // Standard 30 pt reicht nicht
        v.toleranz = 60.0;
        assert_eq!(seite_auswerten(&seite_tief, &v, 1).kundennummer.as_deref(), Some("5"));
        v.toleranz = 0.0; // 0 = keine Suche, nur der Rahmen selbst
        let seite_ok = seite(&[(10.0, 72.0, "Kundennummer: 5")]);
        assert_eq!(seite_auswerten(&seite_ok, &v, 1).kundennummer.as_deref(), Some("5"));
        assert!(seite_auswerten(&seite(&[(10.0, 85.0, "Kundennummer: 5")]), &v, 1).kundennummer.is_some());
    }

    #[test]
    fn fehlende_kundennummer_wird_gemeldet() {
        let pages = vec![rechnungsseite("RECHNUNG 100", "irgendwas", Some(1))];
        let (r, _) = lauf(&vorlage_standard(), pages);
        assert_eq!(r.len(), 1);
        assert!(r[0].warnungen.iter().any(|w| w.contains("Kundennummer")));
    }

    #[test]
    fn vorlage_pflichtpruefung() {
        assert!(vorlage_bauen(&[frame(FrameType::Rot, 0.0, 0.0, 10.0, 10.0, "RECHNUNG")]).is_err());
        assert!(vorlage_bauen(&[frame(FrameType::Blau, 0.0, 0.0, 10.0, 10.0, "Kundennummer")]).is_err());
        assert!(vorlage_bauen(&[frame(FrameType::Blau, 0.0, 0.0, 10.0, 10.0, "Kundennummer*[KU]")]).is_ok());
    }

    #[test]
    fn kunden_abgleich_fuehrende_nullen() {
        let mut r = vec![Rechnung { kundennummer: "0042".into(), ..Default::default() }];
        let k = vec![Kunde { id: 1, kundennummer: "42".into(), kundenname: "Muster".into(), email: "a@b.at".into() }];
        mit_kunden_abgleichen(&mut r, &k);
        assert!(r[0].in_liste);
        assert_eq!(r[0].email, "a@b.at");
    }

    #[test]
    fn json_format_passt_zum_frontend() {
        // genau so speichert/sendet der Editor seine Rahmen
        let json = r#"[{"type":"gruen","page":0,"x":13.4,"y":204.8,"w":202.0,"h":22.2,"params":"Nr*[RE]"},
                       {"type":"violett","page":2,"x":1,"y":2,"w":3,"h":4,"params":""}]"#;
        let frames: Vec<FrameDef> = serde_json::from_str(json).unwrap();
        assert_eq!(frames[0].kind, FrameType::Gruen);
        assert_eq!(frames[1].kind, FrameType::Violett);
        // und so liest die UI das Ergebnis: Rahmentypen als Text-Schluessel
        let mut p = SeitenProbe { seite: 1, ..Default::default() };
        p.treffer.insert(FrameType::Rot, true);
        p.versatz.insert(FrameType::Gruen, 12.0);
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["treffer"]["rot"], true);
        assert_eq!(v["versatz"]["gruen"], 12.0);
    }

    /// Echte DOS-Rechnungs-PDFs: B2M_PDF=<datei> cargo test real -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real() {
        let Ok(path) = std::env::var("B2M_PDF") else { return };
        // A: Siegfried Maislinger, B: Prindl, sonst: Rudigier
        let frames = match std::env::var("B2M_LAYOUT").as_deref() {
            Ok("A") => vec![
                frame(FrameType::Rot, 40.0, 251.0, 270.0, 13.0, "Rechnung;Lieferschein"),
                frame(FrameType::Blau, 50.0, 119.0, 230.0, 14.0, "*[KU]"),
                frame(FrameType::Gruen, 40.0, 251.0, 270.0, 13.0, "Nr*[RE]"),
                frame(FrameType::Violett, 400.0, 143.0, 180.0, 14.0, "Seite*[SE]"),
            ],
            Ok("B") => vec![
                frame(FrameType::Rot, -5.0, 167.0, 300.0, 13.0, "Rechnung;Lieferschein"),
                frame(FrameType::Blau, 10.0, 48.0, 250.0, 13.0, "*[KU]"),
                frame(FrameType::Gruen, -5.0, 167.0, 300.0, 13.0, "Nr*[RE]"),
                frame(FrameType::Violett, 400.0, 84.0, 160.0, 13.0, "Seite*[SE]"),
            ],
            _ => vec![
                frame(FrameType::Rot, 10.0, 205.0, 300.0, 16.0, "Rechnung;Lieferschein"),
                frame(FrameType::Blau, 10.0, 99.0, 120.0, 16.0, "*[KU]"),
                frame(FrameType::Gruen, 10.0, 205.0, 470.0, 16.0, "Nr*[RE]"),
                frame(FrameType::Violett, 400.0, 124.0, 180.0, 16.0, "Seite*[SE]"),
            ],
        };
        let t = std::time::Instant::now();
        let a = analysiere_pdf(&std::fs::read(&path).unwrap(), &frames, TOLERANZ_STANDARD_PT).unwrap();
        println!("{path}: {} Seiten, {} Rechnungen, {:?}", a.seiten, a.rechnungen.len(), t.elapsed());
        for r in a.rechnungen.iter().take(8) {
            println!("  #{} KU={} RE={} Typ={} S.{}-{} {:?}", r.nr, r.kundennummer, r.rechnungsnummer, r.formulartyp, r.start_page + 1, r.end_page + 1, r.warnungen);
        }
        let probleme = a.rechnungen.iter().filter(|r| !r.warnungen.is_empty()).count();
        println!("  mit Warnungen: {probleme}; globale Warnungen: {:?}", a.warnungen.iter().take(5).collect::<Vec<_>>());
    }
}
