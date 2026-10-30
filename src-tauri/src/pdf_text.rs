//! Positionsgenaue Textextraktion. Die Koordinaten entsprechen denen von pdf.js im
//! Editor: PDF-Punkte, Ursprung links oben auf der sichtbaren Seite (CropBox).

use lopdf::{Document, Object, ObjectId};
use pdf_extract::{MediaBox, OutputDev, OutputError, Transform};
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Debug, Clone)]
pub struct Glyph {
    /// Mittelpunkt des Zeichens
    pub cx: f64,
    pub cy: f64,
    pub w: f64,
    pub size: f64,
    pub ch: String,
}

#[derive(Debug, Clone, Default)]
pub struct PageText {
    pub glyphs: Vec<Glyph>,
}

pub struct Extracted {
    pub pages: Vec<PageText>,
    pub warnings: Vec<String>,
}

struct PageBox {
    llx: f64,
    lly: f64,
    urx: f64,
    ury: f64,
    rotate: i64,
}

fn inherited<'a>(doc: &'a Document, mut id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    for _ in 0..32 {
        let dict = doc.get_dictionary(id).ok()?;
        if let Ok(o) = dict.get(key) {
            return Some(o);
        }
        id = dict.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn number(doc: &Document, o: &Object) -> Option<f64> {
    match doc.dereference(o).ok()?.1 {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

fn page_box(doc: &Document, id: ObjectId) -> PageBox {
    let rect = inherited(doc, id, b"CropBox")
        .or_else(|| inherited(doc, id, b"MediaBox"))
        .and_then(|o| doc.dereference(o).ok())
        .and_then(|(_, o)| o.as_array().ok())
        .and_then(|a| {
            let n: Vec<f64> = a.iter().filter_map(|o| number(doc, o)).collect();
            (n.len() == 4).then(|| (n[0].min(n[2]), n[1].min(n[3]), n[0].max(n[2]), n[1].max(n[3])))
        })
        .unwrap_or((0.0, 0.0, 612.0, 792.0));
    let rotate = inherited(doc, id, b"Rotate")
        .and_then(|o| number(doc, o))
        .map(|r| (r as i64).rem_euclid(360))
        .unwrap_or(0);
    PageBox { llx: rect.0, lly: rect.1, urx: rect.2, ury: rect.3, rotate }
}

struct Collector {
    llx: f64,
    ury: f64,
    glyphs: Vec<Glyph>,
}

impl OutputDev for Collector {
    fn begin_page(&mut self, _: u32, _: &MediaBox, _: Option<(f64, f64, f64, f64)>) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        _spacing: f64,
        font_size: f64,
        ch: &str,
    ) -> Result<(), OutputError> {
        let vx = font_size * (trm.m11 + trm.m21);
        let vy = font_size * (trm.m12 + trm.m22);
        let size = (vx * vy).abs().sqrt();
        if ch.is_empty() || !size.is_finite() || size < 0.1 {
            return Ok(());
        }
        let x = trm.m31 - self.llx;
        let baseline = self.ury - trm.m32;
        let w = (width * size).abs();
        self.glyphs.push(Glyph { cx: x + w / 2.0, cy: baseline - 0.3 * size, w, size, ch: ch.to_string() });
        Ok(())
    }
    fn begin_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
}

/// Dreht Mittelpunkte einer um `rotate` Grad (im Uhrzeigersinn) gedrehten Seite in die
/// Darstellung, die pdf.js anzeigt.
fn rotate_point(rotate: i64, w: f64, h: f64, x: f64, y: f64) -> (f64, f64) {
    match rotate {
        90 => (h - y, x),
        180 => (w - x, h - y),
        270 => (y, w - x),
        _ => (x, y),
    }
}

pub fn extract_pages(bytes: &[u8]) -> Result<Extracted, String> {
    let doc = Document::load_mem(bytes).map_err(|e| format!("Fehler beim Lesen der PDF: {e}"))?;
    if doc.is_encrypted() {
        return Err("Die PDF ist passwortgeschützt und kann nicht ausgewertet werden.".into());
    }
    let mut pages = Vec::new();
    let mut warnings = Vec::new();

    for (num, id) in doc.get_pages() {
        let b = page_box(&doc, id);
        let (w, h) = (b.urx - b.llx, b.ury - b.lly);
        let mut c = Collector { llx: b.llx, ury: b.ury, glyphs: Vec::new() };

        let result = catch_unwind(AssertUnwindSafe(|| pdf_extract::output_doc_page(&doc, &mut c, num)));
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => warnings.push(format!("Seite {num}: Text konnte nicht gelesen werden ({e:?}).")),
            Err(_) => warnings.push(format!("Seite {num}: Text konnte nicht gelesen werden (interner Fehler).")),
        }

        if b.rotate != 0 {
            warnings.push(format!("Seite {num} ist gedreht ({}°), die Texterkennung ist dort eingeschränkt.", b.rotate));
            for g in &mut c.glyphs {
                (g.cx, g.cy) = rotate_point(b.rotate, w, h, g.cx, g.cy);
            }
        }
        pages.push(PageText { glyphs: c.glyphs });
    }
    Ok(Extracted { pages, warnings })
}

/// Text aller Zeichen, deren Mittelpunkt im Rechteck liegt (mit kleiner Toleranz).
/// `spacing` ist die Luecke (als Vielfaches der Schriftgroesse), ab der ein
/// Leerzeichen eingefuegt wird. Mehrere Zeilen werden mit '\n' getrennt.
pub fn region_text(page: &PageText, x: f64, y: f64, w: f64, h: f64, spacing: f64) -> String {
    const TOL: f64 = 1.5;
    let mut hits: Vec<&Glyph> = page
        .glyphs
        .iter()
        .filter(|g| g.cx >= x - TOL && g.cx <= x + w + TOL && g.cy >= y - TOL && g.cy <= y + h + TOL)
        .collect();
    hits.sort_by(|a, b| a.cy.total_cmp(&b.cy));

    // Zeilen bilden: Zeichen mit aehnlicher Hoehe gehoeren zusammen.
    let mut lines: Vec<Vec<&Glyph>> = Vec::new();
    for g in hits {
        match lines.last_mut() {
            Some(line) if (line[0].cy - g.cy).abs() <= 0.6 * line[0].size => line.push(g),
            _ => lines.push(vec![g]),
        }
    }

    let mut out_lines = Vec::new();
    for mut line in lines {
        line.sort_by(|a, b| a.cx.total_cmp(&b.cx));
        let mut s = String::new();
        let mut prev_end: Option<f64> = None;
        for g in line {
            if let Some(end) = prev_end {
                if g.cx - g.w / 2.0 > end + g.size * spacing {
                    s.push(' ');
                }
            }
            if g.ch.trim().is_empty() {
                s.push(' ');
            } else {
                s.push_str(&g.ch);
            }
            prev_end = Some(g.cx + g.w / 2.0);
        }
        let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
            out_lines.push(collapsed);
        }
    }
    out_lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gibt Zeilen mit Position aus, um Rahmen fuer echte PDFs zu bestimmen:
    /// B2M_PDF=<datei> cargo test dump -- --ignored --nocapture
    #[test]
    #[ignore]
    fn dump() {
        let Ok(path) = std::env::var("B2M_PDF") else { return };
        let max: usize = std::env::var("B2M_PAGES").ok().and_then(|s| s.parse().ok()).unwrap_or(2);
        let ex = extract_pages(&std::fs::read(path).unwrap()).unwrap();
        println!("{} Seiten, Warnungen: {:?}", ex.pages.len(), ex.warnings);
        for (n, p) in ex.pages.iter().take(max).enumerate() {
            println!("=== Seite {}", n + 1);
            let mut gl: Vec<&Glyph> = p.glyphs.iter().collect();
            gl.sort_by(|a, b| a.cy.total_cmp(&b.cy));
            let mut lines: Vec<Vec<&Glyph>> = vec![];
            for g in gl {
                match lines.last_mut() {
                    Some(l) if (l[0].cy - g.cy).abs() <= 0.6 * l[0].size => l.push(g),
                    _ => lines.push(vec![g]),
                }
            }
            for l in lines {
                let (x0, x1) = (l.iter().map(|g| g.cx - g.w / 2.0).fold(f64::MAX, f64::min), l.iter().map(|g| g.cx + g.w / 2.0).fold(f64::MIN, f64::max));
                let text = region_text(p, x0 - 1.0, l[0].cy - 3.0, x1 - x0 + 2.0, 6.0, 0.3);
                println!("y={:6.1} x={:6.1}..{:6.1}  {}", l[0].cy, x0, x1, text.replace('\n', " | "));
            }
        }
    }
}

#[cfg(test)]
pub mod testutil {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    /// Erzeugt ein PDF (A4, Courier 12 pt). Jede Seite: Liste von (x, y von oben, Text).
    pub fn test_pdf(seiten: &[&[(f64, f64, &str)]]) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier" });
        let res = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
        let mut kids = Vec::new();
        for teile in seiten {
            let mut ops = Vec::new();
            for (x, y, text) in *teile {
                ops.push(Operation::new("BT", vec![]));
                ops.push(Operation::new("Tf", vec!["F1".into(), 12.into()]));
                ops.push(Operation::new("Td", vec![Object::Real(*x as f32), Object::Real((842.0 - y) as f32)]));
                ops.push(Operation::new("Tj", vec![Object::string_literal(*text)]));
                ops.push(Operation::new("ET", vec![]));
            }
            let content = doc.add_object(Stream::new(dictionary! {}, Content { operations: ops }.encode().unwrap()));
            let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id, "Contents" => content });
            kids.push(Object::Reference(page));
        }
        let n = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => kids, "Count" => n, "Resources" => res,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }
}
