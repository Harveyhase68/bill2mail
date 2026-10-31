export type FrameType = "rot" | "blau" | "gruen" | "violett" | "grau";

/** Rahmen in PDF-Punkten (1/72 Zoll), Ursprung links oben auf der Seite. */
export interface FrameDef {
  type: FrameType;
  page: number;
  x: number;
  y: number;
  w: number;
  h: number;
  /** Parametertext, Parameter mit ";" getrennt. Keine Laengenbeschraenkung. */
  params: string;
}

export interface FrameMeta {
  label: string;
  color: string;
  optional: boolean;
  hint: string;
  placeholder: string;
}

export const FRAME_ORDER: FrameType[] = ["rot", "blau", "gruen", "violett", "grau"];

export const FRAME_META: Record<FrameType, FrameMeta> = {
  rot: {
    label: "Formulartyp",
    color: "#8b1a1a",
    optional: false,
    hint: "Rahmen über die Überschrift legen. Erlaubte Texte mit ; trennen. Mit *[RE] wird die Rechnungsnummer aus derselben Zeile gelesen.",
    placeholder: "RECHNUNG;LIEFERSCHEIN   oder   RECHNUNG*[RE]",
  },
  blau: {
    label: "Kundennummer",
    color: "#1a3a8b",
    optional: false,
    hint: "Text vor der Kundennummer, danach *[KU] für die Nummer selbst.",
    placeholder: "Kundennummer*[KU]",
  },
  gruen: {
    label: "Rechnungsnummer",
    color: "#1a6b2a",
    optional: false,
    hint: "Text vor der Rechnungsnummer, danach *[RE]. Entfällt, wenn der rote Rahmen schon *[RE] enthält.",
    placeholder: "RE*[RE]",
  },
  violett: {
    label: "Seitennummer",
    color: "#5b1a8b",
    optional: true,
    hint: "Zum Erkennen zusammenhängender Seiten. Weglassen, wenn nicht jede Seite eine Seitennummer hat.",
    placeholder: "Seite*[SE]",
  },
  grau: {
    label: "Kundenname",
    color: "#555555",
    optional: true,
    hint: "Nur setzen, wenn der Name eindeutig ist. Sonst weglassen, der Name kommt aus der Kundenliste.",
    placeholder: "*[NA]",
  },
};

export interface SeitenProbe {
  seite: number;
  texte: Partial<Record<FrameType, string>>;
  treffer: Partial<Record<FrameType, boolean>>;
  versatz: Partial<Record<FrameType, number>>;
}

export interface Rechnung {
  nr: number;
  kundennummer: string;
  rechnungsnummer: string;
  formulartyp: string;
  kundenname_pdf: string;
  /** 0-basiert, einschliesslich */
  start_page: number;
  end_page: number;
  warnungen: string[];
  in_liste: boolean;
  kundenname: string;
  email: string;
}

export interface Analyse {
  seiten: number;
  rechnungen: Rechnung[];
  proben: SeitenProbe[];
  warnungen: string[];
}

export interface Einstellungen {
  smtp_host: string;
  smtp_port: number;
  sicherheit: "keine" | "starttls" | "ssl";
  smtp_user: string;
  anmeldung: "passwort" | "google" | "microsoft";
  absender: string;
  absender_name: string;
  betreff: string;
  text: string;
  test_empfaenger: string;
  ueberwachung_aktiv: boolean;
  ueberwachung_ordner: string;
  toleranz_pt: number;
}

export interface OauthStatus {
  google: { konfiguriert: boolean; angemeldet: boolean };
  microsoft: { konfiguriert: boolean; angemeldet: boolean };
}

export interface UeberwachungsStatus {
  aktiv: boolean;
  meldung: string;
}

export interface LizenzStatus {
  lizenziert: boolean;
  kunde: string;
  bis: string | null;
  meldung: string;
}

export interface NeuePdf {
  pfad: string;
  fehler: string | null;
}

export interface GeplanteMail {
  kundennummer: string;
  kundenname: string;
  email: string;
  rechnungen: number[];
  seiten: number;
  betreff: string;
  senden: boolean;
  grund: string;
}

export interface Vorschau {
  analyse: Analyse;
  mails: GeplanteMail[];
  betreff: string;
  text_html: string;
  absender: string;
  bereits_gesendet: string | null;
}

export interface ProtokollZeile {
  nr: number;
  kundennummer: string;
  kundenname: string;
  rechnungsnummer: string;
  seiten: string;
  email: string;
  status: "gesendet" | "uebersprungen" | "fehler";
  fehler: string;
  datei: string;
}

export interface VersandErgebnis {
  zeilen: ProtokollZeile[];
  mails_gesendet: number;
  mails_fehler: number;
  rechnungen_uebersprungen: number;
  abgebrochen: boolean;
  protokoll: string;
  ordner: string;
}

export interface Kunde {
  id: number;
  kundennummer: string;
  kundenname: string;
  email: string;
}
