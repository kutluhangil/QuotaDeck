import { describe, expect, it } from "vitest";

import { formatCost } from "./format";

describe("formatCost", () => {
  it("keeps cents where cents are the whole figure", () => {
    expect(formatCost(0.42)).toBe("$0.42");
    expect(formatCost(4.05)).toBe("$4.05");
  });

  it("drops cents once they are noise", () => {
    // At $86 the cents are two digits of precision nobody reads, and they make the footer
    // column jitter on every refresh.
    expect(formatCost(86.42)).toBe("$86");
    expect(formatCost(623.91)).toBe("$624");
  });

  it("groups thousands", () => {
    expect(formatCost(2126.53)).toBe("$2,127");
  });

  it("says a small amount is small rather than rounding it to nothing", () => {
    // "$0.00" reads as free. A fraction of a cent was still spent.
    expect(formatCost(0.004)).toBe("<$0.01");
    expect(formatCost(0)).toBe("$0");
  });
});
