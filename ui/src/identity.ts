/**
 * The dot that tells one tool's rows from another's.
 *
 * Identity, not level. The ramp (`--level-*`) means exactly one thing in this app — a quota
 * running out — and blueprint §7.2 keeps it off anything that is not a fullness indicator. So
 * these hues are drawn from the violet-to-cyan arc only: nothing in the green, amber or coral
 * bands the ramp occupies, and nothing that could be read as a reading.
 *
 * Four of them, cycled by the provider's fixed position. Two tools far enough apart in the list
 * to collide are almost never on screen together, and when they are, the name is written beside
 * the dot — the dot speeds up recognition, it does not carry meaning on its own.
 */

import { PROVIDER_IDS, type ProviderId } from "./types";

/** Matches the `--id-*` token count in `styles/tokens.css`. */
const IDENTITY_HUES = 4;

export function identityHue(provider: ProviderId): number {
  const index = PROVIDER_IDS.indexOf(provider);
  // A provider the frontend does not know about would land at -1 and paint nothing.
  return index < 0 ? 0 : index % IDENTITY_HUES;
}
