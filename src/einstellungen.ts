import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Einstellungen, UeberwachungsStatus } from "./types";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const input = (id: string) => $<HTMLInputElement>(id);

const STANDARD_PORTS: Record<string, number> = { starttls: 587, ssl: 465, keine: 25 };

export class EinstellungenView {
  private statusTimer = 0;

  constructor() {
    $("s-save").addEventListener("click", () => void this.save().catch((e) => this.status(String(e), true)));
    $("s-ue-pick").addEventListener("click", async () => {
      const dir = await open({ directory: true, multiple: false });
      if (typeof dir === "string") input("s-ue-ordner").value = dir;
    });
    $("s-test").addEventListener("click", () => void this.test());
    $<HTMLSelectElement>("s-sicherheit").addEventListener("change", (e) => {
      const port = Number(input("s-port").value);
      if (Object.values(STANDARD_PORTS).includes(port)) {
        input("s-port").value = String(STANDARD_PORTS[(e.target as HTMLSelectElement).value]);
      }
    });
  }

  private status(msg: string, isError = false) {
    const el = $("s-status");
    el.textContent = msg;
    el.classList.toggle("error", isError);
    clearTimeout(this.statusTimer);
    if (msg && !isError) this.statusTimer = window.setTimeout(() => (el.textContent = ""), 6000);
  }

  private ueStatus(s: UeberwachungsStatus) {
    const el = $("s-ue-status");
    el.textContent = s.meldung;
    el.classList.toggle("error", !s.aktiv && s.meldung !== "" && input("s-ue-aktiv").checked);
  }

  async load() {
    const e = await invoke<Einstellungen & { passwort_gesetzt: boolean; ueberwachung: UeberwachungsStatus }>(
      "einstellungen_laden",
    );
    input("s-ue-aktiv").checked = e.ueberwachung_aktiv;
    input("s-ue-ordner").value = e.ueberwachung_ordner;
    input("s-toleranz").value = String(e.toleranz_pt);
    this.ueStatus(e.ueberwachung);
    input("s-host").value = e.smtp_host;
    input("s-port").value = String(e.smtp_port);
    $<HTMLSelectElement>("s-sicherheit").value = e.sicherheit;
    input("s-user").value = e.smtp_user;
    input("s-pass").value = "";
    input("s-pass").placeholder = e.passwort_gesetzt ? "(gespeichert – leer lassen zum Beibehalten)" : "";
    input("s-absender").value = e.absender;
    input("s-absender-name").value = e.absender_name;
    input("s-test-an").value = e.test_empfaenger;
    input("s-betreff").value = e.betreff;
    $<HTMLTextAreaElement>("s-text").value = e.text;
  }

  async save() {
    const einstellungen: Einstellungen = {
      smtp_host: input("s-host").value.trim(),
      smtp_port: Number(input("s-port").value) || 587,
      sicherheit: $<HTMLSelectElement>("s-sicherheit").value as Einstellungen["sicherheit"],
      smtp_user: input("s-user").value.trim(),
      absender: input("s-absender").value.trim(),
      absender_name: input("s-absender-name").value.trim(),
      betreff: input("s-betreff").value,
      text: $<HTMLTextAreaElement>("s-text").value,
      test_empfaenger: input("s-test-an").value.trim(),
      ueberwachung_aktiv: input("s-ue-aktiv").checked,
      ueberwachung_ordner: input("s-ue-ordner").value.trim(),
      toleranz_pt: Number.isFinite(parseFloat(input("s-toleranz").value)) ? parseFloat(input("s-toleranz").value) : 30,
    };
    const pass = input("s-pass").value;
    const ue = await invoke<UeberwachungsStatus>("einstellungen_speichern", {
      einstellungen,
      passwort: pass === "" ? null : pass,
    });
    this.ueStatus(ue);
    input("s-pass").value = "";
    if (pass !== "") input("s-pass").placeholder = "(gespeichert – leer lassen zum Beibehalten)";
    this.status("Gespeichert.");
  }

  private async test() {
    try {
      await this.save();
      this.status("Sende Test-Mail …");
      this.status(await invoke<string>("test_mail_senden"));
    } catch (e) {
      this.status(String(e), true);
    }
  }
}
