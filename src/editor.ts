import { invoke } from "@tauri-apps/api/core";
import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentLoadingTask, PDFDocumentProxy, PDFPageProxy, RenderTask } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { FRAME_META, FRAME_ORDER, type FrameDef, type FrameType } from "./types";
import { t } from "./i18n";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

const MIN_SIZE = 8; // Punkte
const MIN_ZOOM = 0.4;
const MAX_ZOOM = 4;
const HANDLES = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];

interface PageView {
  page: PDFPageProxy;
  wrap: HTMLDivElement;
  canvas: HTMLCanvasElement;
  overlay: HTMLDivElement;
  w: number; // Punkte
  h: number;
  renderedKey: string;
  task?: RenderTask;
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

export class Editor {
  private loading: PDFDocumentLoadingTask | null = null;
  private pages: PageView[] = [];
  private frames = new Map<FrameType, FrameDef>();
  private frameEls = new Map<FrameType, HTMLDivElement>();
  private zoom = 1.25;
  private armed: FrameType | null = null;
  private selected: FrameType | null = null;
  private renderQueued = false;

  private scroller = $<HTMLDivElement>("scroller");
  private pagesEl = $<HTMLDivElement>("pages");
  private palette = $<HTMLDivElement>("palette");

  private dlg = $<HTMLDivElement>("dlg");
  private dlgText = $<HTMLTextAreaElement>("dlg-text");
  private dlgType: FrameType | null = null;
  private dlgIsNew = false;

  constructor(private onChange: (frames: FrameDef[]) => void, private onWarning: (msg: string | null) => void) {
    this.scroller.addEventListener("scroll", () => this.queueRender());
    window.addEventListener("resize", () => this.queueRender());
    this.scroller.addEventListener(
      "wheel",
      (e) => {
        if (!e.ctrlKey) return;
        e.preventDefault();
        this.setZoom(this.zoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1));
      },
      { passive: false },
    );
    window.addEventListener("keydown", (e) => this.onKey(e));

    $("dlg-ok").addEventListener("click", () => this.dialogOk());
    $("dlg-cancel").addEventListener("click", () => this.dialogCancel());
    $("dlg-delete").addEventListener("click", () => {
      if (this.dlgType) this.removeFrame(this.dlgType);
      this.closeDialog();
    });
    this.dlgText.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        this.dialogOk();
      } else if (e.key === "Escape") {
        e.preventDefault();
        this.dialogCancel();
      }
    });

    this.renderPalette();
  }

  // ---------------------------------------------------------------- Vorlage

  setFrames(frames: FrameDef[]) {
    for (const el of this.frameEls.values()) el.remove();
    this.frameEls.clear();
    this.frames.clear();
    for (const f of frames) if (FRAME_META[f.type]) this.frames.set(f.type, f);
    this.mountFrames();
    this.renderPalette();
  }

  getFrames(): FrameDef[] {
    return FRAME_ORDER.flatMap((t) => (this.frames.has(t) ? [this.frames.get(t)!] : []));
  }

  // -------------------------------------------------------------------- PDF

  async open(path: string) {
    const bytes: ArrayBuffer = await invoke("pdf_lesen", { path });
    const loading = pdfjs.getDocument({ data: new Uint8Array(bytes) });
    const doc = await loading.promise;

    this.cancelRenders();
    void this.loading?.destroy();
    this.loading = loading;
    this.pages = [];
    this.pagesEl.replaceChildren();
    for (const el of this.frameEls.values()) el.remove();
    this.frameEls.clear();

    for (let i = 1; i <= doc.numPages; i++) {
      const page = await doc.getPage(i);
      const vp = page.getViewport({ scale: 1 });
      const wrap = document.createElement("div");
      wrap.className = "page";
      const canvas = document.createElement("canvas");
      const overlay = document.createElement("div");
      overlay.className = "overlay";
      const pv: PageView = { page, wrap, canvas, overlay, w: vp.width, h: vp.height, renderedKey: "" };
      this.bindOverlay(pv, i - 1);
      wrap.append(canvas, overlay);
      this.pagesEl.append(wrap);
      this.pages.push(pv);
    }

    $("empty").hidden = true;
    this.fitWidth();
    this.mountFrames();
    this.scroller.scrollTop = 0;
    void this.checkForText(doc);
  }

  scrollToPage(index: number) {
    this.pages[index]?.wrap.scrollIntoView({ block: "start", behavior: "smooth" });
  }

  /** Nach dem Wiedereinblenden der Ansicht sichtbare Seiten neu zeichnen. */
  refresh() {
    this.queueRender();
  }

  get pageCount() {
    return this.pages.length;
  }

  /** Scans erkennt man daran, dass auf den ersten Seiten kein Text steht. */
  private async checkForText(doc: PDFDocumentProxy) {
    let chars = 0;
    for (let i = 1; i <= Math.min(3, doc.numPages); i++) {
      const tc = await (await doc.getPage(i)).getTextContent();
      chars += tc.items.reduce((n, it) => n + ("str" in it ? it.str.trim().length : 0), 0);
    }
    this.onWarning(
      chars === 0
        ? t("Dieses PDF enthält keinen auswertbaren Text. Vermutlich handelt es sich um eingescannte Rechnungen. Diese werden in dieser Version nicht unterstützt (OCR folgt später).")
        : null,
    );
  }

  // ------------------------------------------------------------------- Zoom

  getZoom() {
    return this.zoom;
  }

  setZoom(z: number) {
    const next = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));
    if (next === this.zoom) return;
    const ratio = this.scroller.scrollHeight > 0 ? this.scroller.scrollTop / this.scroller.scrollHeight : 0;
    this.zoom = next;
    this.layoutPages();
    this.scroller.scrollTop = ratio * this.scroller.scrollHeight;
    this.queueRender();
  }

  fitWidth() {
    if (this.pages.length === 0) return;
    const maxW = Math.max(...this.pages.map((p) => p.w));
    const avail = this.scroller.clientWidth - 48 - 20; // Rand + vertikale Scrollleiste
    this.zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, avail / maxW));
    this.layoutPages();
    this.queueRender();
  }

  private layoutPages() {
    for (const pv of this.pages) {
      pv.wrap.style.width = `${pv.w * this.zoom}px`;
      pv.wrap.style.height = `${pv.h * this.zoom}px`;
    }
    for (const [t, f] of this.frames) {
      const el = this.frameEls.get(t);
      if (el) this.place(el, f);
    }
    $("zoom-label").textContent = `${Math.round(this.zoom * 100)} %`;
  }

  // -------------------------------------------------------------- Rendering

  private queueRender() {
    if (this.renderQueued) return;
    this.renderQueued = true;
    requestAnimationFrame(() => {
      this.renderQueued = false;
      this.renderVisible();
    });
  }

  private renderVisible() {
    const view = this.scroller.getBoundingClientRect();
    const margin = 600;
    for (const pv of this.pages) {
      const r = pv.wrap.getBoundingClientRect();
      const visible = r.bottom > view.top - margin && r.top < view.bottom + margin;
      if (visible) void this.renderPage(pv);
      else if (pv.renderedKey) {
        // weit entfernte Seiten freigeben, spart Speicher bei langen PDFs
        pv.task?.cancel();
        pv.canvas.width = 0;
        pv.canvas.height = 0;
        pv.renderedKey = "";
      }
    }
  }

  private async renderPage(pv: PageView) {
    const dpr = window.devicePixelRatio || 1;
    const key = `${this.zoom.toFixed(3)}@${dpr}`;
    if (pv.renderedKey === key) return;
    pv.task?.cancel();
    pv.renderedKey = key;

    const viewport = pv.page.getViewport({ scale: this.zoom * dpr });
    pv.canvas.width = Math.floor(viewport.width);
    pv.canvas.height = Math.floor(viewport.height);
    pv.canvas.style.width = `${pv.w * this.zoom}px`;
    pv.canvas.style.height = `${pv.h * this.zoom}px`;

    const task = pv.page.render({ canvas: pv.canvas, viewport });
    pv.task = task;
    try {
      await task.promise;
    } catch {
      // abgebrochen (Zoom geaendert oder Seite freigegeben)
      if (pv.task === task && pv.renderedKey === key) pv.renderedKey = "";
    }
  }

  private cancelRenders() {
    for (const pv of this.pages) pv.task?.cancel();
  }

  // ----------------------------------------------------------------- Rahmen

  private mountFrames() {
    if (this.pages.length === 0) return;
    for (const [t, f] of this.frames) {
      if (this.frameEls.has(t)) continue;
      const pageIdx = Math.min(f.page, this.pages.length - 1);
      this.createFrameEl(f, this.pages[pageIdx]);
    }
  }

  private place(el: HTMLElement, f: { x: number; y: number; w: number; h: number }) {
    const z = this.zoom;
    el.style.left = `${f.x * z}px`;
    el.style.top = `${f.y * z}px`;
    el.style.width = `${f.w * z}px`;
    el.style.height = `${f.h * z}px`;
  }

  private createFrameEl(f: FrameDef, pv: PageView) {
    const meta = FRAME_META[f.type];
    const el = document.createElement("div");
    el.className = "frame";
    el.style.setProperty("--c", meta.color);
    const tag = document.createElement("span");
    tag.className = "frame-tag";
    tag.textContent = t(meta.label);
    el.append(tag);
    for (const h of HANDLES) {
      const hd = document.createElement("span");
      hd.className = `handle h-${h}`;
      hd.dataset.h = h;
      el.append(hd);
    }
    el.addEventListener("pointerdown", (e) => this.startDrag(e, f, el, pv));
    el.addEventListener("dblclick", () => this.openDialog(f.type, false));
    this.place(el, f);
    pv.overlay.append(el);
    this.frameEls.set(f.type, el);
    return el;
  }

  private startDrag(e: PointerEvent, f: FrameDef, el: HTMLDivElement, pv: PageView) {
    if (e.button !== 0) return;
    e.stopPropagation();
    this.select(f.type);
    const handle = (e.target as HTMLElement).dataset.h ?? "";
    const z = this.zoom;
    const o = { x: f.x, y: f.y, w: f.w, h: f.h };
    const sx = e.clientX;
    const sy = e.clientY;
    el.setPointerCapture(e.pointerId);
    el.classList.add("dragging");

    const move = (ev: PointerEvent) => {
      const dx = (ev.clientX - sx) / z;
      const dy = (ev.clientY - sy) / z;
      let left = o.x;
      let top = o.y;
      let right = o.x + o.w;
      let bottom = o.y + o.h;
      if (!handle) {
        const nx = Math.min(Math.max(0, o.x + dx), pv.w - o.w);
        const ny = Math.min(Math.max(0, o.y + dy), pv.h - o.h);
        left = nx;
        top = ny;
        right = nx + o.w;
        bottom = ny + o.h;
      } else {
        if (handle.includes("w")) left = Math.min(Math.max(0, o.x + dx), right - MIN_SIZE);
        if (handle.includes("e")) right = Math.max(Math.min(pv.w, right + dx), left + MIN_SIZE);
        if (handle.includes("n")) top = Math.min(Math.max(0, o.y + dy), bottom - MIN_SIZE);
        if (handle.includes("s")) bottom = Math.max(Math.min(pv.h, bottom + dy), top + MIN_SIZE);
      }
      f.x = left;
      f.y = top;
      f.w = right - left;
      f.h = bottom - top;
      this.place(el, f);
    };
    const up = () => {
      el.classList.remove("dragging");
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      if (f.x !== o.x || f.y !== o.y || f.w !== o.w || f.h !== o.h) this.changed();
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  /** Neuen Rahmen mit der Maus aufziehen, solange ein Rahmentyp gewaehlt ist. */
  private bindOverlay(pv: PageView, pageIdx: number) {
    pv.overlay.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || e.target !== pv.overlay) return;
      if (!this.armed) {
        this.select(null);
        return;
      }
      const type = this.armed;
      const z = this.zoom;
      const box = pv.overlay.getBoundingClientRect();
      const x0 = (e.clientX - box.left) / z;
      const y0 = (e.clientY - box.top) / z;
      const draft = document.createElement("div");
      draft.className = "frame draft";
      draft.style.setProperty("--c", FRAME_META[type].color);
      pv.overlay.append(draft);
      pv.overlay.setPointerCapture(e.pointerId);

      const rect = (ev: PointerEvent) => {
        const x1 = Math.min(Math.max(0, (ev.clientX - box.left) / z), pv.w);
        const y1 = Math.min(Math.max(0, (ev.clientY - box.top) / z), pv.h);
        return { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) };
      };
      const move = (ev: PointerEvent) => this.place(draft, rect(ev));
      const up = (ev: PointerEvent) => {
        pv.overlay.removeEventListener("pointermove", move);
        pv.overlay.removeEventListener("pointerup", up);
        pv.overlay.removeEventListener("pointercancel", up);
        draft.remove();
        const r = rect(ev);
        if (ev.type === "pointercancel" || r.w < MIN_SIZE || r.h < MIN_SIZE) return;
        this.setArmed(null);
        const f: FrameDef = { type, page: pageIdx, ...r, params: "" };
        this.frames.set(type, f);
        this.createFrameEl(f, pv);
        this.select(type);
        this.openDialog(type, true);
      };
      pv.overlay.addEventListener("pointermove", move);
      pv.overlay.addEventListener("pointerup", up);
      pv.overlay.addEventListener("pointercancel", up);
    });
  }

  private removeFrame(type: FrameType) {
    this.frameEls.get(type)?.remove();
    this.frameEls.delete(type);
    this.frames.delete(type);
    if (this.selected === type) this.selected = null;
    this.changed();
  }

  private select(type: FrameType | null) {
    this.selected = type;
    for (const [t, el] of this.frameEls) el.classList.toggle("selected", t === type);
    this.renderPalette();
  }

  private setArmed(type: FrameType | null) {
    this.armed = type;
    this.pagesEl.classList.toggle("armed", type !== null);
    this.renderPalette();
  }

  private changed() {
    this.renderPalette();
    this.onChange(this.getFrames());
  }

  private onKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    if (e.key === "Escape" && this.armed) this.setArmed(null);
    if ((e.key === "Delete" || e.key === "Backspace") && this.selected && !typing && this.dlg.hidden) {
      this.removeFrame(this.selected);
    }
  }

  // ---------------------------------------------------------------- Palette

  private renderPalette() {
    this.palette.replaceChildren();
    for (const type of FRAME_ORDER) {
      const meta = FRAME_META[type];
      const f = this.frames.get(type);
      const btn = document.createElement("button");
      btn.className = "tool";
      btn.classList.toggle("armed", this.armed === type);
      btn.classList.toggle("selected", this.selected === type);
      btn.style.setProperty("--c", meta.color);
      const params = f?.params.trim();
      btn.innerHTML = `<span class="swatch"></span><span class="tool-text"><b></b><small></small></span>`;
      btn.querySelector("b")!.textContent = t(meta.label) + (meta.optional ? " " + t("(optional)") : "");
      btn.querySelector("small")!.textContent = f
        ? params || t("Parameter fehlt – Doppelklick auf den Rahmen")
        : this.armed === type
          ? t("Rahmen auf der Seite aufziehen …")
          : t("Nicht gesetzt");
      btn.addEventListener("click", () => {
        if (this.pages.length === 0) return;
        if (f) {
          this.setArmed(null);
          this.select(type);
          this.scrollToFrame(type);
        } else {
          this.setArmed(this.armed === type ? null : type);
        }
      });
      this.palette.append(btn);
    }
  }

  private scrollToFrame(type: FrameType) {
    this.frameEls.get(type)?.scrollIntoView({ block: "center", behavior: "smooth" });
  }

  // ----------------------------------------------------------------- Dialog

  private openDialog(type: FrameType, isNew: boolean) {
    const meta = FRAME_META[type];
    const f = this.frames.get(type);
    const el = this.frameEls.get(type);
    if (!f || !el) return;
    this.dlgType = type;
    this.dlgIsNew = isNew;
    this.dlg.style.setProperty("--c", meta.color);
    $("dlg-title").textContent = `${t(meta.label)}${meta.optional ? " " + t("(optional)") : ""}`;
    $("dlg-hint").textContent = t(meta.hint);
    this.dlgText.placeholder = t(meta.placeholder);
    this.dlgText.value = f.params;
    this.dlg.hidden = false;

    const r = el.getBoundingClientRect();
    const dw = this.dlg.offsetWidth;
    const dh = this.dlg.offsetHeight;
    let left = Math.min(Math.max(8, r.left), window.innerWidth - dw - 8);
    let top = r.bottom + 10;
    if (top + dh > window.innerHeight - 8) top = Math.max(8, r.top - dh - 10);
    this.dlg.style.left = `${left}px`;
    this.dlg.style.top = `${top}px`;
    this.dlgText.focus();
    this.dlgText.select();
  }

  private dialogOk() {
    if (!this.dlgType) return;
    const f = this.frames.get(this.dlgType);
    if (f) f.params = this.dlgText.value.trim();
    this.closeDialog();
    this.changed();
  }

  private dialogCancel() {
    // Ein gerade erst aufgezogener Rahmen ohne Parameter wird wieder verworfen.
    if (this.dlgType && this.dlgIsNew) this.removeFrame(this.dlgType);
    this.closeDialog();
  }

  private closeDialog() {
    this.dlg.hidden = true;
    this.dlgType = null;
  }
}
