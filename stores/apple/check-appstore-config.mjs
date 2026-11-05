import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
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
      value: JSON.parse(read("stores/apple/tauri.appstore.conf.json")),
    },
    {
      name: "Microsoft Store override",
      value: JSON.parse(read("stores/microsoft/tauri.msstore.conf.json")),
    },
    { name: "Linux override", value: JSON.parse(read("app/tauri.linux.conf.json")) },
  ];
  const appStore = configs[1].value;
  const cargo = read("app/Cargo.toml");
  const entitlements = read("stores/apple/Entitlements.appstore.plist");

  // The direct (GitHub) build turns on macOSPrivateApi and a transparent popover in the base
  // config. Only what the App Store override switches back off is checked here; the Cargo
  // feature follows `system-monitor`, which the store build disables.
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
  // one here would silently ship inside the store build. `stores/apple/STORE.md` §9 says the GUI does
  // not install the command line; `quotadeck-cli` is a separate crate to keep that true.
  requireConfig(
    (cargo.match(/^\[\[bin\]\]/gm) ?? []).length === 1,
    "app/Cargo.toml must declare exactly one [[bin]]; a second one ships inside the App Store bundle",
  );
  const appStoreScript = read("stores/apple/appstore.sh");
  requireConfig(
    /^system-monitor = \["dep:quotadeck-sysmon", "tauri\/macos-private-api"\]$/m.test(cargo) &&
      /quotadeck-sysmon = \{[^}]*optional = true/.test(cargo),
    "the system monitor must stay an optional feature of app/Cargo.toml",
  );
  requireConfig(
    appStoreScript.includes("-- --no-default-features"),
    "stores/apple/appstore.sh must build without default features, or the system monitor ships in the store",
  );
  requireConfig(
    entitlements.includes("com.apple.security.files.user-selected.read-only"),
    "the user-selected home grant must remain read-only",
  );
  requireConfig(
    !entitlements.includes("com.apple.security.files.user-selected.read-write"),
    "the App Store build must not gain write access to the selected home directory",
  );

  // The widget extension. Its entitlements are checked separately from the host's because
  // `--deep` signing would have given it the host's, which is exactly the mistake this guards.
  const widgetEntitlements = read("stores/apple/widget/Entitlements.widget.plist");
  requireConfig(
    !/\.network\./.test(widgetEntitlements),
    "the widget entitlements request a network capability",
  );
  requireConfig(
    !widgetEntitlements.includes("com.apple.security.files.user-selected"),
    "the widget must not request file access; it reads only the app group container",
  );

  const appGroupOf = (plist) => {
    const match = plist.match(
      /application-groups<\/key>\s*<array>\s*<string>([^<]+)<\/string>/,
    );
    return match ? match[1] : null;
  };
  requireConfig(
    appGroupOf(entitlements) !== null &&
      appGroupOf(entitlements) === appGroupOf(widgetEntitlements),
    "the host and the widget must declare the same app group, or neither can see the snapshot",
  );

  const valueOf = (plist, key) => {
    const match = plist.match(
      new RegExp(`<key>${key}</key>\\s*<string>([^<]+)</string>`),
    );
    return match ? match[1] : null;
  };
  const widgetInfo = read("stores/apple/widget/Info.plist");
  requireConfig(
    valueOf(widgetInfo, "CFBundleIdentifier") === "com.kutluhangil.quotadeck.widget",
    "the widget bundle identifier must be the host identifier plus .widget",
  );
  requireConfig(
    valueOf(widgetInfo, "QuotaDeckAppGroup") === appGroupOf(widgetEntitlements),
    "the widget's Info.plist names a different app group than its entitlements",
  );
}

checkAppStoreConfig();

console.log("Release config check: private APIs off, native-only tray, selected home read-only.");
