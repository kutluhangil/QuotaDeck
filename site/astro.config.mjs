// @ts-check
import { defineConfig } from "astro/config";

/**
 * A static marketing site, in the two languages the app itself ships.
 *
 * Astro rather than the app's own Vite/React chain for one reason: this page ships no
 * JavaScript at all unless a component asks for it, and a page about an app whose headline
 * claim is that it costs nothing to run should not open with a 78 KB bundle.
 *
 * `prefixDefaultLocale: false` keeps English at `/` and puts Turkish at `/tr/`, which is what
 * the two catalogues in `ui/src/i18n` already imply.
 */
export default defineConfig({
  site: "https://quotadeck.app",
  i18n: {
    defaultLocale: "en",
    locales: ["en", "tr"],
    routing: { prefixDefaultLocale: false },
  },
  build: { inlineStylesheets: "auto" },
  vite: {
    // The design tokens live at the repo root, one level above this root, so the app and the
    // site cannot drift apart by hand. Only the dev server needs telling.
    server: { fs: { allow: [".."] } },
  },
});
