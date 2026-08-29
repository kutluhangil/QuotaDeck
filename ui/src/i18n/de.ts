/**
 * The German catalogue.
 *
 * Typed as `Catalogue`, so a key added to `en.ts` and not translated here fails the build.
 * Product names — Quota Deck, Claude Code, Codex — stay in their original form.
 */

import type { HostPlatform } from "../platform";
import type { ProviderId } from "../types";
import type { Catalogue } from "./en";

/**
 * Wie dieser Desktop die Leiste nennt, in der das Symbol sitzt. Die Wörter stehen auch am
 * Satzanfang, also bleiben sie unflektiert.
 */
const surfaces: Record<HostPlatform, string> = {
  macos: "Menüleiste",
  windows: "Taskleiste",
  linux: "Systemleiste",
};

const planHintDefault =
  "Wird nur benutzt, um zu schätzen, wie voll deine Limits sind. Für diese Rechnung wird nichts verschickt.";

const planHints: Partial<Record<ProviderId, string>> = {
  "claude-code":
    "Wird benutzt, um zu schätzen, wie voll deine Limits sind. Anthropic veröffentlicht dafür keine Zahl; bis du unten die Statuszeile verbindest, bleibt das eine Schätzung.",
  "copilot-cli":
    "GitHub veröffentlicht das Guthaben jedes Tarifs, die Obergrenze ist also exakt. Geschätzt wird die Ausgabenseite: hier zählen nur Kommandozeilen-Sitzungen, und erst nachdem sie beendet sind. Guthaben, das im Editor oder im Web verbraucht wird, ist von diesem Gerät aus nicht sichtbar.",
};

export const de: Catalogue = {
  appName: "Quota Deck",
  shellFailed: (reason) => `Die App konnte einen Vorgang nicht abschließen: ${reason}`,
  refreshFailed: (reason) => `Die Nutzung konnte nicht aktualisiert werden: ${reason}`,

  header: {
    settings: "Einstellungen",
  },

  footer: {
    updated: (time) => `Aktualisiert um ${time}`,
    reporting: (reporting, total) =>
      total === 0 ? "" : `${reporting} von ${total} Werkzeugen melden`,
    dashboard: "Übersicht",
    refresh: "Aktualisieren",
    quit: "Beenden",
  },

  status: {
    ample: "Reichlich",
    tight: "Knapp",
    critical: "Kritisch",
  },

  health: {
    rebuilding: (_reason) =>
      "Die Nutzung wird mit der aktuellen Preistabelle neu aufgebaut. Teilergebnisse sind noch nicht endgültig.",
    stale: (reason) =>
      `Die letzte Aktualisierung ist fehlgeschlagen. Angezeigt wird die letzte erfolgreiche Messung.${reason ? ` ${reason}` : ""}`,
    error: (reason) =>
      `Die Nutzung konnte nicht aktualisiert werden.${reason ? ` ${reason}` : " Versuche es noch einmal."}`,
    unavailable: (reason) =>
      `Dieses Werkzeug ist derzeit nicht verfügbar.${reason ? ` ${reason}` : ""}`,
  },

  filters: {
    all: "Alle",
    label: "Nur ein Werkzeug zeigen",
  },

  units: {
    day: "T",
    hour: "Std",
    minute: "Min",
  },

  relative: {
    justNow: "gerade eben",
    ago: (duration) => `vor ${duration}`,
  },

  confidence: {
    measured: "gemessen",
    estimated: "geschätzt",
    idle: "inaktiv",
  },

  window: {
    session: "Sitzung",
    weekly: "Woche",
    monthly: "Monat",
    other: (minutes) => `${minutes} Min`,
  },

  strip: {
    now: "jetzt",
    tokens: (tokens) => `${tokens} Token`,
    quiet: "keine Nutzung",
    summary: (duration, tokens) => `Nutzung in den letzten ${duration}: ${tokens} Token`,
  },

  pace: {
    projected: (percent) => `in diesem Tempo ${percent}`,
    /** A label rather than a sentence, so the clock format stays out of the grammar. */
    exhausted: (clock, duration) => `Aufgebraucht ${clock} · ${duration}`,
    risk: {
      healthy: "im Rahmen",
      "at-risk": "gefährdet",
      over: "darüber",
    },
    label: (percent) => `Voraussichtlich ${percent}`,
    rowLabel: "Tempo",
  },

  card: {
    resetsAt: (time) => `Zurückgesetzt um ${time}`,
    resetsIn: (duration) => `Frei in ${duration}`,
    noReset: "Kein Zurücksetzen gemeldet",
    todayTokens: (tokens) => `heute ${tokens} Token`,
    todayCost: (amount) => `heute ${amount}`,
    costPartial: (tokens) => `plus ${tokens} Token ohne bekannten Preis`,
    lastActivity: (when) => `Zuletzt benutzt ${when}`,
    neverUsed: "Noch keine Nutzung aufgezeichnet",
    pickPlan: "Wähle deinen Tarif, um eine Schätzung zu sehen",
    pickPlanAction: "Tarif wählen",
    limitLabel: (window) => `${window}-Limit`,
  },

  quiet: {
    heading: (count) =>
      count === 1 ? "1 Werkzeug nicht installiert" : `${count} Werkzeuge nicht installiert`,
  },

  dashboard: {
    title: "Quota Deck",
    rangeLabel: "Zeitraum",
    range: { day: "Tag", week: "Woche", month: "Monat", quarter: "Quartal", year: "Jahr" },
    rangeSpan: (days) => (days === 1 ? "Letzte 24 Stunden" : `Letzte ${days} Tage`),
    rangeTokens: "Token",
    rangeCost: "Entsprechende Kosten",
    retention: (days) => `${days} Tage Verlauf auf diesem Gerät`,
    unpriced: (tokens) => `Für ${tokens} Token war kein Preis bekannt`,
    heatmapLabel: "Tägliche Aktivität im letzten Monat",
    heatmapQuiet: "ruhig",
    heatmapBusy: "voll",
    customRange: "Daten",
    rangeFrom: "Von",
    rangeTo: "Bis",
    hourlyHistory: "Stündlicher Verlauf; Einträge zählen zur angefangenen Stunde.",
    copyJson: "JSON kopieren",
    copyCsv: "CSV kopieren",
    exporting: "Wird vorbereitet…",
    copied: (format, rows) => `${format} kopiert: ${rows} Zeilen`,
    exportFailed: (reason) => `Der Verlauf konnte nicht exportiert werden: ${reason}`,
    exportClamped: (from, to) =>
      `Der kopierte stündliche Verlauf reicht von ${from} bis ${to}; der gewählte Beginn liegt vor dem Verlauf, den dieses Gerät aufbewahrt.`,
    exportUnavailable:
      "Der Export ist nicht verfügbar, bis der aktuelle Nutzungsverlauf vollständig ist.",
    rebuilding: (from, to) =>
      `Der vollständige Verlauf von ${from} Tagen bleibt erhalten, während ${to} Tage aus lokalen Protokollen neu aufgebaut werden.`,
    rebuildFailed: (reason) => `Der Neuaufbau des Verlaufs wartet: ${reason}`,
  },

  breakdown: {
    models: "Wofür es ausgegeben wurde",
    projects: "In welchem Verzeichnis",
    unreported: "Kein Modell gemeldet",
    unattributed: "Kein Verzeichnis gemeldet",
    empty: "In diesem Zeitraum wurde nichts gezählt",
    dropped: (count) =>
      `${count} Einträge ohne Zuordnung — zu viele verschiedene Modelle`,
    droppedProjects: (count) =>
      `${count} Einträge ohne Zuordnung — zu viele verschiedene Verzeichnisse`,
    share: (label, percent) => `${label} — ${percent} dieses Zeitraums`,
    listLabel: (tool) => `Wofür ${tool} in diesem Zeitraum ausgegeben hat, nach Modell`,
    projectListLabel: (tool) =>
      `Wofür ${tool} in diesem Zeitraum ausgegeben hat, nach Verzeichnis`,
    agents: "Wer es ausgegeben hat",
    origin: {
      main: "Hauptunterhaltung",
      subagent: "Subagenten",
      workflow: "Workflow-Agenten",
    },
    droppedAgents: (count) =>
      `${count} Einträge ohne Zuordnung — zu viele verschiedene Agententypen`,
    agentListLabel: (tool) => `Wofür ${tool} in diesem Zeitraum ausgegeben hat, nach Arbeitsart`,
  },

  burst: {
    label: "Agenten",
    meta: (factor) => `${factor}× eine übliche Stunde`,
    detail: (tokens, factor) =>
      `Agenten haben in der letzten Stunde ${tokens} Token verbraucht — etwa das ${factor}-Fache deiner üblichen Stunde.`,
  },

  empty: {
    noTools: {
      title: "Keine unterstützten Werkzeuge gefunden",
      body: "Quota Deck liest die Sitzungsprotokolle, die deine Coding-Werkzeuge ohnehin schreiben. Installiere Claude Code, Codex oder ein anderes unterstütztes Werkzeug, damit es hier erscheint.",
      action: "Unterstützte Werkzeuge",
    },
    providersDisabled: {
      title: "Alle Werkzeuge ausgeblendet",
      body: "Aktiviere in den Einstellungen mindestens ein Werkzeug, um lokale Protokolle wieder zu lesen.",
    },
    noPermission: {
      title: "Ordnerzugriff nötig",
      body: "Quota Deck muss die Sitzungsprotokolle in deinem Persönlichen Ordner lesen. Du erteilst das einmal, und nichts verlässt dieses Gerät.",
      action: "Ordner wählen",
    },
    demoAction: "Mit Beispieldaten ansehen",
    scanning: "Sitzungsprotokolle werden gelesen…",
  },

  unavailable: {
    "not-installed": "Nicht installiert",
    "no-logs-found": "Noch keine Sitzungsprotokolle",
    "permission-denied": "Kein Zugriff auf diesen Ordner",
    "read-error": "Ein Sitzungsprotokoll konnte nicht gelesen werden",
    "never-reported": "Dieses Werkzeug hat kein Limit gemeldet",
  },

  provider: {
    "claude-code": "Claude Code",
    codex: "Codex",
    "copilot-cli": "Copilot CLI",
  },

  settings: {
    title: "Einstellungen",
    trayTitle: (platform: HostPlatform) => surfaces[platform],
    trayGlyph: "Symbol",
    trayGlyphHint: "Ein einzelner Balken. Keine Zahl und keine Farbe, solange nichts nötig ist.",
    trayCompact: "Prozent",
    trayCompactHint: "Die höchste gemeldete Auslastung, als Zahl.",
    trayStrip: "Horizont",
    trayStripHint: "Die kleine Fassung des Zeitstreifens im Panel.",
    themeTitle: "Erscheinungsbild",
    themeSystem: "Wie das System",
    themeDark: "Dunkel",
    themeLight: "Hell",
    retentionTitle: "Auf diesem Gerät aufbewahrter Verlauf",
    retentionDays: (days) => `${days} Tage`,
    retentionHint:
      "Ein längerer Zeitraum liest nur die lokalen Protokolle neu. Der letzte vollständige Verlauf bleibt nutzbar, bis der neue fertig ist.",
    retentionRebuilding: (from, to) =>
      `Der vollständige Verlauf von ${from} Tagen bleibt erhalten, während ${to} Tage aus lokalen Protokollen neu aufgebaut werden.`,
    back: "Fertig",
    settingsFailed: (reason) => `Die Einstellungen konnten nicht gespeichert werden: ${reason}`,
    providersTitle: "Werkzeuge",
    providersHint:
      "Deaktivierte Werkzeuge werden nicht gelesen, nicht beobachtet, lösen keine Warnung aus und werden nicht exportiert.",
    providerEnabled: (provider) => `${provider} lesen`,
    providerUp: (provider) => `${provider} nach oben`,
    providerDown: (provider) => `${provider} nach unten`,

    rootsTitle: (provider) => `${provider} · zusätzliche Protokollordner`,
    rootsHint:
      "Wird demselben Limit zugerechnet wie die eigenen Protokolle dieses Werkzeugs. Gedacht für die Protokolle einer zweiten Maschine auf einer gemeinsamen Platte, nicht für ein zweites Abo.",
    rootsUnsupported:
      "Diese Fassung liest deinen Persönlichen Ordner über eine einzige Freigabe und kann keine zweite öffnen; zusätzliche Ordner sind hier nicht verfügbar.",
    rootsPlaceholder: "Absoluter Pfad zu einem Ordner",
    rootsAdd: "Ordner hinzufügen",
    rootsRemove: (path) => `${path} entfernen`,
    rootsEmpty: "Keine zusätzlichen Ordner.",
    rootsInvalidEmpty: "Gib zuerst den vollständigen Pfad zu einem Ordner ein.",
    rootsInvalidRelative:
      "Nimm den vollständigen Pfad. Ein Pfad ohne festen Bezug meint bei jedem Start einen anderen Ordner.",
    rootsInvalidDuplicate: "Dieser Ordner steht schon auf der Liste.",
    rootsInvalidTooMany: (limit) => `Höchstens ${limit} zusätzliche Ordner pro Werkzeug.`,

    instancesTitle: "Getrennte Konten",
    instancesHint:
      "Eine zweite Anmeldung beim selben Werkzeug hat ihr eigenes Limit. Füge sie hinzu, um sie getrennt zu verfolgen — mit eigenem Tarif, eigenen Schwellen und eigenem Verlauf. Sie liest nur die Protokollordner, die du ihr unten gibst; nichts wird vom ersten Konto kopiert und nichts mit ihm geteilt.",
    instancesEmpty: "Keine getrennten Konten.",
    instancesTool: "Werkzeug",
    instancesNamePlaceholder: "kurzname",
    instancesLabelPlaceholder: "Name auf der Karte",
    instancesAdd: "Konto hinzufügen",
    instancesRemove: (name) => `${name} entfernen`,
    instancesInvalidName:
      "Nimm Kleinbuchstaben, Ziffern und Bindestriche — daraus wird der Schlüssel, unter dem das Konto gespeichert wird.",

    languageTitle: "Sprache",
    languageSystem: "Wie das System",
    languageEnglish: "English",
    languageTurkish: "Türkçe",
    languageGerman: "Deutsch",
    languageSpanish: "Español",
    languageHint:
      "Datums- und Uhrzeitformate richten sich weiterhin nach den Regionseinstellungen des Systems.",

    startupTitle: "Bei der Anmeldung starten",
    startupOn: "Eintrag anlegen",
    startupOff: "Eintrag entfernen",
    startupHint:
      "Diese Einstellung verwaltet den Windows-Autostart-Eintrag. Die Windows-Einstellungen oder der Task-Manager können ihn zusätzlich deaktivieren.",
    startupFailed: (reason) =>
      `Der Windows-Autostart konnte nicht geändert werden: ${reason}`,

    planTitle: (provider) => `${provider}-Tarif`,
    planHint: (provider) => planHints[provider] ?? planHintDefault,
    planNone: "Nicht gewählt",
    planNoneHint: "Es wird keine Schätzung angezeigt. Nichts wird angenommen.",

    alertsTitle: "Warnen bei",
    alertsHint:
      "Du bekommst eine Mitteilung, wenn ein Limit eine dieser Marken überschreitet — einmal pro Limit und Zeitfenster. macOS fragt vor der ersten um Erlaubnis.",
    alertsThreshold: (percent) => `${percent} %`,
    alertsOff: "Keine Warnungen für dieses Werkzeug",
    muteTitle: "Stumm",
    muteHour: "Für eine Stunde",
    muteToday: "Bis morgen",
    muteClear: "Warnungen wieder einschalten",
    mutedUntil: (time) => `Stumm bis ${time}`,

    statuslineTitle: "Gemessene Limits",
    statuslineBody:
      "Claude Code gibt den echten Prozentwert seiner 5-Stunden- und Wochenlimits an die eigene Statuszeile. Quota Deck liest statusLine.command, um die Verbindung zu prüfen. In beiden Einrichtungswegen speichert es den vorherigen statusLine-Wert im eigenen lokalen Datenordner, bevor das Objekt geändert wird oder du gebeten wirst, es zu ändern — damit sich alles vollständig zurücknehmen lässt. Nichts verlässt dieses Gerät.",
    statuslineUnsupported:
      "Die Statuszeilen-Einstellung von Claude Code lässt sich hier noch nicht prüfen.",
    statuslineConnect: "Statuszeile verbinden",
    statuslineConnecting: "Wird verbunden…",
    statuslineRevert: "Verbindung lösen",
    statuslineReverting: "Wird gelöst…",
    statuslineInstalled: "Verbunden",
    statuslineFile: (path) => `Bearbeitet ${path}`,
    statuslineBefore: "Jetzt",
    statuslineAfter: "Nach dem Verbinden",
    statuslineNoPrevious:
      "Du hast keine Statuszeile eingerichtet. Beim Lösen wird die Einstellung wieder entfernt.",
    statuslineChains:
      "Deine bisherige Statuszeile läuft weiter — unsere reicht ihre Ausgabe unverändert durch.",
    statuslineManualNotice:
      "Die App-Store-Fassung darf die Claude-Code-Einstellungen nur lesen. Quota Deck ändert diese Datei nicht.",
    statuslineManualInstruction:
      "Ersetze den obersten statusLine-Wert durch das folgende vollständige JSON-Objekt. Es enthält das nötige type-Feld und bewahrt die übrigen statusLine-Felder.",
    statuslineManualRestore:
      "Um die Verbindung zu lösen, setze statusLine.command auf den folgenden früheren Befehl zurück.",
    statuslineManualRestoreObject:
      "Um die Verbindung zu lösen, ersetze den obersten statusLine-Wert durch das folgende ursprüngliche JSON-Objekt.",
    statuslineManualRemove:
      "Um die Verbindung zu lösen, entferne das statusLine-Feld aus der Einstellungsdatei.",
    statuslineManualRemoveCommand:
      "Um die Verbindung zu lösen, entferne nur das Feld statusLine.command und bewahre die übrigen statusLine-Felder.",
    statuslineCopyCommand: "statusLine-JSON kopieren",
    statuslineCopyPrevious: "Früheren Befehl kopieren",
    statuslineCopyPreviousObject: "Vorheriges statusLine-JSON kopieren",
    statuslineCopied: "Befehl kopiert",
    statuslineCopyFailed: (reason) => `Der Befehl konnte nicht kopiert werden: ${reason}`,
    statuslineRefresh: "Status erneut prüfen",
    statuslineRefreshing: "Wird geprüft…",
    statuslineWaiting:
      "Noch keine Messwerte. Claude Code sendet sie nur in einer interaktiven Sitzung, nach der ersten Antwort.",
    statuslineReadings: (count, when) => `${count} Messwerte, zuletzt ${when}`,
    statuslineFailed: (reason) => `Die Einstellung konnte nicht geändert werden: ${reason}`,

    accessTitle: "Ordnerzugriff",
    accessGranted: (path) => `Liest ${path}`,
    accessMissing: "Noch kein Ordner gewählt.",
    accessChoose: "Ordner wählen",
    accessRevoke: "Zugriff zurücknehmen",
    accessFailed: (reason) => `Die gespeicherte Freigabe war nicht nutzbar: ${reason}`,
    accessHint:
      "Nur lesbare Sitzungsprotokolle und, wenn du es willst, das Feld statusLine.command in deinen Claude-Einstellungen. Anmeldedateien von Anbietern werden nie geöffnet. Das Zurücknehmen wirkt sofort.",

    demoTitle: "Beispieldaten",
    demoOn: "Beispiel zeigen",
    demoOff: "Meine Maschine zeigen",
    demoHint: (platform: HostPlatform) =>
      `Glaubwürdige, aber erfundene Zahlen, damit sichtbar ist, wie die App arbeitet, bevor irgendein Werkzeug installiert ist. Die ${surfaces[platform]} meldet weiterhin deine echte Nutzung.`,
  },

  a11y: {
    tools: "Beobachtete Werkzeuge",
    settingsRegion: "Einstellungen",
    status: "Status",
    panelActions: "Panel-Aktionen",
    footerActions: "Aktionen",
    windows: (provider) => `${provider}-Limits`,
    source: (source) => `Quelle: ${source}`,
  },
};
