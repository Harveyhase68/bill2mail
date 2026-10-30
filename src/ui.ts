import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ProtokollZeile, VersandErgebnis, Vorschau } from "./types";

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

export function row(cells: (string | HTMLElement)[], cls = ""): HTMLTableRowElement {
  const tr = el("tr", cls);
  for (const c of cells) {
    const td = el("td");
    if (typeof c === "string") td.textContent = c;
    else td.append(c);
    tr.append(td);
  }
  return tr;
}

export function table(head: string[]): { table: HTMLTableElement; body: HTMLTableSectionElement } {
  const t = el("table", "result static");
  const thead = el("thead");
  const tr = el("tr");
  for (const h of head) tr.append(el("th", "", h));
  thead.append(tr);
  const body = el("tbody");
  t.append(thead, body);
  return { table: t, body };
}

/** Es darf immer nur ein Versand gleichzeitig laufen (manuell oder aus der Automatik). */
export const versandSperre = { aktiv: false };

const STATUS_TEXT = { gesendet: "Gesendet", uebersprungen: "Übersprungen", fehler: "Fehler" } as const;

function zeilenRow(z: ProtokollZeile): HTMLTableRowElement {
  const cls = z.status === "gesendet" ? "ok" : z.status === "fehler" ? "bad" : "skip";
  return row(
    [
      String(z.nr),
      z.kundennummer,
      z.kundenname,
      z.rechnungsnummer,
      z.seiten,
      z.email.replaceAll(";", "; "),
      el("span", `chip ${cls}`, STATUS_TEXT[z.status]),
      z.fehler,
    ],
    z.status === "fehler" ? "warn" : "",
  );
}

/** Zusammenfassung, Hinweise und Tabelle der geplanten E-Mails. */
export function vorschauTeile(v: Vorschau, mitMailText: boolean): HTMLElement[] {
  const senden = v.mails.filter((m) => m.senden);
  const skip = v.mails.length - senden.length;
  const parts: HTMLElement[] = [];

  if (v.bereits_gesendet) parts.push(el("div", "banner bad", `⚠ ${v.bereits_gesendet} Nochmal senden erzeugt doppelte E-Mails.`));

  parts.push(
    el(
      "p",
      "v-summary",
      `${v.analyse.rechnungen.length} Rechnungen aus ${v.analyse.seiten} Seiten · ${senden.length} E-Mails werden gesendet` +
        (skip ? ` · ${skip} Kunden werden übersprungen` : ""),
    ),
  );

  if (v.analyse.warnungen.length > 0) {
    const ul = el("ul", "analyse-warnings");
    for (const w of v.analyse.warnungen) ul.append(el("li", "", w));
    parts.push(ul);
  }

  const { table: t, body } = table(["", "Kundennr.", "Kunde", "E-Mail", "Rechnungen", "Seiten", "Grund"]);
  for (const m of v.mails) {
    const nrs = v.analyse.rechnungen.filter((r) => m.rechnungen.includes(r.nr)).map((r) => r.rechnungsnummer || `#${r.nr}`);
    body.append(
      row(
        [
          el("span", m.senden ? "chip ok" : "chip skip", m.senden ? "senden" : "überspringen"),
          m.kundennummer,
          m.kundenname,
          m.email.replaceAll(";", "; "),
          nrs.join(", "),
          String(m.seiten),
          m.grund,
        ],
        m.senden ? "" : "warn",
      ),
    );
  }
  parts.push(t);

  if (mitMailText) {
    const mail = el("details", "v-mail");
    mail.append(el("summary", "", `So sieht die E-Mail aus – Betreff: ${v.betreff}`));
    mail.append(el("div", "hint", `Von: ${v.absender || "(kein Absender eingetragen)"}`));
    const frame = el("iframe", "mail-frame");
    frame.setAttribute("sandbox", ""); // kein Skript, keine Navigation
    frame.srcdoc = `<body style="font:14px Segoe UI,sans-serif">${v.text_html}</body>`;
    mail.append(frame);
    parts.push(mail);
  }
  return parts;
}

export function ergebnisTeile(e: VersandErgebnis): HTMLElement[] {
  const parts: HTMLElement[] = [];
  parts.push(
    el(
      "p",
      "v-summary" + (e.mails_fehler > 0 ? " bad" : ""),
      `${e.mails_gesendet} E-Mails gesendet` +
        (e.mails_fehler ? ` · ${e.mails_fehler} mit Fehler` : "") +
        (e.rechnungen_uebersprungen ? ` · ${e.rechnungen_uebersprungen} Rechnungen übersprungen` : "") +
        (e.abgebrochen ? " · abgebrochen" : ""),
    ),
  );
  const links = el("div", "v-links");
  const xlsx = el("button", "", "Protokoll (XLSX) im Explorer zeigen");
  xlsx.addEventListener("click", () => void revealItemInDir(e.protokoll));
  const ordner = el("button", "", "Ordner mit den Einzel-PDFs zeigen");
  ordner.addEventListener("click", () => void revealItemInDir(e.ordner));
  links.append(xlsx, ordner);
  parts.push(links);

  const { table: t, body } = table(["Nr.", "Kundennr.", "Kunde", "Rechnungsnr.", "Seiten", "E-Mail", "Status", "Hinweis"]);
  for (const z of e.zeilen) body.append(zeilenRow(z));
  parts.push(t);
  return parts;
}
