import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Fixed port: the Tauri dev shell points at it, and a shifting port breaks the tray window.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "safari15", outDir: "dist", emptyOutDir: true },
});
