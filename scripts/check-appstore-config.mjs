import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const fromRoot = (path) => readFileSync(join(root, path), "utf8");

function requireConfig(condition, message) {
  if (!condition) {
    throw new Error(`unsafe Mac App Store configuration: ${message}`);
  }
}

export function checkAppStoreConfig(read = fromRoot) {
  const configs = [
    { name: "base", value: JSON.parse(read("app/tauri.conf.json")) },
    {
      name: "App Store override",
      value: JSON.parse(read("app/tauri.appstore.conf.json")),
    },
    {
      name: "Microsoft Store override",
      value: JSON.parse(read("app/tauri.msstore.conf.json")),
    },
    { name: "Linux override", value: JSON.parse(read("app/tauri.linux.conf.json")) },
  ];
  const base = configs[0].value;
  const appStore = configs[1].value;
  const cargo = read("app/Cargo.toml");
  const entitlements = read("app/Entitlements.appstore.plist");

  requireConfig(
    base.app?.macOSPrivateApi === false,
    "the base config must not enable app.macOSPrivateApi",
  );
  requireConfig(
    base.app?.windows?.every((window) => window.transparent === false),
    "base windows must not require macOS private transparency APIs",
  );
  for (const config of configs) {
    requireConfig(
      config.value.app?.trayIcon === undefined,
      `${config.name} config must not declare app.trayIcon; app/src/tray.rs owns tray creation`,
    );
  }
  requireConfig(
    appStore.app?.macOSPrivateApi === false,
    "app.macOSPrivateApi must be false in the App Store override",
  );
  requireConfig(
    appStore.app?.windows?.every((window) => window.transparent === false),
    "every App Store window must be opaque",
  );
  requireConfig(
    !cargo.includes('"macos-private-api"'),
    "Cargo.toml must not force-enable Tauri's macos-private-api feature",
  );
  // The bundler copies every `[[bin]]` of the packaged crate into `Contents/MacOS`, so a second
  // one here would silently ship inside the store build. `docs/STORE.md` §9 says the GUI does
  // not install the command line; `quotadeck-cli` is a separate crate to keep that true.
  requireConfig(
    (cargo.match(/^\[\[bin\]\]/gm) ?? []).length === 1,
    "app/Cargo.toml must declare exactly one [[bin]]; a second one ships inside the App Store bundle",
  );
  requireConfig(
    entitlements.includes("com.apple.security.files.user-selected.read-only"),
    "the user-selected home grant must remain read-only",
  );
  requireConfig(
    !entitlements.includes("com.apple.security.files.user-selected.read-write"),
    "the App Store build must not gain write access to the selected home directory",
  );
}

checkAppStoreConfig();

console.log("Release config check: private APIs off, native-only tray, selected home read-only.");
