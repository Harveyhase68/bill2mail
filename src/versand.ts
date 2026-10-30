import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask, open } from "@tauri-apps/plugin-dialog";
import type { FrameDef, VersandErgebnis, Vorschau } from "./types";
import { el, ergebnisTeile, versandSperre, vorschauTeile } from "./ui";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class VersandView {
  private pdf: string | null = null;
  /** Nur nach erfolgreicher Vorschau des aktuell gewählten PDFs darf versendet werden. */
  private vorschauFuer: string | null = null;
  private zuSenden = 0;
  private doppelt = false;
  private busy = false;
  private sending = false;

  private body = $<HTMLDivElement>("v-body");
  private status = $<HTMLDivElement>("v-status");

  constructor(private getFrames: () => FrameDef[]) {
    $("v-open").addEventListener("click", () => void this.pick());
    $("v-vorschau").addEventListener("click", () => void this.vorschau());
    $("v-test").addEventListener("click", () => void this.testMail());
    $("v-senden").addEventListener("click", () => void this.senden());
    $("v-abbrechen").addEventListener("click", () => {
      void invoke("versand_abbrechen");
      $<HTMLButtonElement>("v-abbrechen").disabled = true;
      this.setStatus("Abbruch angefordert – die laufende Mail wird noch beendet …");
    });
  }

  private setStatus(msg: string, isError = false) {
    this.status.hidden = msg === "";
    this.status.textContent = msg;
    this.status.classList.toggle("error", isError);
  }

  private updateButtons() {
    $<HTMLButtonElement>("v-open").disabled = this.busy;
    $<HTMLButtonElement>("v-vorschau").disabled = this.busy || !this.pdf;
    $<HTMLButtonElement>("v-test").disabled = this.busy;
    $<HTMLButtonElement>("v-senden").disabled =
      this.busy || !this.pdf || this.vorschauFuer !== this.pdf || this.zuSenden === 0;
    $("v-abbrechen").hidden = !this.busy || !this.sending;
  }

  private async pick() {
    const picked = await open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (typeof picked !== "string") return;
    this.pdf = picked;
    this.vorschauFuer = null;
    this.zuSenden = 0;
    $("v-file").textContent = picked;
    this.setStatus("");
    this.body.replaceChildren(el("div", "empty dark", "PDF gewählt. Jetzt „Vorschau“ drücken."));
    this.updateButtons();
  }

  private async run<T>(fn: () => Promise<T>): Promise<T | undefined> {
    this.busy = true;
    this.updateButtons();
    try {
      return await fn();
    } catch (e) {
      this.setStatus(String(e), true);
      return undefined;
    } finally {
      this.busy = false;
      this.sending = false;
      this.updateButtons();
    }
  }

  private async vorschau() {
    if (!this.pdf) return;
    const pdf = this.pdf;
    this.setStatus("Analysiere PDF …");
    const v = await this.run(() => invoke<Vorschau>("versand_vorschau", { path: pdf, frames: this.getFrames() }));
    if (!v) return;
    this.zuSenden = v.mails.filter((m) => m.senden).length;
    this.doppelt = v.bereits_gesendet !== null;
    this.vorschauFuer = pdf;
    this.setStatus("");
    this.body.replaceChildren(...vorschauTeile(v, true));
    this.updateButtons();
  }

  private async testMail() {
    this.setStatus("Sende Test-Mail …");
    const msg = await this.run(() => invoke<string>("test_mail_senden"));
    if (msg) this.setStatus(msg);
  }

  private async senden() {
    if (!this.pdf || this.vorschauFuer !== this.pdf) return;
    if (versandSperre.aktiv) {
      this.setStatus("Es läuft bereits ein anderer Versand.", true);
      return;
    }
    const pdf = this.pdf;
    const warnung = this.doppelt ? "\n\nACHTUNG: Dieses PDF wurde schon einmal versendet (siehe Vorschau)." : "";
    const ok = await ask(
      `${this.zuSenden} E-Mails jetzt an die Kunden senden?\n\nDas lässt sich nicht rückgängig machen. Haben Sie die Test-Mail geprüft?${warnung}`,
      { title: "Versenden", kind: "warning", okLabel: "Jetzt senden", cancelLabel: "Abbrechen" },
    );
    if (!ok) return;

    const progress = el("div", "v-progress");
    const bar = el("progress");
    bar.max = this.zuSenden;
    bar.value = 0;
    const label = el("span", "", "Bereite vor …");
    progress.append(bar, label);
    this.body.replaceChildren(progress);
    $<HTMLButtonElement>("v-abbrechen").disabled = false;
    this.sending = true;
    versandSperre.aktiv = true;
    this.setStatus("");

    const unlisten = await listen<{ aktuell: number; gesamt: number; text: string }>("versand-fortschritt", (e) => {
      bar.max = e.payload.gesamt;
      bar.value = e.payload.aktuell - 1;
      label.textContent = `${e.payload.aktuell} von ${e.payload.gesamt}: ${e.payload.text}`;
    });
    // Nach dem Versand ist die Vorschau verbraucht, damit nicht versehentlich doppelt gesendet wird.
    this.vorschauFuer = null;
    const erg = await this.run(() => invoke<VersandErgebnis>("versenden", { path: pdf, frames: this.getFrames() }));
    unlisten();
    versandSperre.aktiv = false;
    if (!erg) {
      this.body.replaceChildren(el("div", "empty dark", "Versand nicht durchgeführt. Es wurde nichts gesendet."));
      return;
    }
    this.body.replaceChildren(...ergebnisTeile(erg));
  }
}
