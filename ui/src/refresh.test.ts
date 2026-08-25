import { describe, expect, it } from "vitest";

import { completeRefresh } from "./refresh";

describe("manual refresh completion", () => {
  it("stays busy until the matching or a newer generation arrives", () => {
    expect(completeRefresh(7, { refreshGeneration: 6, refreshError: null })).toEqual({
      pendingRequest: 7,
      error: null,
    });
    expect(completeRefresh(7, { refreshGeneration: 8, refreshError: null })).toEqual({
      pendingRequest: null,
      error: null,
    });
  });

  it("exposes a completed pass failure without leaving the request busy", () => {
    expect(completeRefresh(3, { refreshGeneration: 3, refreshError: "store flush failed" })).toEqual(
      {
        pendingRequest: null,
        error: "store flush failed",
      },
    );
  });
});
