import { describe, expect, it } from "vitest";

import { formatCost, formatDuration, formatPercent, formatTokens } from "./format";
import { en } from "./i18n/en";
import { tr } from "./i18n/tr";

describe("formatCost", () => {
  it("keeps cents where cents are the whole figure", () => {
    expect(formatCost(0.42, "en")).toBe("$0.42");
    expect(formatCost(4.05, "en")).toBe("$4.05");
  });

  it("drops cents once they are noise", () => {
    // At $86 the cents are two digits of precision nobody reads, and they make the footer
    // column jitter on every refresh.
    expect(formatCost(86.42, "en")).toBe("$86");
    expect(formatCost(623.91, "en")).toBe("$624");
  });

  it("groups thousands", () => {
    expect(formatCost(2126.53, "en")).toBe("$2,127");
  });

  it("says a small amount is small rather than rounding it to nothing", () => {
    // "$0.00" reads as free. A fraction of a cent was still spent.
    expect(formatCost(0.004, "en")).toBe("<$0.01");
    expect(formatCost(0, "en")).toBe("$0");
  });

  it("stays in dollars whatever the language is", () => {
    // The figure is the equivalent list price of the tokens, not a charge in the reader's own
    // currency. Converting it would invent an exchange rate nobody asked for.
    expect(formatCost(2126.53, "tr")).toBe("$2.127");
  });
});

describe("formatPercent", () => {
  it("puts the sign where the language puts it", () => {
    // Turkish writes %76. Concatenating the sign after the number would be wrong in half the
    // catalogue, which is why this goes through Intl rather than a template string.
    expect(formatPercent(76, "en")).toBe("76%");
    expect(formatPercent(76, "tr")).toBe("%76");
  });

  it("keeps whole numbers, because the providers report whole numbers", () => {
    expect(formatPercent(76.4, "en")).toBe("76%");
    expect(formatPercent(0, "en")).toBe("0%");
  });
});

describe("formatTokens", () => {
  it("uses the language's own decimal separator", () => {
    expect(formatTokens(1_240_000, "en")).toBe("1.2M");
    expect(formatTokens(1_240_000, "tr")).toBe("1,2M");
  });

  it("leaves small counts whole", () => {
    expect(formatTokens(847, "en")).toBe("847");
  });
});

describe("formatDuration", () => {
  it("takes its units from the catalogue", () => {
    expect(formatDuration(6 * 86_400 + 19 * 3_600, en)).toBe("6d 19h");
    expect(formatDuration(6 * 86_400 + 19 * 3_600, tr)).toBe("6g 19sa");
  });

  it("keeps the minute component padded so a countdown does not jitter", () => {
    expect(formatDuration(2 * 3_600 + 5 * 60, en)).toBe("2h 05m");
    expect(formatDuration(47 * 60, tr)).toBe("47dk");
  });
});
