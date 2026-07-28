/**
 * Mirrors the Rust types in `core/src/types.rs`. Raw log lines never cross this boundary —
 * the backend sends folded snapshots only.
 */

export type ProviderId =
  | "claude-code"
  | "codex"
  | "copilot-cli"
  | "kimi"
  | "gemini-cli"
  | "qwen"
  | "opencode"
  | "amp"
  | "droid"
  | "codebuff"
  | "hermes"
  | "pi-agent"
  | "goose"
  | "kilo"
  | "openclaw"
  | "antigravity";

export type UnavailableReason =
  | "not-installed"
  | "no-logs-found"
  | "permission-denied"
  | "never-reported";

export type DerivationBasis = "token-window" | "request-count";

export type Confidence =
  | { level: "measured"; reportedAt: string }
  | { level: "derived"; basis: DerivationBasis }
  | { level: "stale"; reportedAt: string; ageSeconds: number }
  | { level: "unavailable"; reason: UnavailableReason };

/** A window is classified by the duration the provider reported, never by its slot name. */
export type WindowKind = "session" | "weekly" | "monthly" | "other";

export interface QuotaWindow {
  /** Groups windows belonging to one limit. A provider can report several at once. */
  limitId: string;
  kind: WindowKind;
  windowMinutes: number;
  usedPercent: number | null;
  /** ISO-8601. Providers report an absolute instant, not a countdown. */
  resetsAt: string | null;
  confidence: Confidence;
}

export interface TokenRollup {
  input: number;
  output: number;
  cacheRead: number;
  cacheCreation: number;
  reasoning: number;
}

export interface Bucket {
  /** Unix epoch seconds at the bucket start, on a five-minute grid. */
  start: number;
  tokens: TokenRollup;
  requests: number;
}

export type PaceRisk = "healthy" | "at-risk" | "over";

export interface PaceForecast {
  limitId: string;
  projectedPercent: number;
  risk: PaceRisk;
  exhaustedAt: string | null;
}

export interface ProviderSnapshot {
  id: ProviderId;
  installed: boolean;
  windows: QuotaWindow[];
  today: TokenRollup;
  series: Bucket[];
  pace: PaceForecast[];
  lastActivity: string | null;
  unavailable: UnavailableReason | null;
}

/** What the backend pushes on every refresh. */
export interface DeckState {
  providers: ProviderSnapshot[];
  /** ISO-8601 timestamp of the scan that produced these snapshots. */
  updatedAt: string;
  scanning: boolean;
}

export type TrayMode = "glyph" | "compact" | "strip";

export interface Settings {
  trayMode: TrayMode;
  theme: "system" | "dark" | "light";
}

export function totalTokens(rollup: TokenRollup): number {
  return rollup.input + rollup.output + rollup.cacheRead + rollup.cacheCreation;
}

/** Highest reported usage across every window a provider exposes. */
export function peakPercent(snapshot: ProviderSnapshot): number | null {
  const values = snapshot.windows
    .map((window) => window.usedPercent)
    .filter((percent): percent is number => percent !== null);
  return values.length > 0 ? Math.max(...values) : null;
}
