import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "./i18n";
import type { FrameDef, NeuePdf, VersandErgebnis, Vorschau } from "./types";
import { el, ergebnisTeile, versandSperre, vorschauTeile } from "./ui";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

type Phase = "laedt" | "bereit" | "fehler" | "sendet" | "fertig";

/**
 * Fenster der Ordnerüberwachung: zeigt die erkannten Kunden- und Rechnungsnummern einer neu
 * eingetroffenen PDF und sendet erst nach Klick auf „Versenden“. Mehrere PDFs werden nacheinander
 * abgearbeitet.
 */
export class AutoFenster {
  private queue: NeuePdf[] = [];
  private aktuell: NeuePdf | null = null;
  private phase: Phase = "laedt";
  private zuSenden = 0;

  constructor() {
    $("auto-abbrechen").addEventListener("click", () => this.abbrechenKlick());
    $("auto-senden").addEventListener("click", () => void this.senden());
    // Esc darf nie versehentlich etwas senden; es schließt höchstens (nur wenn nicht gesendet wird).
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && !$("auto").hidden && this.phase !== "sendet") this.abbrechenKlick();
    });
  }

  async start() {
    await listen<NeuePdf>("neue-pdf", (e) => this.eingang(e.payload));
  }

  private eingang(p: NeuePdf) {
    if (this.aktuell?.pfad === p.pfad || this.queue.some((q) => q.pfad === p.pfad)) return;
    this.queue.push(p);
    if (!this.aktuell) void this.naechste();
    else this.zeigeQueue();
  }

  private zeigeQueue() {
    $("auto-queue").textContent = this.queue.length > 0 ? t("+ {n} weitere PDF warten", { n: this.queue.length }) : "";
  }

  private buttons() {
    const nochSendend = this.phase === "sendet";
    $<HTMLButtonElement>("auto-senden").hidden = this.phase === "fertig" || this.phase === "fehler";
    $<HTMLButtonElement>("auto-senden").disabled = this.phase !== "bereit" || this.zuSenden === 0;
    $<HTMLButtonElement>("auto-abbrechen").textContent =
      this.phase === "fertig" || this.phase === "fehler" ? t("Schließen") : nochSendend ? t("Versand abbrechen") : t("Abbrechen");
  }

  private hinweis(t: string) {
    $("auto-hinweis").textContent = t;
  }

  private async naechste() {
    const p = this.queue.shift() ?? null;
    this.aktuell = p;
    this.zeigeQueue();
    if (!p) {
      $("auto").hidden = true;
      return;
    }
    $("auto").hidden = false;
    $("auto-file").textContent = p.pfad;
    this.phase = "laedt";
    this.zuSenden = 0;
    this.hinweis("");
    this.buttons();

    if (p.fehler) {
      this.zeigeFehler(p.fehler);
      return;
    }
    $("auto-body").replaceChildren(el("p", "hint", t("Analysiere PDF …")));
    try {
      const json = await invoke<string | null>("vorlage_laden");
      const frames: FrameDef[] = json ? (JSON.parse(json).frames ?? []) : [];
      if (frames.length === 0) {
        this.zeigeFehler(t("Es gibt noch keine Vorlage. Bitte zuerst im Reiter „Vorlage & PDF“ die Rahmen setzen."));
        return;
      }
      const v = await invoke<Vorschau>("versand_vorschau", { path: p.pfad, frames });
      if (this.aktuell !== p) return;
      this.zuSenden = v.mails.filter((m) => m.senden).length;
      this.vorlageFrames = frames;
      this.phase = "bereit";
      $("auto-body").replaceChildren(...vorschauTeile(v, false));
      this.hinweis(
        this.zuSenden === 0
          ? t("Es gibt nichts zu senden.")
          : t("{n} E-Mails werden mit „Versenden“ sofort an die Kunden gesendet.", { n: this.zuSenden }),
      );
      this.buttons();
    } catch (e) {
      this.zeigeFehler(String(e));
    }
  }

  private vorlageFrames: FrameDef[] = [];

  private zeigeFehler(msg: string) {
    this.phase = "fehler";
    $("auto-body").replaceChildren(el("p", "error-text", msg));
    this.hinweis(t("Es wurde nichts gesendet."));
    this.buttons();
  }

  private abbrechenKlick() {
    if (this.phase === "sendet") {
      void invoke("versand_abbrechen");
      $<HTMLButtonElement>("auto-abbrechen").disabled = true;
      this.hinweis(t("Abbruch angefordert – die laufende Mail wird noch beendet …"));
      return;
    }
    $<HTMLButtonElement>("auto-abbrechen").disabled = false;
    void this.naechste();
  }

  private async senden() {
    const p = this.aktuell;
    if (!p || this.phase !== "bereit") return;
    if (versandSperre.aktiv) {
      this.hinweis(t("Es läuft bereits ein anderer Versand – bitte warten."));
      return;
    }
    versandSperre.aktiv = true;
    this.phase = "sendet";
    this.buttons();
    const bar = el("progress");
    bar.max = this.zuSenden;
    bar.value = 0;
    const label = el("span", "", t("Bereite vor …"));
    const box = el("div", "v-progress");
    box.append(bar, label);
    $("auto-body").replaceChildren(box);
    this.hinweis("");
    const unlisten = await listen<{ aktuell: number; gesamt: number; text: string }>("versand-fortschritt", (e) => {
      bar.max = e.payload.gesamt;
      bar.value = e.payload.aktuell - 1;
      label.textContent = t("{a} von {g}: {t}", { a: e.payload.aktuell, g: e.payload.gesamt, t: e.payload.text });
    });
    try {
      const erg = await invoke<VersandErgebnis>("versenden", { path: p.pfad, frames: this.vorlageFrames });
      this.phase = "fertig";
      $("auto-body").replaceChildren(...ergebnisTeile(erg));
      this.hinweis("");
    } catch (e) {
      this.zeigeFehler(t("Versand nicht durchgeführt: {e}", { e: String(e) }));
    } finally {
      unlisten();
      versandSperre.aktiv = false;
      this.buttons();
    }
  }
}
