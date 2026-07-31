import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Fixed port: the Tauri dev shell points at it, and a shifting port breaks the tray window.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // `fs.allow` reaches the repo root because the design tokens live in `shared/`, one level
  // above this root. The bundler follows the import either way; only the dev server refuses
  // to serve a file outside its root without being told.
  server: { port: 5173, strictPort: true, fs: { allow: [".."] } },
  build: { target: "safari15", outDir: "dist", emptyOutDir: true },
});
