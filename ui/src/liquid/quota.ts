/**
 * The three tools the Liquid surfaces lead with, folded to what a glance needs: how much is
 * left, when it resets, where even use would be, and whether the reading is measured.
 */

import type { DeckState, PaceForecast, ProviderSnapshot, QuotaWindow } from "../types";
import { paceFor, worstWindow } from "../types";
import type { Antigravity, ModelReset } from "./system";

export type ToolKey = "claude-code" | "codex" | "antigravity";

export interface WindowGlance {
  /** Share left, 0–100. */
  left: number;
  windowMinutes: number;
  resetsAt: string | null;
  /** Where the fill would stand if the window were used evenly, as a share left (0–100). */
  paceLeft: number | null;
  measured: boolean;
  pace: PaceForecast | null;
}

export interface ToolGlance {
  key: ToolKey;
  installed: boolean;
  /** The window closest to full, or null when nothing has been reported. */
  headline: WindowGlance | null;
  windows: WindowGlance[];
  snapshot: ProviderSnapshot | null;
}

export function evenUseLeft(window: QuotaWindow, now: number): number | null {
  if (window.resetsAt === null) return null;
  const remaining = (Date.parse(window.resetsAt) - now) / 1000;
  const length = window.windowMinutes * 60;
  if (!Number.isFinite(remaining) || length <= 0) return null;
  return Math.min(100, Math.max(0, (remaining / length) * 100));
}

function glance(snapshot: ProviderSnapshot, window: QuotaWindow, now: number): WindowGlance | null {
  if (window.usedPercent === null) return null;
  return {
    left: Math.max(0, 100 - window.usedPercent),
    windowMinutes: window.windowMinutes,
    resetsAt: window.resetsAt,
    paceLeft: evenUseLeft(window, now),
    measured: window.confidence.level === "measured",
    pace: paceFor(snapshot, window),
  };
}

/** The default instance of a tool, never a named copy. */
function defaultInstance(deck: DeckState, key: ToolKey): ProviderSnapshot | null {
  return deck.providers.find((p) => p.id === key && p.instance === key) ?? null;
}

export function toolGlance(deck: DeckState, key: ToolKey, now: number): ToolGlance {
  const snapshot = defaultInstance(deck, key);
  if (snapshot === null) {
    return { key, installed: false, headline: null, windows: [], snapshot: null };
  }
  const worst = worstWindow(snapshot.windows);
  const windows = [...snapshot.windows]
    .sort((a, b) => a.windowMinutes - b.windowMinutes)
    .map((window) => glance(snapshot, window, now))
    .filter((window): window is WindowGlance => window !== null);
  return {
    key,
    installed: snapshot.installed && snapshot.unavailable !== "not-installed",
    headline: worst === null ? null : glance(snapshot, worst, now),
    windows,
    snapshot,
  };
}

/** Models still exhausted at `now`, soonest to reopen first. */
export function openingSoonest(antigravity: Antigravity | null, now: number): ModelReset[] {
  return (antigravity?.exhausted ?? [])
    .filter((reset) => Date.parse(reset.resetsAt) > now)
    .sort((a, b) => Date.parse(a.resetsAt) - Date.parse(b.resetsAt));
}

/** `gemini-3.8-pro-high` → `Pro`; anything unrecognised is returned as written. */
export function modelName(model: string): string {
  const match = /(flash-image|flash-lite|flash|pro|ultra|opus|sonnet|haiku)/i.exec(model);
  if (match === null) return model;
  const word = match[1] ?? model;
  return word
    .split("-")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
    .join(" ");
}
