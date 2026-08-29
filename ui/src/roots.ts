/**
 * Additional log folders — the rules, away from the component that renders them.
 *
 * "Additional log folders", never "accounts": every folder here is folded into the same quota
 * identity as the tool's own logs, so two subscriptions pointed at one provider would be added
 * together rather than reported apart.
 *
 * The backend re-checks everything decided here and additionally requires the folder to exist
 * and be readable, which a webview cannot know. What this file decides is what can be settled
 * without asking: the shape of the path, and the size of the list.
 */

/** Mirrors `Deck::MAX_ADDITIONAL_ROOTS` in `app/src/deck.rs`. */
export const MAX_ADDITIONAL_ROOTS = 8;

export type AddRootFailure = "empty" | "relative" | "duplicate" | "too-many";

export type AddRootOutcome =
  | { ok: true; roots: string[] }
  | { ok: false; reason: AddRootFailure };

/**
 * Is this a path the operating system will resolve the same way from any working directory?
 *
 * Three shapes, because the panel runs on three desktops: a POSIX path, a Windows drive path,
 * and a UNC share. A relative path is refused rather than resolved — the app has no working
 * directory the user chose, so resolving one would invent a folder.
 */
function isAbsolute(path: string): boolean {
  return /^\//.test(path) || /^[A-Za-z]:[\\/]/.test(path) || path.startsWith("\\\\");
}

export function addRoot(roots: string[], candidate: string): AddRootOutcome {
  const path = candidate.trim();
  if (path === "") return { ok: false, reason: "empty" };
  if (!isAbsolute(path)) return { ok: false, reason: "relative" };
  if (roots.includes(path)) return { ok: false, reason: "duplicate" };
  if (roots.length >= MAX_ADDITIONAL_ROOTS) return { ok: false, reason: "too-many" };
  // Appended, not sorted: the order is the order the user built, and a list that reshuffles
  // itself after every addition is a list nobody can scan.
  return { ok: true, roots: [...roots, path] };
}

export function removeRoot(roots: string[], path: string): string[] {
  return roots.filter((root) => root !== path);
}
