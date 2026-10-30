import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { t } from "./i18n";
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
      el("span", `chip ${cls}`, t(STATUS_TEXT[z.status])),
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

  if (v.bereits_gesendet) parts.push(el("div", "banner bad", t("⚠ {m} Nochmal senden erzeugt doppelte E-Mails.", { m: v.bereits_gesendet })));

  parts.push(
    el(
      "p",
      "v-summary",
      t("{r} Rechnungen aus {s} Seiten · {n} E-Mails werden gesendet", { r: v.analyse.rechnungen.length, s: v.analyse.seiten, n: senden.length }) +
        (skip ? t(" · {k} Kunden werden übersprungen", { k: skip }) : ""),
    ),
  );

  if (v.analyse.warnungen.length > 0) {
    const ul = el("ul", "analyse-warnings");
    for (const w of v.analyse.warnungen) ul.append(el("li", "", w));
    parts.push(ul);
  }

  const { table: tab, body } = table(["", "Kundennr.", "Kunde", "E-Mail", "Rechnungen", "Seiten", "Grund"].map((h) => t(h)));
  for (const m of v.mails) {
    const nrs = v.analyse.rechnungen.filter((r) => m.rechnungen.includes(r.nr)).map((r) => r.rechnungsnummer || `#${r.nr}`);
    body.append(
      row(
        [
          el("span", m.senden ? "chip ok" : "chip skip", t(m.senden ? "senden" : "überspringen")),
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
  parts.push(tab);

  if (mitMailText) {
    const mail = el("details", "v-mail");
    mail.append(el("summary", "", t("So sieht die E-Mail aus – Betreff: {b}", { b: v.betreff })));
    mail.append(el("div", "hint", t("Von: {a}", { a: v.absender || t("(kein Absender eingetragen)") })));
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
      t("{n} E-Mails gesendet", { n: e.mails_gesendet }) +
        (e.mails_fehler ? t(" · {n} mit Fehler", { n: e.mails_fehler }) : "") +
        (e.rechnungen_uebersprungen ? t(" · {n} Rechnungen übersprungen", { n: e.rechnungen_uebersprungen }) : "") +
        (e.abgebrochen ? t(" · abgebrochen") : ""),
    ),
  );
  const links = el("div", "v-links");
  const xlsx = el("button", "", t("Protokoll (XLSX) im Explorer zeigen"));
  xlsx.addEventListener("click", () => void revealItemInDir(e.protokoll));
  const ordner = el("button", "", t("Ordner mit den Einzel-PDFs zeigen"));
  ordner.addEventListener("click", () => void revealItemInDir(e.ordner));
  links.append(xlsx, ordner);
  parts.push(links);

  const { table: tab, body } = table(["Nr.", "Kundennr.", "Kunde", "Rechnungsnr.", "Seiten", "E-Mail", "Status", "Hinweis"].map((h) => t(h)));
  for (const z of e.zeilen) body.append(zeilenRow(z));
  parts.push(tab);
  return parts;
}
