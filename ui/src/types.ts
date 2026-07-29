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
  /** Equivalent API cost of this bucket, in USD. */
  costUsd: number;
  /** Tokens here whose model carried no known price. Never billed as zero. */
  unpricedTokens: number;
}

/** Cost over a slice of the series, with the part that could not be priced kept apart. */
export interface CostRange {
  usd: number;
  unpricedTokens: number;
}

export type PaceRisk = "healthy" | "at-risk" | "over";

export interface PaceForecast {
  limitId: string;
  /** Identifies the window alongside `limitId`: one limit can hold several windows. */
  windowMinutes: number;
  /**
   * Highest usage the projection reaches before the window resets — the peak, not the
   * endpoint. Against a rolling window a trajectory can cross the ceiling and fall back.
   */
  projectedPercent: number;
  risk: PaceRisk;
  exhaustedAt: string | null;
}

export interface ProviderSnapshot {
  id: ProviderId;
  installed: boolean;
  windows: QuotaWindow[];
  today: TokenRollup;
  /** Equivalent API cost over the same rolling day as `today`. */
  todayCost: CostRange;
  series: Bucket[];
  pace: PaceForecast[];
  lastActivity: string | null;
  unavailable: UnavailableReason | null;
}

/** A ceiling one tier is assumed to allow over one window, in equivalent API dollars. */
export interface PlanCeiling {
  windowMinutes: number;
  costUsd: number;
}

/**
 * A subscription tier, as the provider itself declared it. The panel renders whatever comes
 * back — no tier list is hardcoded here.
 */
export interface PlanOption {
  id: string;
  label: string;
  ceilings: PlanCeiling[];
}

export interface ProviderPlans {
  provider: ProviderId;
  plans: PlanOption[];
}

/**
 * State of the opt-in Claude Code statusline shim: the one thing this app writes outside its
 * own data directory, and only with consent.
 */
export interface StatuslineState {
  supported: boolean;
  installed: boolean;
  settingsPath: string | null;
  /** What `statusLine.command` holds right now. */
  currentCommand: string | null;
  /** What installing would write. Shown beside `currentCommand` as the before/after. */
  proposedCommand: string | null;
  /** What reverting restores. Null means there was no statusline before us. */
  previousCommand: string | null;
  readings: number;
  lastReadingAt: string | null;
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
  /**
   * Chosen tier per provider, keyed by provider id. A provider absent from this map has no
   * tier picked, and therefore gets no estimated window at all.
   */
  plans: Partial<Record<ProviderId, string>>;
}

export function totalTokens(rollup: TokenRollup): number {
  return rollup.input + rollup.output + rollup.cacheRead + rollup.cacheCreation;
}

/**
 * Whether a provider is installed and logging but has nothing to report yet — the state a
 * plan pick or the statusline shim would resolve. Distinct from a tool that is simply absent.
 */
export function awaitingSetup(snapshot: ProviderSnapshot): boolean {
  return (
    snapshot.installed &&
    snapshot.unavailable === "never-reported" &&
    totalTokens(snapshot.today) > 0
  );
}

/**
 * The forecast belonging to one window, or null when nothing could be projected.
 *
 * Matched on both fields: Claude Code reports its five-hour and its weekly limit under one id,
 * and they run out at very different rates.
 */
export function paceFor(
  snapshot: ProviderSnapshot,
  window: QuotaWindow,
): PaceForecast | null {
  return (
    snapshot.pace.find(
      (pace) =>
        pace.limitId === window.limitId && pace.windowMinutes === window.windowMinutes,
    ) ?? null
  );
}

/** Highest reported usage across every window a provider exposes. */
export function peakPercent(snapshot: ProviderSnapshot): number | null {
  const values = snapshot.windows
    .map((window) => window.usedPercent)
    .filter((percent): percent is number => percent !== null);
  return values.length > 0 ? Math.max(...values) : null;
}
