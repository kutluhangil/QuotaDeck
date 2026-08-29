/**
 * The Spanish catalogue.
 *
 * Typed as `Catalogue`, so a key added to `en.ts` and not translated here fails the build.
 * Product names — Quota Deck, Claude Code, Codex — stay in their original form.
 */

import type { HostPlatform } from "../platform";
import type { ProviderId } from "../types";
import type { Catalogue } from "./en";

/**
 * Cómo llama cada escritorio a la franja donde vive el icono. También abre frases, así que se
 * escriben sin artículo.
 */
const surfaces: Record<HostPlatform, string> = {
  macos: "Barra de menús",
  windows: "Barra de tareas",
  linux: "Bandeja del sistema",
};

const planHintDefault =
  "Solo se usa para estimar cuánto has consumido de tus límites. No se envía nada a ninguna parte para calcularlo.";

const planHints: Partial<Record<ProviderId, string>> = {
  "claude-code":
    "Se usa para estimar cuánto has consumido de tus límites. Anthropic no publica una cifra para estos; hasta que conectes la línea de estado de abajo, esto es una estimación.",
  "copilot-cli":
    "GitHub publica los créditos de cada plan, así que el techo es exacto. Lo estimado es el gasto: aquí solo se cuentan las sesiones de línea de comandos, y solo cuando terminan. Los créditos gastados en tu editor o en la web no se ven desde este equipo.",
};

export const es: Catalogue = {
  appName: "Quota Deck",
  shellFailed: (reason) => `La aplicación no pudo completar una operación: ${reason}`,
  refreshFailed: (reason) => `No se pudo actualizar el uso: ${reason}`,

  header: {
    settings: "Ajustes",
  },

  footer: {
    updated: (time) => `Actualizado a las ${time}`,
    reporting: (reporting, total) =>
      total === 0 ? "" : `${reporting} de ${total} herramientas informan`,
    dashboard: "Panel",
    refresh: "Actualizar",
    quit: "Salir",
  },

  status: {
    ample: "Holgado",
    tight: "Ajustado",
    critical: "Crítico",
  },

  health: {
    rebuilding: (_reason) =>
      "El uso se está reconstruyendo con la tabla de precios actual. Los resultados parciales aún no son definitivos.",
    stale: (reason) =>
      `La última actualización falló. Se muestra la última medición correcta.${reason ? ` ${reason}` : ""}`,
    error: (reason) =>
      `No se pudo actualizar el uso.${reason ? ` ${reason}` : " Vuelve a intentarlo."}`,
    unavailable: (reason) =>
      `Esta herramienta no está disponible ahora mismo.${reason ? ` ${reason}` : ""}`,
  },

  filters: {
    all: "Todas",
    label: "Mostrar una sola herramienta",
  },

  units: {
    day: "d",
    hour: "h",
    minute: "min",
  },

  relative: {
    justNow: "ahora mismo",
    ago: (duration) => `hace ${duration}`,
  },

  confidence: {
    measured: "medido",
    estimated: "estimado",
    idle: "inactivo",
  },

  window: {
    session: "sesión",
    weekly: "semanal",
    monthly: "mensual",
    other: (minutes) => `${minutes} min`,
  },

  strip: {
    now: "ahora",
    tokens: (tokens) => `${tokens} tokens`,
    quiet: "sin uso",
    summary: (duration, tokens) => `Uso en ${duration}: ${tokens} tokens`,
  },

  pace: {
    projected: (percent) => `a este ritmo ${percent}`,
    /** A label rather than a sentence, so the clock format stays out of the grammar. */
    exhausted: (clock, duration) => `Se agota ${clock} · ${duration}`,
    risk: {
      healthy: "bien",
      "at-risk": "en riesgo",
      over: "se pasa",
    },
    label: (percent) => `Previsto ${percent}`,
    rowLabel: "Ritmo",
  },

  card: {
    resetsAt: (time) => `Se restablece a las ${time}`,
    resetsIn: (duration) => `Se libera en ${duration}`,
    noReset: "No informa cuándo se restablece",
    todayTokens: (tokens) => `hoy ${tokens} tokens`,
    todayCost: (amount) => `hoy ${amount}`,
    costPartial: (tokens) => `más ${tokens} tokens sin precio conocido`,
    lastActivity: (when) => `Último uso ${when}`,
    neverUsed: "Todavía no hay uso registrado",
    pickPlan: "Elige tu plan para ver una estimación",
    pickPlanAction: "Elegir plan",
    limitLabel: (window) => `Límite ${window}`,
  },

  quiet: {
    heading: (count) =>
      count === 1 ? "1 herramienta sin instalar" : `${count} herramientas sin instalar`,
  },

  dashboard: {
    title: "Quota Deck",
    rangeLabel: "Periodo",
    range: { day: "Día", week: "Semana", month: "Mes", quarter: "Trimestre", year: "Año" },
    rangeSpan: (days) => (days === 1 ? "Últimas 24 horas" : `Últimos ${days} días`),
    rangeTokens: "Tokens",
    rangeCost: "Coste equivalente",
    retention: (days) => `${days} días de historial en este equipo`,
    unpriced: (tokens) => `No se conocía el precio de ${tokens} tokens`,
    heatmapLabel: "Actividad diaria del último mes",
    heatmapQuiet: "tranquilo",
    heatmapBusy: "intenso",
    customRange: "Fechas",
    rangeFrom: "Desde",
    rangeTo: "Hasta",
    hourlyHistory: "Historial por horas; los registros cuentan en la hora en que empiezan.",
    copyJson: "Copiar JSON",
    copyCsv: "Copiar CSV",
    exporting: "Preparando…",
    copied: (format, rows) => `${format} copiado: ${rows} filas`,
    exportFailed: (reason) => `No se pudo exportar el historial: ${reason}`,
    exportClamped: (from, to) =>
      `El historial por horas copiado va de ${from} a ${to}; el inicio elegido es anterior al historial que guarda este equipo.`,
    exportUnavailable:
      "La exportación no está disponible hasta que el historial de uso actual esté completo.",
    rebuilding: (from, to) =>
      `Se conserva el historial completo de ${from} días mientras ${to} días se reconstruyen desde los registros locales.`,
    rebuildFailed: (reason) => `La reconstrucción del historial está en espera: ${reason}`,
  },

  breakdown: {
    models: "En qué se gastó",
    projects: "En qué directorio",
    unreported: "No informó el modelo",
    unattributed: "No informó el directorio",
    empty: "No se contó nada en este periodo",
    dropped: (count) => `${count} registros sin atribuir: demasiados modelos distintos`,
    droppedProjects: (count) =>
      `${count} registros sin atribuir: demasiados directorios distintos`,
    share: (label, percent) => `${label} — ${percent} de este periodo`,
    listLabel: (tool) => `En qué gastó ${tool} en este periodo, por modelo`,
    projectListLabel: (tool) => `En qué gastó ${tool} en este periodo, por directorio`,
    agents: "Quién lo gastó",
    origin: {
      main: "Conversación principal",
      subagent: "Subagentes",
      workflow: "Agentes de flujo de trabajo",
    },
    droppedAgents: (count) =>
      `${count} registros sin atribuir: demasiados tipos de agente distintos`,
    agentListLabel: (tool) => `En qué gastó ${tool} en este periodo, por tipo de trabajo`,
  },

  burst: {
    label: "Agentes",
    meta: (factor) => `${factor}× una hora normal`,
    detail: (tokens, factor) =>
      `Los agentes han gastado ${tokens} tokens en la última hora, unas ${factor} veces tu hora normal.`,
  },

  empty: {
    noTools: {
      title: "No se encontró ninguna herramienta compatible",
      body: "Quota Deck lee los registros de sesión que tus herramientas de programación ya escriben. Instala Claude Code, Codex u otra herramienta compatible y aparecerá aquí.",
      action: "Herramientas compatibles",
    },
    providersDisabled: {
      title: "Todas las herramientas ocultas",
      body: "Activa al menos una herramienta en Ajustes para volver a leer los registros locales.",
    },
    noPermission: {
      title: "Hace falta acceso a la carpeta",
      body: "Quota Deck necesita leer los registros de sesión de tu carpeta personal. Lo concedes una vez y nada sale de este dispositivo.",
      action: "Elegir carpeta",
    },
    demoAction: "Ver con datos de ejemplo",
    scanning: "Leyendo registros de sesión…",
  },

  unavailable: {
    "not-installed": "No instalada",
    "no-logs-found": "Todavía no hay registros de sesión",
    "permission-denied": "Sin acceso a esta carpeta",
    "read-error": "No se pudo leer un registro de sesión",
    "never-reported": "Esta herramienta no ha informado de ningún límite",
  },

  provider: {
    "claude-code": "Claude Code",
    codex: "Codex",
    "copilot-cli": "Copilot CLI",
  },

  settings: {
    title: "Ajustes",
    trayTitle: (platform: HostPlatform) => surfaces[platform],
    trayGlyph: "Icono",
    trayGlyphHint: "Una sola barra. Ni número ni color mientras no haga falta.",
    trayCompact: "Porcentaje",
    trayCompactHint: "El mayor consumo informado, como número.",
    trayStrip: "Horizonte",
    trayStripHint: "La versión pequeña de la franja temporal del panel.",
    themeTitle: "Apariencia",
    themeSystem: "Como el sistema",
    themeDark: "Oscuro",
    themeLight: "Claro",
    retentionTitle: "Historial guardado en este equipo",
    retentionDays: (days) => `${days} días`,
    retentionHint:
      "Elegir un periodo más largo solo vuelve a leer los registros locales. El último historial completo sigue disponible hasta que termine.",
    retentionRebuilding: (from, to) =>
      `Se conserva el historial completo de ${from} días mientras ${to} días se reconstruyen desde los registros locales.`,
    back: "Listo",
    settingsFailed: (reason) => `No se pudieron guardar los ajustes: ${reason}`,
    providersTitle: "Herramientas",
    providersHint:
      "Las herramientas desactivadas no se leen, no se vigilan, no avisan y no se exportan.",
    providerEnabled: (provider) => `Leer ${provider}`,
    providerUp: (provider) => `Subir ${provider}`,
    providerDown: (provider) => `Bajar ${provider}`,

    rootsTitle: (provider) => `${provider} · carpetas de registro adicionales`,
    rootsHint:
      "Se suman al mismo límite que los registros propios de esta herramienta. Para los registros de otra máquina en un disco compartido, no para una segunda suscripción.",
    rootsUnsupported:
      "Esta versión lee tu carpeta personal con un único permiso y no puede abrir otro; las carpetas adicionales no están disponibles aquí.",
    rootsPlaceholder: "Ruta absoluta a una carpeta",
    rootsAdd: "Añadir carpeta",
    rootsRemove: (path) => `Quitar ${path}`,
    rootsEmpty: "No hay carpetas adicionales.",
    rootsInvalidEmpty: "Escribe primero la ruta completa de una carpeta.",
    rootsInvalidRelative:
      "Usa la ruta completa. Una ruta sin referencia fija señala una carpeta distinta en cada arranque.",
    rootsInvalidDuplicate: "Esa carpeta ya está en la lista.",
    rootsInvalidTooMany: (limit) => `Como máximo ${limit} carpetas adicionales por herramienta.`,

    instancesTitle: "Cuentas separadas",
    instancesHint:
      "Un segundo inicio de sesión en la misma herramienta tiene su propio límite. Añádelo para seguirlo aparte, con su plan, sus umbrales y su historial. Solo lee las carpetas de registro que le des abajo: no se copia nada de la primera ni se comparte nada con ella.",
    instancesEmpty: "No hay cuentas separadas.",
    instancesTool: "Herramienta",
    instancesNamePlaceholder: "nombre-corto",
    instancesLabelPlaceholder: "Nombre que se ve en la tarjeta",
    instancesAdd: "Añadir cuenta",
    instancesRemove: (name) => `Quitar ${name}`,
    instancesInvalidName:
      "Usa minúsculas, dígitos y guiones: esto será la clave con la que se guarda la cuenta.",

    languageTitle: "Idioma",
    languageSystem: "Como el sistema",
    languageEnglish: "English",
    languageTurkish: "Türkçe",
    languageGerman: "Deutsch",
    languageSpanish: "Español",
    languageHint:
      "Los formatos de fecha y hora siguen la configuración regional del sistema.",

    startupTitle: "Abrir al iniciar sesión",
    startupOn: "Añadir la entrada",
    startupOff: "Quitar la entrada",
    startupHint:
      "Este ajuste gestiona la entrada de inicio de Windows. La configuración de Windows o el Administrador de tareas pueden desactivarla por su cuenta.",
    startupFailed: (reason) =>
      `No se pudo cambiar el inicio automático de Windows: ${reason}`,

    planTitle: (provider) => `Plan de ${provider}`,
    planHint: (provider) => planHints[provider] ?? planHintDefault,
    planNone: "Sin elegir",
    planNoneHint: "No se muestra ninguna estimación. No se supone nada.",

    alertsTitle: "Avisar al",
    alertsHint:
      "Recibes una notificación cuando un límite pasa una de estas marcas, una vez por límite y ventana. macOS pide permiso antes de la primera.",
    alertsThreshold: (percent) => `${percent} %`,
    alertsOff: "Sin avisos para esta herramienta",
    muteTitle: "Silencio",
    muteHour: "Durante una hora",
    muteToday: "Hasta mañana",
    muteClear: "Volver a activar los avisos",
    mutedUntil: (time) => `En silencio hasta las ${time}`,

    statuslineTitle: "Límites medidos",
    statuslineBody:
      "Claude Code entrega el porcentaje real de sus límites de 5 horas y semanal a su propia línea de estado. Quota Deck lee statusLine.command para comprobar la conexión. En los dos caminos de configuración guarda el valor anterior de statusLine en su propia carpeta de datos local antes de que el objeto cambie o de pedirte que lo cambies, para poder deshacerlo por completo. Nada sale de este dispositivo.",
    statuslineUnsupported:
      "Todavía no se puede comprobar aquí el ajuste de línea de estado de Claude Code.",
    statuslineConnect: "Conectar la línea de estado",
    statuslineConnecting: "Conectando…",
    statuslineRevert: "Desconectar",
    statuslineReverting: "Desconectando…",
    statuslineInstalled: "Conectada",
    statuslineFile: (path) => `Edita ${path}`,
    statuslineBefore: "Ahora",
    statuslineAfter: "Después de conectar",
    statuslineNoPrevious:
      "No tienes ninguna línea de estado configurada. Al desconectar se quita otra vez el ajuste.",
    statuslineChains:
      "Tu línea de estado actual sigue funcionando: la nuestra pasa su salida tal cual.",
    statuslineManualNotice:
      "La versión de la App Store solo puede leer los ajustes de Claude Code. Quota Deck no modifica ese archivo.",
    statuslineManualInstruction:
      "Sustituye el valor statusLine de nivel superior por el objeto JSON completo de abajo. Incluye el campo type obligatorio y conserva los demás campos de statusLine.",
    statuslineManualRestore:
      "Para desconectar, devuelve statusLine.command al comando anterior que aparece abajo.",
    statuslineManualRestoreObject:
      "Para desconectar, sustituye el valor statusLine de nivel superior por el objeto JSON original de abajo.",
    statuslineManualRemove:
      "Para desconectar, quita el campo statusLine del archivo de ajustes.",
    statuslineManualRemoveCommand:
      "Para desconectar, quita solo el campo statusLine.command y conserva los demás campos de statusLine.",
    statuslineCopyCommand: "Copiar el JSON de statusLine",
    statuslineCopyPrevious: "Copiar el comando anterior",
    statuslineCopyPreviousObject: "Copiar el statusLine anterior",
    statuslineCopied: "Comando copiado",
    statuslineCopyFailed: (reason) => `No se pudo copiar el comando: ${reason}`,
    statuslineRefresh: "Volver a comprobar el estado",
    statuslineRefreshing: "Comprobando…",
    statuslineWaiting:
      "Todavía no hay lecturas. Claude Code solo las envía en una sesión interactiva, después de su primera respuesta.",
    statuslineReadings: (count, when) => `${count} lecturas, la última ${when}`,
    statuslineFailed: (reason) => `No se pudo cambiar el ajuste: ${reason}`,

    accessTitle: "Acceso a la carpeta",
    accessGranted: (path) => `Leyendo ${path}`,
    accessMissing: "Todavía no se ha elegido ninguna carpeta.",
    accessChoose: "Elegir carpeta",
    accessRevoke: "Retirar el acceso",
    accessFailed: (reason) => `El permiso guardado no se pudo usar: ${reason}`,
    accessHint:
      "Registros de sesión de solo lectura y, si quieres, el campo statusLine.command de tus ajustes de Claude. Los archivos de credenciales de los proveedores no se abren nunca. Retirarlo tiene efecto inmediato.",

    demoTitle: "Datos de ejemplo",
    demoOn: "Mostrar el ejemplo",
    demoOff: "Mostrar mi equipo",
    demoHint: (platform: HostPlatform) =>
      `Cifras creíbles pero inventadas, para ver cómo funciona la aplicación antes de instalar ninguna herramienta. La ${surfaces[platform].toLowerCase()} sigue informando de tu uso real.`,
  },

  a11y: {
    tools: "Herramientas vigiladas",
    settingsRegion: "Ajustes",
    status: "Estado",
    panelActions: "Acciones del panel",
    footerActions: "Acciones",
    windows: (provider) => `Límites de ${provider}`,
    source: (source) => `Fuente: ${source}`,
  },
};
