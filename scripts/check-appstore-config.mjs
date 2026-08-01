import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const fromRoot = (path) => readFileSync(join(root, path), "utf8");

const base = JSON.parse(fromRoot("app/tauri.conf.json"));
const override = JSON.parse(fromRoot("app/tauri.appstore.conf.json"));
const cargo = fromRoot("app/Cargo.toml");
const entitlements = fromRoot("app/Entitlements.appstore.plist");

function requireConfig(condition, message) {
  if (!condition) {
    throw new Error(`unsafe Mac App Store configuration: ${message}`);
  }
}

requireConfig(
  base.app?.macOSPrivateApi === false,
  "the base config must not enable app.macOSPrivateApi",
);
requireConfig(
  base.app?.windows?.every((window) => window.transparent === false),
  "base windows must not require macOS private transparency APIs",
);
requireConfig(
  override.app?.macOSPrivateApi === false,
  "app.macOSPrivateApi must be false in the App Store override",
);
requireConfig(
  override.app?.windows?.every((window) => window.transparent === false),
  "every App Store window must be opaque",
);
requireConfig(
  !cargo.includes('"macos-private-api"'),
  "Cargo.toml must not force-enable Tauri's macos-private-api feature",
);
requireConfig(
  entitlements.includes("com.apple.security.files.user-selected.read-only"),
  "the user-selected home grant must remain read-only",
);
requireConfig(
  !entitlements.includes("com.apple.security.files.user-selected.read-write"),
  "the App Store build must not gain write access to the selected home directory",
);

console.log("Mac App Store config check: private APIs off, selected home read-only.");
