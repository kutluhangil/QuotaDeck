import { describe, expect, it } from "vitest";

import type { HostPlatform } from "../platform";
import type { ProviderId } from "../types";
import { en } from "./en";
import { tr } from "./tr";
import { LANGUAGES, catalogueFor, intlLocale, languageFor } from "./index";

/**
 * Every leaf of a catalogue, as `path -> shape`.
 *
 * A function's shape carries its arity: a translator who drops a placeholder produces a
 * one-argument function where the original took two, and the interpolation silently
 * disappears from the sentence. TypeScript alone does not catch that — `(a, b) => "x"` is
 * assignable to a two-argument signature with either arity written out.
 */
const PLATFORMS: HostPlatform[] = ["macos", "windows", "linux"];

function shape(value: unknown, path = ""): Map<string, string> {
  const leaves = new Map<string, string>();
  if (typeof value === "function") {
    leaves.set(path, `function/${value.length}`);
    return leaves;
  }
  if (typeof value !== "object" || value === null) {
    leaves.set(path, typeof value);
    return leaves;
  }
  for (const [key, child] of Object.entries(value)) {
    for (const [childPath, childShape] of shape(child, path === "" ? key : `${path}.${key}`)) {
      leaves.set(childPath, childShape);
    }
  }
  return leaves;
}

describe("catalogues", () => {
  it("cover exactly the same keys", () => {
    const english = [...shape(en).keys()].sort();
    const turkish = [...shape(tr).keys()].sort();
    expect(turkish).toEqual(english);
  });

  it("keep every placeholder a sentence was written around", () => {
    const english = shape(en);
    for (const [path, form] of shape(tr)) {
      expect(form, `tr.${path} does not take the arguments en.${path} does`).toBe(
        english.get(path),
      );
    }
  });

  it("leave nothing blank", () => {
    for (const [path, value] of Object.entries(flatten(tr))) {
      expect(value.trim(), `tr.${path} is empty`).not.toBe("");
    }
  });

  it("call the tray's surface what each desktop calls it", () => {
    // The three platforms name this strip three different things, and the copy is written
    // around the noun. A catalogue that returned the same word for all three would read as
    // "Menu bar" on a machine that has no menu bar.
    for (const catalogue of [en, tr]) {
      const surfaces = PLATFORMS.map((platform) => catalogue.settings.trayTitle(platform));
      expect(new Set(surfaces).size, `${surfaces.join(", ")} are not three distinct names`).toBe(
        PLATFORMS.length,
      );
      // The sample-deck sentence names the same surface, and it is the one place where the
      // noun sits inside a sentence rather than on its own.
      for (const platform of PLATFORMS) {
        const surface = catalogue.settings.trayTitle(platform).toLowerCase();
        expect(catalogue.settings.demoHint(platform).toLowerCase()).toContain(surface);
      }
    }
  });

  it("say the same thing about the same provider", () => {
    // Product names are not translated. A "Codex" that became something else in one catalogue
    // would stop matching what the user sees in their own terminal.
    for (const id of Object.keys(en.provider) as ProviderId[]) {
      expect(tr.provider[id]).toBe(en.provider[id]);
    }
  });
});

/** Every plain-string leaf, keyed by path. Functions cannot be called without arguments. */
function flatten(value: unknown, path = ""): Record<string, string> {
  if (typeof value === "string") return { [path]: value };
  if (typeof value !== "object" || value === null) return {};
  let flat: Record<string, string> = {};
  for (const [key, child] of Object.entries(value)) {
    flat = { ...flat, ...flatten(child, path === "" ? key : `${path}.${key}`) };
  }
  return flat;
}

describe("locale resolution", () => {
  it("hands an explicit pick straight through", () => {
    expect(languageFor("tr")).toBe("tr");
    expect(catalogueFor("tr")).toBe(tr);
    expect(catalogueFor("en")).toBe(en);
  });

  it("returns the same object every time, so a selector does not thrash", () => {
    // zustand v5 compares snapshots by reference; a catalogue built per call would make every
    // component that reads copy re-render on every store update.
    expect(catalogueFor("tr")).toBe(catalogueFor("tr"));
  });

  it("leaves the number and date conventions to the system unless a language was picked", () => {
    expect(intlLocale("system")).toBeUndefined();
    expect(intlLocale("tr")).toBe("tr");
  });

  it("has a catalogue for every language it advertises", () => {
    for (const language of LANGUAGES) {
      expect(catalogueFor(language)).toBeDefined();
    }
  });
});
