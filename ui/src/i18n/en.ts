/**
 * The English catalogue, and the shape every other language is held to.
 *
 * `Catalogue` is derived from this object rather than declared separately, so a key added here
 * is a compile error in `tr.ts` until it is translated. That is the point: a half-translated
 * panel that silently falls back to English is worse than one that will not build.
 *
 * Numbers, dates and clock times are not in here. They are formatted by `Intl` from the
 * viewer's own conventions — see `intlLocale` in `./index.ts`.
 */

import type { HostPlatform } from "../platform";
import type { PaceRisk, ProviderId, UnavailableReason, WindowKind } from "../types";

/**
 * What this desktop calls the strip the tray item lives in.
 *
 * Each catalogue writes its own, because the word has to fit the sentence around it — English
 * wants it lowercase mid-sentence and capitalised as a heading, and the next language may want
 * a suffix on it.
 */
const surfaces: Record<HostPlatform, string> = {
  macos: "menu bar",
  windows: "taskbar",
  linux: "tray",
};

function sentenceCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

const planHintDefault =
  "Used to estimate how full your limits are. Nothing is sent anywhere to work it out.";

const planHints: Partial<Record<ProviderId, string>> = {
  "claude-code":
    "Used to estimate how full your limits are. Anthropic does not publish a number for these, so this is an estimate until you connect the status line below.",
  "copilot-cli":
    "GitHub publishes the credit allowance for each plan, so the ceiling is exact. The estimate is in the spend: only command-line sessions count here, and only once they have ended. Credits used in your editor or on the web are not visible from this machine.",
};

export const en = {
  appName: "Quota Deck",

  header: {
    settings: "Settings",
  },

  footer: {
    updated: (time: string) => `Updated ${time}`,
    /** Says how many detected tools actually have a reading to show. */
    reporting: (reporting: number, total: number) =>
      total === 0 ? "" : `${reporting} of ${total} reporting`,
    dashboard: "Dashboard",
    /**
     * With no dock icon, the tray menu used to be the only way out. It is a right click on
     * macOS and Windows and a left click on Linux, which is three different gestures for the
     * one action every user eventually wants — so it is also a button.
     */
    quit: "Quit",
  },

  /**
   * The reading as a word, so the level survives colour blindness and a greyscale screenshot.
   * Taken from the fullest window a tool reports, which is the one that will stop the work.
   */
  status: {
    ample: "Good",
    tight: "Caution",
    critical: "Critical",
  },

  filters: {
    all: "All",
    label: "Show one tool",
  },

  /**
   * Duration units, as the compact forms the panel sets them in. Kept to one or two letters:
   * these appear inside a 380px row beside a number and must not push it around.
   */
  units: {
    day: "d",
    hour: "h",
    minute: "m",
  },

  relative: {
    justNow: "just now",
    /** Rendered as "2h ago". */
    ago: (duration: string) => `${duration} ago`,
  },

  confidence: {
    measured: "measured",
    estimated: "estimated",
    idle: "idle",
  },

  window: {
    session: "session",
    weekly: "weekly",
    monthly: "monthly",
    /** Falls back to the reported duration when the shape is unfamiliar. */
    other: (minutes: number) => `${minutes} min`,
  } satisfies Record<WindowKind, string | ((minutes: number) => string)>,

  strip: {
    now: "now",
    /** Hover readout for one slice of the timeline. */
    tokens: (tokens: string) => `${tokens} tokens`,
    quiet: "no usage",
    summary: (duration: string, tokens: string) =>
      `Usage over the last ${duration}: ${tokens} tokens`,
  },

  /**
   * Where a window is heading. The wording never states a projection as a fact: "at this
   * pace" is doing real work, because the number after it is not a reading.
   */
  pace: {
    projected: (percent: string) => `${percent} at this pace`,
    /** An instant plus the countdown to it, e.g. "Full 17:42 · 2h 05m". */
    exhausted: (clock: string, duration: string) => `Full ${clock} · ${duration}`,
    risk: {
      healthy: "on pace",
      "at-risk": "at risk",
      over: "over",
    } satisfies Record<PaceRisk, string>,
    /** Read out for the meter beside the risk word, which is decorative on its own. */
    label: (percent: string) => `Projected ${percent}`,
    /** Names the projection row where the other rows carry a window length. */
    rowLabel: "Pace",
  },

  card: {
    resetsAt: (time: string) => `Resets ${time}`,
    resetsIn: (duration: string) => `Frees up in ${duration}`,
    noReset: "Reset time not reported",
    todayTokens: (tokens: string) => `${tokens} tokens today`,
    todayCost: (amount: string) => `${amount} today`,
    /** Says outright that a dollar figure is short, rather than quietly under-reporting. */
    costPartial: (tokens: string) => `plus ${tokens} tokens at an unknown price`,
    lastActivity: (when: string) => `Last used ${when}`,
    neverUsed: "No usage recorded yet",
    /**
     * Shown when a tool is logging but no tier has been picked. The app will not guess one:
     * an unrequested percentage reads as a real reading.
     */
    pickPlan: "Pick your plan to see an estimate",
    pickPlanAction: "Choose plan",
    /**
     * The fullness bar's spoken name. The bar restates the number printed beside it, so the
     * printed pair is hidden from assistive technology and the meter announces both — its
     * name from here and its value from `aria-valuetext`.
     */
    limitLabel: (window: string) => `${window} limit`,
  },

  quiet: {
    /** Collapsed section for tools that are not on this machine at all. */
    heading: (count: number) =>
      count === 1 ? "1 tool not installed" : `${count} tools not installed`,
  },

  dashboard: {
    title: "Quota Deck",
    rangeLabel: "Range",
    /** Rolling, not calendar: the same window model the panel uses. */
    range: { day: "Day", week: "Week", month: "Month" },
    rangeSpan: (days: number) => (days === 1 ? "Last 24 hours" : `Last ${days} days`),
    rangeTokens: "Tokens",
    rangeCost: "Equivalent cost",
    retention: (days: number) => `${days} days of history kept on this device`,
    /**
     * Standalone form of the panel's `costPartial`. On a card the phrase follows a dollar
     * figure and reads as a continuation; here it is a line of its own and has to stand up
     * without one.
     */
    unpriced: (tokens: string) => `${tokens} tokens carried no known price`,
    heatmapLabel: "Daily activity over the last month",
    heatmapQuiet: "quiet",
    heatmapBusy: "busy",
  },

  empty: {
    noTools: {
      title: "No supported tool found",
      body: "Quota Deck reads the session logs that coding tools already write. Install Claude Code, Codex or another supported tool and it will appear here.",
      action: "See supported tools",
    },
    noPermission: {
      title: "Folder access needed",
      body: "Quota Deck needs to read the session logs in your home folder. You grant this once, and nothing ever leaves this device.",
      action: "Choose folder",
    },
    /** Offered beside the grant, so a machine with no tools installed is not a dead end. */
    demoAction: "See a sample instead",
    scanning: "Reading session logs…",
  },

  unavailable: {
    "not-installed": "Not installed",
    "no-logs-found": "No session logs yet",
    "permission-denied": "No access to this folder",
    "never-reported": "This tool has not reported a limit",
  } satisfies Record<UnavailableReason, string>,

  /**
   * Provider names are set as spaced uppercase text, never as vendor logos. Reproducing a
   * third-party mark in the app is a trademark risk and would break the design language.
   *
   * These are product names and stay in their original form in every language.
   */
  provider: {
    "claude-code": "Claude Code",
    codex: "Codex",
    "copilot-cli": "Copilot CLI",
    kimi: "Kimi",
    "gemini-cli": "Gemini CLI",
    qwen: "Qwen Code",
    opencode: "OpenCode",
    amp: "Amp",
    droid: "Droid",
    codebuff: "Codebuff",
    hermes: "Hermes",
    "pi-agent": "pi-agent",
    goose: "Goose",
    kilo: "Kilo",
    openclaw: "OpenClaw",
    antigravity: "Antigravity",
  } satisfies Record<ProviderId, string>,

  settings: {
    title: "Settings",
    trayTitle: (platform: HostPlatform) => sentenceCase(surfaces[platform]),
    trayGlyph: "Glyph",
    trayGlyphHint: "A single bar. No numbers, no colour until it matters.",
    trayCompact: "Percentage",
    trayCompactHint: "The highest reported usage, as a number.",
    trayStrip: "Horizon",
    trayStripHint: "A miniature of the panel's timeline.",
    themeTitle: "Appearance",
    themeSystem: "Match system",
    themeDark: "Dark",
    themeLight: "Light",
    back: "Done",
    settingsFailed: (reason: string) => `Could not save settings: ${reason}`,

    languageTitle: "Language",
    languageSystem: "Match system",
    /**
     * Each language names itself. Someone who has landed in a language they cannot read has
     * to be able to find their way out of it.
     */
    languageEnglish: "English",
    languageTurkish: "Türkçe",
    /** Times and dates are a regional setting, not a language one, and stay with the system. */
    languageHint: "Dates and clock times keep following your system's regional settings.",

    planTitle: (provider: string) => `${provider} plan`,
    /**
     * Why the number is an estimate, said in the provider's own terms. The reason differs:
     * Anthropic publishes no ceiling at all, while GitHub publishes an exact one and the
     * doubt sits in what we can see of the spend against it. A single sentence covering both
     * would be vague enough to be useless.
     */
    planHint: (provider: ProviderId) => planHints[provider] ?? planHintDefault,
    planNone: "Not set",
    planNoneHint: "No estimate is shown. Nothing is guessed.",

    alertsTitle: "Warn me at",
    /**
     * Says outright that the app decides nothing about whether a notification appears — the
     * operating system asked first, and the user can take that back without coming here.
     */
    alertsHint:
      "A notification when a limit crosses one of these, once per limit per window. macOS asks for permission before the first one.",
    alertsThreshold: (percent: number) => `${percent}%`,
    alertsOff: "No warnings for this tool",
    muteTitle: "Quiet",
    muteHour: "For an hour",
    muteToday: "Until tomorrow",
    muteClear: "Turn warnings back on",
    mutedUntil: (time: string) => `Muted until ${time}`,

    statuslineTitle: "Measured limits",
    statuslineBody:
      "Claude Code hands its status line the real percentage for your 5-hour and weekly limits. Quota Deck reads statusLine.command to inspect the connection. Before either setup flow changes, or asks you to change, that object, it stores the complete prior statusLine value in its own local data directory for exact restoration. Nothing leaves this device.",
    statuslineUnsupported: "Claude Code status line setup cannot be inspected yet.",
    statuslineConnect: "Connect status line",
    statuslineConnecting: "Connecting…",
    statuslineRevert: "Disconnect",
    statuslineReverting: "Disconnecting…",
    statuslineInstalled: "Connected",
    statuslineFile: (path: string) => `Edits ${path}`,
    statuslineBefore: "Now",
    statuslineAfter: "After connecting",
    statuslineNoPrevious: "You have no status line set. Disconnecting removes the setting again.",
    /** The commitment that makes this safe to accept. */
    statuslineChains: "Your existing status line keeps running — ours passes its output through.",
    statuslineManualNotice:
      "The App Store version can only read Claude Code settings. Quota Deck does not change this file.",
    statuslineManualInstruction:
      "Replace the top-level statusLine value with the complete JSON object below. It includes the required type and preserves your other statusLine fields.",
    statuslineManualRestore:
      "To disconnect, restore statusLine.command to the previous command below.",
    statuslineManualRestoreObject:
      "To disconnect, replace the top-level statusLine value with the exact prior JSON object below.",
    statuslineManualRemove:
      "To disconnect, remove the statusLine field from the settings file.",
    statuslineManualRemoveCommand:
      "To disconnect, remove only statusLine.command and preserve the other statusLine fields.",
    statuslineCopyCommand: "Copy statusLine JSON",
    statuslineCopyPrevious: "Copy previous command",
    statuslineCopyPreviousObject: "Copy previous statusLine JSON",
    statuslineCopied: "Command copied",
    statuslineCopyFailed: (reason: string) => `Could not copy the command: ${reason}`,
    statuslineRefresh: "Check again",
    statuslineRefreshing: "Checking…",
    statuslineWaiting:
      "No reading yet. Claude Code sends this only in an interactive session, after its first reply.",
    statuslineReadings: (count: number, when: string) =>
      count === 1 ? `1 reading, last ${when}` : `${count} readings, last ${when}`,
    statuslineFailed: (reason: string) => `Could not change the setting: ${reason}`,

    accessTitle: "Folder access",
    /** Names the folder rather than describing it: the grant is over a specific path. */
    accessGranted: (path: string) => `Reading ${path}`,
    accessMissing: "No folder has been chosen yet.",
    accessChoose: "Choose folder",
    accessRevoke: "Revoke access",
    accessFailed: (reason: string) => `The stored grant could not be used: ${reason}`,
    /** Says what the grant is not, which is the part that makes it safe to give. */
    accessHint:
      "Read-only session logs, plus statusLine.command from Claude settings for the optional integration. Provider credential files are never opened. Revoking takes effect at once.",

    demoTitle: "Sample deck",
    demoOn: "Show a sample",
    demoOff: "Show my machine",
    demoHint: (platform: HostPlatform) =>
      `Realistic but invented figures, so the app can be seen working before any tool is installed. The ${surfaces[platform]} keeps reporting your real usage.`,
  },

  /**
   * Names for things a screen reader has to announce but the design deliberately leaves
   * unlabelled on screen. Never duplicates visible copy — where a heading already says it,
   * the region is associated with that heading instead.
   */
  a11y: {
    tools: "Tracked tools",
    settingsRegion: "Settings",
    status: "Status",
    /** The panel's own toolbar: the settings button. */
    panelActions: "Panel actions",
    /** The bar along the bottom: the dashboard and quit buttons. */
    footerActions: "Actions",
    /** Names what a provider's rows belong to, since the card's heading is above them. */
    windows: (provider: string) => `${provider} limits`,
    /** The mark beside a percentage says where the number came from; this reads it out. */
    source: (source: string) => `Source: ${source}`,
  },
};

export type Catalogue = typeof en;
