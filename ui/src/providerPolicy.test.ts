import { describe, expect, it } from "vitest";

import {
  applyProviderPolicy,
  focusDirectionAfterMove,
  providerPolicySettings,
} from "./providerPolicy";
import type { Settings } from "./types";
import { PROVIDER_IDS } from "./types";

const settings: Settings = {
  trayMode: "glyph",
  theme: "system",
  locale: "system",
  plans: {},
  alerts: {},
  mutedUntil: null,
  demo: false,
  disabledProviders: [],
  additionalRoots: {},
  instances: {},
  providerOrder: ["claude-code", "codex", "copilot-cli"],
  retentionDays: 32,
};

describe("provider policy", () => {
  it("exposes only provider ids compiled into the backend registry", () => {
    expect(PROVIDER_IDS).toEqual(["claude-code", "codex", "copilot-cli"]);
  });

  it("rolls an optimistic update back when persistence fails", async () => {
    const optimistic = providerPolicySettings(settings, ["codex"], [
      "copilot-cli",
      "claude-code",
      "codex",
    ]);

    const outcome = await applyProviderPolicy(settings, optimistic, async () => {
      throw new Error("disk full");
    });

    expect(outcome.settings).toBe(settings);
    expect(outcome.persisted).toBe(false);
    expect(outcome.error).toContain("disk full");
  });

  it("uses the authoritative saved settings after persistence succeeds", async () => {
    const optimistic = providerPolicySettings(settings, ["codex"], settings.providerOrder);
    const saved: Settings = {
      ...optimistic,
      providerOrder: ["codex", "claude-code", "copilot-cli"],
    };

    const outcome = await applyProviderPolicy(settings, optimistic, async () => ({
      settings: saved,
      warning: null,
    }));

    expect(outcome).toEqual({ settings: saved, error: null, persisted: true });
  });

  it("keeps authoritative settings and exposes a watcher warning without rollback", async () => {
    const optimistic = providerPolicySettings(settings, ["codex"], settings.providerOrder);
    const saved: Settings = {
      ...optimistic,
      providerOrder: ["codex", "claude-code", "copilot-cli"],
    };

    const outcome = await applyProviderPolicy(settings, optimistic, async () => ({
      settings: saved,
      warning: "Provider policy was saved, but filesystem watcher sync failed",
    }));

    expect(outcome.settings).toBe(saved);
    expect(outcome.persisted).toBe(true);
    expect(outcome.error).toContain("filesystem watcher sync failed");
  });

  it("moves focus to the enabled opposite button at a list boundary", () => {
    expect(focusDirectionAfterMove(1, 3, 1)).toBe(1);
    expect(focusDirectionAfterMove(2, 3, 1)).toBe(-1);
    expect(focusDirectionAfterMove(0, 3, -1)).toBe(1);
  });
});
