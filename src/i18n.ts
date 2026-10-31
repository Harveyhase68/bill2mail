import { invoke } from "@tauri-apps/api/core";

/**
 * Zweisprachigkeit (Deutsch ist die Quellsprache, die deutschen Texte sind zugleich die Schlüssel).
 * Beim Start gilt immer die Windows-Sprache (alles außer Deutsch -> Englisch). Die Umschaltung in
 * den Einstellungen gilt nur bis zum Beenden des Programms (sessionStorage + Neuladen).
 */
export type Sprache = "de" | "en";

const SPEICHER = "bill2mail-sprache";
let aktuell: Sprache = "de";

const EN: Record<string, string> = {
  // ---- Reiter, Kopfzeile
  "Vorlage & PDF": "Template & PDF",
  Versand: "Sending",
  Kunden: "Customers",
  Einstellungen: "Settings",

  // ---- Editor
  "PDF öffnen …": "Open PDF …",
  "Kein PDF geöffnet": "No PDF open",
  "Rahmen auf das geöffnete PDF anwenden und Rechnungen erkennen": "Apply the frames to the open PDF and detect invoices",
  Analysieren: "Analyze",
  "Verkleinern (Strg + Mausrad)": "Zoom out (Ctrl + mouse wheel)",
  "Vergrößern (Strg + Mausrad)": "Zoom in (Ctrl + mouse wheel)",
  "An Fensterbreite anpassen": "Fit to window width",
  Breite: "Width",
  "Anker-Rahmen": "Anchor frames",
  "Rahmentyp wählen, dann auf der Seite einen Rahmen aufziehen. Rahmen lassen sich mit der Maus verschieben und an den Rändern vergrößern. Doppelklick öffnet die Parameter.":
    "Choose a frame type, then drag a frame on the page. Frames can be moved with the mouse and resized at the edges. Double-click opens the parameters.",
  "Nur gedruckte (textbasierte) PDFs. Eingescannte Rechnungen werden in dieser Version nicht unterstützt.":
    "Printed (text-based) PDFs only. Scanned invoices are not supported in this version.",
  "Öffnen Sie ein PDF mit gedruckten Rechnungen, um die Vorlage festzulegen.": "Open a PDF with printed invoices to define the template.",
  Schließen: "Close",
  "Dieses PDF enthält keinen auswertbaren Text. Vermutlich handelt es sich um eingescannte Rechnungen. Diese werden in dieser Version nicht unterstützt (OCR folgt später).":
    "This PDF contains no analyzable text. It probably consists of scanned invoices, which are not supported in this version (OCR will follow later).",
  "(optional)": "(optional)",
  "Parameter fehlt – Doppelklick auf den Rahmen": "Parameter missing – double-click the frame",
  "Rahmen auf der Seite aufziehen …": "Drag a frame on the page …",
  "Nicht gesetzt": "Not set",
  "Rahmen löschen": "Delete frame",
  Abbrechen: "Cancel",
  Übernehmen: "Apply",
  Formulartyp: "Form type",
  Kundennummer: "Customer number",
  Rechnungsnummer: "Invoice number",
  Seitennummer: "Page number",
  Kundenname: "Customer name",
  "Rahmen über die Überschrift legen. Erlaubte Texte mit ; trennen. Mit *[RE] wird die Rechnungsnummer aus derselben Zeile gelesen.":
    "Place the frame over the heading. Separate allowed texts with ;. With *[RE] the invoice number is read from the same line.",
  "RECHNUNG;LIEFERSCHEIN   oder   RECHNUNG*[RE]": "INVOICE;DELIVERY NOTE   or   INVOICE*[RE]",
  "Text vor der Kundennummer, danach *[KU] für die Nummer selbst.": "Text before the customer number, then *[KU] for the number itself.",
  "Kundennummer*[KU]": "Customer number*[KU]",
  "Text vor der Rechnungsnummer, danach *[RE]. Entfällt, wenn der rote Rahmen schon *[RE] enthält.":
    "Text before the invoice number, then *[RE]. Not needed if the red frame already contains *[RE].",
  "RE*[RE]": "INV*[RE]",
  "Zum Erkennen zusammenhängender Seiten. Weglassen, wenn nicht jede Seite eine Seitennummer hat.":
    "Used to recognize pages that belong together. Leave out if not every page has a page number.",
  "Seite*[SE]": "Page*[SE]",
  "Nur setzen, wenn der Name eindeutig ist. Sonst weglassen, der Name kommt aus der Kundenliste.":
    "Only set if the name is unambiguous. Otherwise leave out; the name comes from the customer list.",

  // ---- Analyse
  "Analysiere …": "Analyzing …",
  "Analysiere PDF …": "Analyzing PDF …",
  "Analyse nicht möglich": "Analysis not possible",
  "{n} Rechnungen aus {s} Seiten erkannt": "{n} invoices detected in {s} pages",
  " – {m} mit Hinweisen": " – {m} with notes",
  "Zeile anklicken: zeigt die gelesenen Rahmentexte der ersten Seite.": "Click a row to show the frame texts read from its first page.",
  "Nr.": "No.",
  "Kundennr.": "Cust. no.",
  Kunde: "Customer",
  "Rechnungsnr.": "Invoice no.",
  Typ: "Type",
  Seiten: "Pages",
  "E-Mail": "Email",
  Hinweise: "Notes",
  "Seite {n}: ": "Page {n}: ",
  " ({v} pt verschoben)": " (shifted {v} pt)",

  // ---- Versand
  "PDF wählen …": "Choose PDF …",
  "Kein PDF gewählt": "No PDF chosen",
  "Zeigt, was versendet würde. Es wird nichts gesendet.": "Shows what would be sent. Nothing is sent.",
  "1. Vorschau": "1. Preview",
  "Schickt eine Test-Mail an die Test-Adresse aus den Einstellungen": "Sends a test email to the test address from the settings",
  "2. Test-Mail senden": "2. Send test email",
  "3. Versenden": "3. Send",
  "Rechnungs-PDF wählen, dann „Vorschau“ drücken. Die Rahmen-Vorlage stammt aus dem Tab „Vorlage & PDF“.":
    "Choose an invoice PDF, then press “Preview”. The frame template comes from the “Template & PDF” tab.",
  "PDF gewählt. Jetzt „Vorschau“ drücken.": "PDF chosen. Now press “Preview”.",
  "Abbruch angefordert – die laufende Mail wird noch beendet …": "Abort requested – the current email is still being finished …",
  "Sende Test-Mail …": "Sending test email …",
  "Es läuft bereits ein anderer Versand.": "Another sending is already in progress.",
  "\n\nACHTUNG: Dieses PDF wurde schon einmal versendet (siehe Vorschau).": "\n\nWARNING: This PDF has already been sent once (see preview).",
  "{n} E-Mails jetzt an die Kunden senden?\n\nDas lässt sich nicht rückgängig machen. Haben Sie die Test-Mail geprüft?{w}":
    "Send {n} emails to the customers now?\n\nThis cannot be undone. Have you checked the test email?{w}",
  Versenden: "Send",
  "Jetzt senden": "Send now",
  "Bereite vor …": "Preparing …",
  "{a} von {g}: {t}": "{a} of {g}: {t}",
  "Versand nicht durchgeführt. Es wurde nichts gesendet.": "Sending not carried out. Nothing was sent.",

  // ---- gemeinsame Tabellen (ui.ts)
  Gesendet: "Sent",
  Übersprungen: "Skipped",
  Fehler: "Error",
  "⚠ {m} Nochmal senden erzeugt doppelte E-Mails.": "⚠ {m} Sending again creates duplicate emails.",
  "{r} Rechnungen aus {s} Seiten · {n} E-Mails werden gesendet": "{r} invoices from {s} pages · {n} emails will be sent",
  " · {k} Kunden werden übersprungen": " · {k} customers will be skipped",
  Rechnungen: "Invoices",
  Grund: "Reason",
  senden: "send",
  überspringen: "skip",
  "So sieht die E-Mail aus – Betreff: {b}": "This is what the email looks like – Subject: {b}",
  "Von: {a}": "From: {a}",
  "(kein Absender eingetragen)": "(no sender entered)",
  "{n} E-Mails gesendet": "{n} emails sent",
  " · {n} mit Fehler": " · {n} with errors",
  " · {n} Rechnungen übersprungen": " · {n} invoices skipped",
  " · abgebrochen": " · aborted",
  "Protokoll (XLSX) im Explorer zeigen": "Show log (XLSX) in Explorer",
  "Ordner mit den Einzel-PDFs zeigen": "Show folder with the individual PDFs",
  Status: "Status",
  Hinweis: "Note",

  // ---- Kunden
  "Suchen …": "Search …",
  "(mehrere mit ; trennen)": "(separate several with ;)",
  Nummer: "Number",
  "Name (für die Anrede im Email)": "Name (for the greeting in the email)",
  Hinzufügen: "Add",
  Löschen: "Delete",
  "Kunde angelegt.": "Customer added.",
  Gespeichert: "Saved",
  "Gespeichert.": "Saved.",
  "Kunde {nr} {name} wirklich löschen?": "Really delete customer {nr} {name}?",
  "{a} von {b} Kunden": "{a} of {b} customers",

  // ---- Einstellungen
  "Speichern & Test-Mail senden": "Save & send test email",
  Speichern: "Save",
  "E-Mail-Server (SMTP)": "Email server (SMTP)",
  Server: "Server",
  Port: "Port",
  Verschlüsselung: "Encryption",
  "Keine (nur für Tests im lokalen Netz)": "None (only for tests on the local network)",
  Benutzer: "User",
  Anmeldung: "Sign-in",
  "Benutzer & Passwort": "User & password",
  "Google (Gmail) – Anmeldung im Browser": "Google (Gmail) – sign in via browser",
  "Microsoft (Outlook.com) – Anmeldung im Browser": "Microsoft (Outlook.com) – sign in via browser",
  "Im Browser anmelden …": "Sign in via browser …",
  Abmelden: "Sign out",
  "Gmail und Outlook.com erlauben das normale Passwort nicht mehr. Bei der Anmeldung im Browser sehen Sie die Seite des Anbieters, Bill2Mail erfährt Ihr Passwort nie. Alternative: ein App-Passwort des Kontos als Passwort verwenden.":
    "Gmail and Outlook.com no longer accept the normal password. When you sign in via the browser you see the provider's own page; Bill2Mail never learns your password. Alternative: use an app password of the account as the password.",
  "Anmeldung mit {n} ist in dieser Programmversion nicht eingerichtet. Bitte Benutzer & Passwort (bzw. App-Passwort) verwenden.":
    "Sign-in with {n} is not set up in this version of the program. Please use user & password (or an app password).",
  "Angemeldet bei {n}.": "Signed in to {n}.",
  "Nicht angemeldet.": "Not signed in.",
  "Browser geöffnet – bitte dort anmelden und die Berechtigung erteilen …": "Browser opened – please sign in there and grant the permission …",
  "Angemeldet.": "Signed in.",
  "Abgemeldet.": "Signed out.",
  Passwort: "Password",
  "Das Passwort wird im Windows-Anmeldeinformationsspeicher abgelegt, nicht in der Datenbank.":
    "The password is stored in the Windows Credential Manager, not in the database.",
  "Absender & Test": "Sender & test",
  "Absender-Adresse": "Sender address",
  "Absender-Name": "Sender name",
  "Test-Empfänger": "Test recipient",
  Erkennung: "Detection",
  "Toleranz der Rahmen (in Punkten, 0–200)": "Frame tolerance (in points, 0–200)",
  "Wie weit der Text vertikal vom Rahmen abweichen darf und trotzdem gefunden wird (z. B. wenn die Adresse eine Zeile länger ist). 72 Punkte = 1 Zoll = 2,54 cm; eine Textzeile hat etwa 10–12 Punkte. Standard: 30. Zu große Werte können fremden Text treffen.":
    "How far the text may deviate vertically from the frame and still be found (e.g. if the address is one line longer). 72 points = 1 inch = 2.54 cm; a line of text is about 10–12 points. Default: 30. Values that are too large may pick up unrelated text.",
  "Automatik: PDF-Ordner überwachen": "Automation: monitor PDF folder",
  "Neue PDFs in diesem Ordner automatisch erkennen und das Versand-Fenster öffnen": "Automatically detect new PDFs in this folder and open the sending window",
  "Ordner wählen …": "Choose folder …",
  "Das Programm wartet, bis die PDF fertig geschrieben und nicht mehr gesperrt ist. Gesendet wird erst nach Klick auf „Versenden“. Das Programm muss dafür geöffnet sein.":
    "The program waits until the PDF is completely written and no longer locked. Nothing is sent until you click “Send”. The program must be running for this.",
  "E-Mail an den Kunden": "Email to the customer",
  Betreff: "Subject",
  "Text (HTML)": "Text (HTML)",
  "wird durch den Kundennamen ersetzt, im Betreff und im Text.": "is replaced by the customer name, in the subject and in the text.",
  "smtp.firma.at": "smtp.company.com",
  "rechnungen@firma.at": "invoices@company.com",
  "Buchhaltung Musterfirma": "Accounting Example Inc.",
  "ihre.adresse@firma.at": "your.address@company.com",
  "name@firma.at": "name@company.com",
  "(gespeichert – leer lassen zum Beibehalten)": "(saved – leave empty to keep)",
  Sprache: "Language",
  "Gilt bis zum Beenden des Programms. Beim nächsten Start wird wieder die Windows-Sprache verwendet.":
    "Applies until the program is closed. The next start uses the Windows language again.",
  Deutsch: "Deutsch",
  English: "English",
  Lizenz: "License",
  "Lizenzdatei laden …": "Load license file …",
  "Lizenz (*.lic)": "License (*.lic)",
  "Ohne Lizenz läuft die Demo: Rahmen setzen, Analyse, Vorschau und Test-Mail funktionieren, Versenden und Ordnerüberwachung sind gesperrt.":
    "Without a license the demo runs: setting frames, analysis, preview and test email work; sending and folder monitoring are locked.",
  "Lizenziert für {k}": "Licensed to {k}",
  "Keine Lizenz installiert (Demo-Version).": "No license installed (demo version).",
  " · gültig bis {d}": " · valid until {d}",
  "Lizenz übernommen.": "License installed.",
  "Demo-Version – Versenden ist erst mit Lizenz möglich (Einstellungen → Lizenz).": "Demo version – sending requires a license (Settings → License).",

  // ---- Automatik-Fenster
  "Neue PDF erkannt": "New PDF detected",
  "+ {n} weitere PDF warten": "+ {n} more PDFs waiting",
  "Es gibt noch keine Vorlage. Bitte zuerst im Reiter „Vorlage & PDF“ die Rahmen setzen.":
    "There is no template yet. Please set the frames in the “Template & PDF” tab first.",
  "Es gibt nichts zu senden.": "There is nothing to send.",
  "{n} E-Mails werden mit „Versenden“ sofort an die Kunden gesendet.": "{n} emails will be sent to the customers immediately when you click “Send”.",
  "Es wurde nichts gesendet.": "Nothing was sent.",
  "Es läuft bereits ein anderer Versand – bitte warten.": "Another sending is in progress – please wait.",
  "Versand nicht durchgeführt: {e}": "Sending not carried out: {e}",
  "Versand abbrechen": "Abort sending",

  // ---- Main
  "{name} – {n} Seiten": "{name} – {n} pages",
  "Fehler: {e}": "Error: {e}",
};

function suche(de: string): string | undefined {
  return EN[de];
}

function eingesetzt(text: string, vars?: Record<string, string | number>): string {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
}

/** Übersetzt einen deutschen Text; `{name}` wird durch `vars` ersetzt. */
export function t(de: string, vars?: Record<string, string | number>): string {
  const text = aktuell === "en" ? (suche(de) ?? de) : de;
  return eingesetzt(text, vars);
}

export function sprache(): Sprache {
  return aktuell;
}

function ausCode(code: string): Sprache {
  const c = code.trim().toLowerCase();
  return c === "" || c.startsWith("de") ? "de" : "en";
}

function gespeichert(): Sprache | null {
  try {
    const s = sessionStorage.getItem(SPEICHER);
    return s === "de" || s === "en" ? s : null;
  } catch {
    return null;
  }
}

/** Beim Start: Sitzungswahl (nach Umschalten), sonst Windows-Sprache. */
export async function spracheStarten(): Promise<void> {
  let s = gespeichert();
  if (!s) {
    let code = "";
    try {
      code = await invoke<string>("sprache_system");
    } catch {
      code = navigator.language;
    }
    s = ausCode(code);
  }
  aktuell = s;
  document.documentElement.lang = s;
  try {
    await invoke("sprache_setzen", { englisch: s === "en" });
  } catch {
    // Vorschau im Browser ohne Backend
  }
  if (s === "en") seiteUebersetzen();
}

/** Nur für diese Sitzung: speichert die Wahl und lädt die Oberfläche neu. */
export function spracheWechseln(s: Sprache) {
  try {
    sessionStorage.setItem(SPEICHER, s);
  } catch {
    /* ohne Speicher bleibt die Sprache unverändert */
  }
  location.reload();
}

// ---------------------------------------------------------------- statisches HTML

function textUebersetzen(node: Text) {
  const roh = node.nodeValue ?? "";
  const kern = roh.trim();
  if (!kern) return;
  const en = suche(kern);
  if (en !== undefined) node.nodeValue = roh.replace(kern, en);
}

function seiteUebersetzen() {
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => {
      const tag = n.parentElement?.tagName;
      return tag === "SCRIPT" || tag === "STYLE" || tag === "TEXTAREA" || tag === "CODE" ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
    },
  });
  const knoten: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) knoten.push(n as Text);
  knoten.forEach(textUebersetzen);
  for (const el of document.querySelectorAll<HTMLElement>("[placeholder], [title]")) {
    for (const a of ["placeholder", "title"]) {
      const v = el.getAttribute(a);
      const en = v ? suche(v) : undefined;
      if (en !== undefined) el.setAttribute(a, en);
    }
  }
}
