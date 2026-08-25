import type { Confidence, ProviderHealth, ProviderSnapshot } from "./types";

export type VisibleProviderHealth = Omit<ProviderHealth, "state"> & {
  state: "rebuilding" | "stale" | "error" | "unavailable";
};

export function visibleProviderHealth(
  snapshot: Pick<ProviderSnapshot, "id">,
  health: ProviderHealth[],
): VisibleProviderHealth | null {
  const match = health.find((entry) => entry.provider === snapshot.id);
  if (
    match === undefined ||
    (match.state !== "rebuilding" &&
      match.state !== "stale" &&
      match.state !== "error" &&
      match.state !== "unavailable")
  ) {
    return null;
  }
  return { ...match, state: match.state };
}

export function confidenceForHealth(
  confidence: Confidence,
  health: VisibleProviderHealth | null,
  nowMs: number,
): Confidence {
  if (health?.state !== "stale") return confidence;
  const reportedAt =
    health.lastSuccessAt ??
    (confidence.level === "measured" || confidence.level === "stale"
      ? confidence.reportedAt
      : new Date(0).toISOString());
  return {
    level: "stale",
    reportedAt,
    ageSeconds: Math.max(0, Math.floor((nowMs - Date.parse(reportedAt)) / 1000)),
  };
}
