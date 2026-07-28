/**
 * Every user-facing string, in one place.
 *
 * Phase 8 turns this into the English catalogue of a two-language bundle. Keeping the copy
 * here from the start means that phase swaps a module rather than hunting through JSX.
 */

import type { ProviderId, UnavailableReason, WindowKind } from "./types";

export const strings = {
  appName: "Quota Deck",

  header: {
    settings: "Settings",
    expand: "Open dashboard",
  },

  footer: {
    updated: (time: string) => `Updated ${time}`,
    /** Says how many detected tools actually have a reading to show. */
    reporting: (reporting: number, total: number) =>
      total === 0 ? "" : `${reporting} of ${total} reporting`,
  },

  confidence: {
    measured: "measured",
    estimated: "estimated",
    idle: "idle",
    /** Rendered as "2h ago" next to the stale marker. */
    stale: (age: string) => `${age} ago`,
  },

  window: {
    session: "session",
    weekly: "weekly",
    monthly: "monthly",
    /** Falls back to the reported duration when the shape is unfamiliar. */
    other: (minutes: number) => `${minutes} min`,
  } satisfies Record<WindowKind, string | ((minutes: number) => string)>,

  strip: {
    /**
     * Leading axis label. This is the strip's main lesson: almost nobody knows their Codex
     * limit is counted over a week rather than over an afternoon.
     */
    ago: (duration: string) => `${duration} ago`,
    now: "now",
    /** Hover readout for one slice of the timeline. */
    tokens: (tokens: string) => `${tokens} tokens`,
    quiet: "no usage",
    summary: (duration: string, tokens: string) =>
      `Usage over the last ${duration}: ${tokens} tokens`,
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
  },

  quiet: {
    /** Collapsed section for installed tools with nothing to show. */
    heading: (count: number) => (count === 1 ? "1 tool is quiet" : `${count} tools are quiet`),
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
    trayTitle: "Menu bar",
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

    planTitle: (provider: string) => `${provider} plan`,
    /**
     * The honest framing. No vendor publishes a numeric ceiling for a coding subscription, so
     * the tier drives an estimate — and the shim below turns that estimate into a measurement.
     */
    planHint:
      "Used to estimate how full your limits are. Anthropic does not publish a number for these, so this is an estimate until you connect the statusline below.",
    planNone: "Not set",
    planNoneHint: "No estimate is shown. Nothing is guessed.",

    statuslineTitle: "Measured limits",
    statuslineBody:
      "Claude Code hands its status line the real percentage for your 5-hour and weekly limits. Connecting it replaces the estimate with the measurement. Nothing leaves this device and no credentials are read.",
    statuslineUnsupported: "Claude Code settings were not found on this machine.",
    statuslineConnect: "Connect status line",
    statuslineRevert: "Disconnect",
    statuslineInstalled: "Connected",
    statuslineFile: (path: string) => `Edits ${path}`,
    statuslineBefore: "Now",
    statuslineAfter: "After connecting",
    statuslineNoPrevious: "You have no status line set. Disconnecting removes the setting again.",
    /** The commitment that makes this safe to accept. */
    statuslineChains: "Your existing status line keeps running — ours passes its output through.",
    statuslineWaiting:
      "No reading yet. Claude Code sends this only in an interactive session, after its first reply.",
    statuslineReadings: (count: number, when: string) =>
      count === 1 ? `1 reading, last ${when}` : `${count} readings, last ${when}`,
    statuslineFailed: (reason: string) => `Could not change the setting: ${reason}`,
  },
} as const;
