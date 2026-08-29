/**
 * Additional log folders, decided before anything is sent to the shell.
 *
 * The list the settings screen renders is the list that gets saved, so the rules that shape it
 * live in one pure function rather than inside a component.
 */
import { describe, expect, it } from "vitest";

import { addRoot, MAX_ADDITIONAL_ROOTS, removeRoot } from "./roots";

describe("addRoot", () => {
  it("keeps an absolute path and trims what the user pasted around it", () => {
    expect(addRoot([], "  /Users/someone/logs  ")).toEqual({
      ok: true,
      roots: ["/Users/someone/logs"],
    });
  });

  it("refuses a relative path, because it means a different folder per launch", () => {
    expect(addRoot([], "logs")).toEqual({ ok: false, reason: "relative" });
    expect(addRoot([], "./logs")).toEqual({ ok: false, reason: "relative" });
    expect(addRoot([], "")).toEqual({ ok: false, reason: "empty" });
    expect(addRoot([], "   ")).toEqual({ ok: false, reason: "empty" });
  });

  it("accepts a Windows path, which is absolute without a leading slash", () => {
    expect(addRoot([], "C:\\Users\\someone\\logs")).toEqual({
      ok: true,
      roots: ["C:\\Users\\someone\\logs"],
    });
    expect(addRoot([], "\\\\server\\share\\logs")).toEqual({
      ok: true,
      roots: ["\\\\server\\share\\logs"],
    });
  });

  it("reports a folder that is already in the list instead of adding it twice", () => {
    expect(addRoot(["/a"], "/a")).toEqual({ ok: false, reason: "duplicate" });
  });

  it("refuses to grow past the number the backend will accept", () => {
    const full = Array.from({ length: MAX_ADDITIONAL_ROOTS }, (_, index) => `/folder-${index}`);
    expect(addRoot(full, "/one-more")).toEqual({ ok: false, reason: "too-many" });
  });

  it("appends rather than sorts, so the list stays in the order it was built", () => {
    expect(addRoot(["/b"], "/a")).toEqual({ ok: true, roots: ["/b", "/a"] });
  });
});

describe("removeRoot", () => {
  it("drops exactly the folder named", () => {
    expect(removeRoot(["/a", "/b"], "/a")).toEqual(["/b"]);
    expect(removeRoot(["/a"], "/missing")).toEqual(["/a"]);
  });
});
