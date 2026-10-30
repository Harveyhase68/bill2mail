import { invoke } from "@tauri-apps/api/core";
import type { Kunde } from "./types";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class KundenView {
  private kunden: Kunde[] = [];
  private body = $<HTMLTableSectionElement>("kunden-body");
  private filter = $<HTMLInputElement>("kunden-filter");
  private statusTimer = 0;

  constructor() {
    this.filter.addEventListener("input", () => this.render());
    $("btn-add").addEventListener("click", () => void this.add());
    for (const id of ["new-nr", "new-name", "new-email"]) {
      $(id).addEventListener("keydown", (e) => {
        if ((e as KeyboardEvent).key === "Enter") void this.add();
      });
    }
  }

  async reload() {
    this.kunden = await invoke<Kunde[]>("kunden_laden");
    this.render();
  }

  private status(msg: string, isError = false) {
    const el = $("kunden-status");
    el.textContent = msg;
    el.classList.toggle("error", isError);
    clearTimeout(this.statusTimer);
    if (msg) this.statusTimer = window.setTimeout(() => (el.textContent = ""), 5000);
  }

  private async add() {
    const nr = $<HTMLInputElement>("new-nr");
    const name = $<HTMLInputElement>("new-name");
    const email = $<HTMLInputElement>("new-email");
    try {
      await invoke("kunde_speichern", {
        kunde: { id: 0, kundennummer: nr.value, kundenname: name.value, email: email.value },
      });
      nr.value = name.value = email.value = "";
      nr.focus();
      await this.reload();
      this.status("Kunde angelegt.");
    } catch (e) {
      this.status(String(e), true);
    }
  }

  private async save(k: Kunde) {
    try {
      await invoke("kunde_speichern", { kunde: k });
      this.status("Gespeichert.");
    } catch (e) {
      this.status(String(e), true);
      await this.reload(); // fehlerhafte Eingabe zuruecksetzen
    }
  }

  private async remove(k: Kunde) {
    if (!confirm(`Kunde ${k.kundennummer} ${k.kundenname} wirklich löschen?`)) return;
    try {
      await invoke("kunde_loeschen", { id: k.id });
      await this.reload();
    } catch (e) {
      this.status(String(e), true);
    }
  }

  private render() {
    const q = this.filter.value.trim().toLowerCase();
    const rows = q
      ? this.kunden.filter((k) => `${k.kundennummer} ${k.kundenname} ${k.email}`.toLowerCase().includes(q))
      : this.kunden;
    $("kunden-count").textContent = `${rows.length} von ${this.kunden.length} Kunden`;

    this.body.replaceChildren(
      ...rows.map((k) => {
        const tr = document.createElement("tr");
        const field = (key: "kundennummer" | "kundenname" | "email") => {
          const td = document.createElement("td");
          const input = document.createElement("input");
          input.value = k[key];
          input.addEventListener("change", () => {
            k[key] = input.value;
            void this.save(k);
          });
          td.append(input);
          return td;
        };
        const del = document.createElement("td");
        const btn = document.createElement("button");
        btn.className = "danger";
        btn.textContent = "Löschen";
        btn.addEventListener("click", () => void this.remove(k));
        del.append(btn);
        tr.append(field("kundennummer"), field("kundenname"), field("email"), del);
        return tr;
      }),
    );
  }
}
