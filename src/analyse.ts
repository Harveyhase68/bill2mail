import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n";
import { FRAME_META, FRAME_ORDER, type Analyse, type FrameDef, type Rechnung } from "./types";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

function cell(text: string, cls = ""): HTMLTableCellElement {
  const td = document.createElement("td");
  td.textContent = text;
  if (cls) td.className = cls;
  return td;
}

export class AnalysePanel {
  private panel = $<HTMLElement>("analyse");
  private body = $<HTMLDivElement>("analyse-body");
  private title = $<HTMLElement>("analyse-title");

  constructor(private gotoPage: (index: number) => void) {
    $("analyse-close").addEventListener("click", () => (this.panel.hidden = true));
  }

  async run(path: string, frames: FrameDef[]) {
    this.panel.hidden = false;
    this.title.textContent = t("Analysiere …");
    this.body.replaceChildren();
    try {
      const a = await invoke<Analyse>("analysiere", { path, frames });
      this.show(a);
    } catch (e) {
      this.title.textContent = t("Analyse nicht möglich");
      const p = document.createElement("p");
      p.className = "analyse-error";
      p.textContent = String(e);
      this.body.replaceChildren(p);
    }
  }

  private show(a: Analyse) {
    const mit = a.rechnungen.filter((r) => r.warnungen.length > 0).length;
    this.title.textContent =
      t("{n} Rechnungen aus {s} Seiten erkannt", { n: a.rechnungen.length, s: a.seiten }) + (mit ? t(" – {m} mit Hinweisen", { m: mit }) : "");

    const detail = document.createElement("div");
    detail.className = "analyse-detail";
    detail.textContent = t("Zeile anklicken: zeigt die gelesenen Rahmentexte der ersten Seite.");

    const table = document.createElement("table");
    table.className = "result";
    table.innerHTML =
      "<thead><tr>" +
      ["Nr.", "Kundennr.", "Kunde", "Rechnungsnr.", "Typ", "Seiten", "E-Mail", "Hinweise"].map((h) => `<th>${t(h)}</th>`).join("") +
      "</tr></thead>";
    const tbody = document.createElement("tbody");
    for (const r of a.rechnungen) {
      const tr = document.createElement("tr");
      tr.classList.toggle("warn", r.warnungen.length > 0);
      tr.append(
        cell(String(r.nr)),
        cell(r.kundennummer),
        cell(r.kundenname),
        cell(r.rechnungsnummer),
        cell(r.formulartyp),
        cell(r.start_page === r.end_page ? `${r.start_page + 1}` : `${r.start_page + 1}–${r.end_page + 1}`),
        cell(r.email.replaceAll(";", "; ")),
        cell(r.warnungen.join(" · "), "hinweis"),
      );
      tr.addEventListener("click", () => {
        tbody.querySelectorAll("tr.active").forEach((t) => t.classList.remove("active"));
        tr.classList.add("active");
        this.gotoPage(r.start_page);
        this.showDetail(detail, a, r);
      });
      tbody.append(tr);
    }
    table.append(tbody);

    const parts: HTMLElement[] = [];
    if (a.warnungen.length > 0) {
      const ul = document.createElement("ul");
      ul.className = "analyse-warnings";
      for (const w of a.warnungen) {
        const li = document.createElement("li");
        li.textContent = w;
        ul.append(li);
      }
      parts.push(ul);
    }
    parts.push(detail, table);
    this.body.replaceChildren(...parts);
  }

  private showDetail(el: HTMLElement, a: Analyse, r: Rechnung) {
    const probe = a.proben[r.start_page];
    el.replaceChildren();
    if (!probe) return;
    const head = document.createElement("b");
    head.textContent = t("Seite {n}: ", { n: probe.seite });
    el.append(head);
    for (const typ of FRAME_ORDER) {
      if (probe.texte[typ] === undefined) continue;
      const span = document.createElement("span");
      span.className = "probe";
      span.style.setProperty("--c", FRAME_META[typ].color);
      const ok = probe.treffer[typ] ? "✓" : "✗";
      const versatz = probe.versatz[typ] ? t(" ({v} pt verschoben)", { v: `${probe.versatz[typ]! > 0 ? "+" : ""}${probe.versatz[typ]}` }) : "";
      span.textContent = `${t(FRAME_META[typ].label)} ${ok} „${probe.texte[typ]!.replaceAll("\n", " ⏎ ")}“${versatz}`;
      el.append(span);
    }
  }
}
