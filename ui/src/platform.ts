/**
 * Which desktop the panel is running on.
 *
 * Two strings in the catalogue name the surface the tray item lives in, and the three
 * platforms call it three different things: a menu bar on macOS, the taskbar on Windows, the
 * tray on Linux. Calling all three a menu bar would be wrong on two of them.
 *
 * Read from the user agent rather than from a Tauri plugin. The webview already reports the
 * host it is running on, and `@tauri-apps/plugin-os` would be a dependency, a capability entry
 * and an IPC round trip to learn a word.
 */
export type HostPlatform = "macos" | "windows" | "linux";

export function hostPlatform(): HostPlatform {
  if (typeof navigator === "undefined") return "macos";
  const agent = navigator.userAgent;
  if (/Windows/i.test(agent)) return "windows";
  // Order matters: Android reports Linux, and iOS reports Mac. Neither is a target, but a
  // narrower test costs nothing and keeps this honest if one ever is.
  if (/Linux|X11|BSD/i.test(agent)) return "linux";
  return "macos";
}
