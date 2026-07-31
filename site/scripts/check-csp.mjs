/**
 * Checks the built pages against the Content-Security-Policy that will be served with them.
 *
 * The page claims the app makes no network request, and it carries a `default-src 'none'`
 * policy so the claim is true of the page as well. That policy is written by hand in
 * `vercel.json`, one directory away from the HTML it governs — which is exactly the kind of
 * pair that drifts. So the build reads what it actually produced:
 *
 *   - every inline <script> must have its sha256 pinned in `script-src`
 *   - every external <script src> must be same-origin
 *
 * A missing hash is a page whose theme never applies in production and whose console fills
 * with CSP violations — a failure that does not show up in `astro dev`, where no policy is
 * served at all.
 */

import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const distDir = join(root, "dist");
const vercelConfig = join(root, "vercel.json");

/** Every `.html` under `dist/`, at any depth. */
async function htmlFiles(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  const found = [];
  for (const entry of entries) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) found.push(...(await htmlFiles(path)));
    else if (entry.name.endsWith(".html")) found.push(path);
  }
  return found;
}

const scriptTag = /<script([^>]*)>([\s\S]*?)<\/script>/gi;
const srcAttribute = /\bsrc\s*=\s*["']([^"']*)["']/i;

function scriptsIn(html) {
  const inline = [];
  const external = [];
  for (const [, attributes, body] of html.matchAll(scriptTag)) {
    const src = attributes.match(srcAttribute);
    if (src) external.push(src[1]);
    else if (body.trim()) inline.push(body);
  }
  return { inline, external };
}

/** The exact form a CSP source expression takes: base64 of the raw script text. */
const hashOf = (body) => `'sha256-${createHash("sha256").update(body, "utf8").digest("base64")}'`;

const policy = JSON.parse(await readFile(vercelConfig, "utf8"))
  .headers.flatMap((rule) => rule.headers)
  .find((header) => header.key === "Content-Security-Policy")?.value;

if (!policy) {
  console.error(`No Content-Security-Policy header in ${relative(root, vercelConfig)}.`);
  process.exit(1);
}

const failures = [];

for (const file of await htmlFiles(distDir)) {
  const page = relative(distDir, file);
  const { inline, external } = scriptsIn(await readFile(file, "utf8"));

  for (const body of inline) {
    const hash = hashOf(body);
    if (!policy.includes(hash)) {
      failures.push(
        `${page}: inline script is not pinned in the CSP. Add ${hash} to script-src in ` +
          `${relative(root, vercelConfig)}.`,
      );
    }
  }

  for (const src of external) {
    if (!src.startsWith("/")) {
      failures.push(`${page}: external script "${src}" is not same-origin; script-src is 'self'.`);
    }
  }
}

if (failures.length > 0) {
  for (const failure of failures) console.error(failure);
  process.exit(1);
}

console.log("CSP check: every inline script is pinned, every external script is same-origin.");
