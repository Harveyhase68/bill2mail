import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { AnalysePanel } from "./analyse";
import { AutoFenster } from "./auto";
import { Editor } from "./editor";
import { EinstellungenView } from "./einstellungen";
import { KundenView } from "./kunden";
import { spracheStarten, t } from "./i18n";
import { VersandView } from "./versand";
import type { FrameDef } from "./types";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

function start() {
// ------------------------------------------------------------------ Vorlage

let saveTimer = 0;
function saveVorlage(frames: FrameDef[]) {
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    invoke("vorlage_speichern", { json: JSON.stringify({ version: 1, frames }) }).catch((e) =>
      console.error("Vorlage konnte nicht gespeichert werden:", e),
    );
  }, 300);
}

const editor = new Editor(saveVorlage, (msg) => {
  const w = $("warning");
  w.textContent = msg ?? "";
  w.hidden = msg === null;
});

async function loadVorlage() {
  const json = await invoke<string | null>("vorlage_laden");
  if (!json) return;
  try {
    editor.setFrames((JSON.parse(json).frames ?? []) as FrameDef[]);
  } catch (e) {
    console.error("Gespeicherte Vorlage ist ungültig:", e);
  }
}

// ---------------------------------------------------------------- Toolbar

let currentPdf: string | null = null;
const analyse = new AnalysePanel((i) => editor.scrollToPage(i));

async function openPdf() {
  const picked = await open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
  if (typeof picked !== "string") return;
  try {
    await editor.open(picked);
    currentPdf = picked;
    $("file-name").textContent = t("{name} – {n} Seiten", { name: picked.split(/[\\/]/).pop() ?? "", n: editor.pageCount });
    $<HTMLButtonElement>("btn-analyse").disabled = false;
    $("analyse").hidden = true;
  } catch (e) {
    $("file-name").textContent = t("Fehler: {e}", { e: String(e) });
  }
}

$("btn-open").addEventListener("click", () => void openPdf());
$("btn-analyse").addEventListener("click", () => {
  if (currentPdf) void analyse.run(currentPdf, editor.getFrames());
});
$("btn-zoom-in").addEventListener("click", () => editor.setZoom(editor.getZoom() * 1.2));
$("btn-zoom-out").addEventListener("click", () => editor.setZoom(editor.getZoom() / 1.2));
$("btn-fit").addEventListener("click", () => editor.fitWidth());

// --------------------------------------------------------------- Ansichten

const kunden = new KundenView();
const einstellungen = new EinstellungenView();
new VersandView(() => editor.getFrames());
void new AutoFenster().start();

for (const tab of document.querySelectorAll<HTMLButtonElement>(".tab")) {
  tab.addEventListener("click", () => {
    const view = tab.dataset.view!;
    document.querySelectorAll(".tab").forEach((t) => t.classList.toggle("active", t === tab));
    for (const v of ["editor", "versand", "kunden", "einstellungen"]) $(`view-${v}`).hidden = v !== view;
    if (view === "kunden") void kunden.reload();
    else if (view === "einstellungen") void einstellungen.load();
    else if (view === "editor") editor.refresh();
  });
}

void loadVorlage();
void EinstellungenView.lizenzAnzeigen().catch(() => {});
}

void spracheStarten().then(start);
