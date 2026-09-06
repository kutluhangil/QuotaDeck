// The config gate's own tests.
//
// Every assertion here is negative: the gate passing on the real repository proves nothing on
// its own, because a check that always passes also always passes. Each test breaks exactly one
// file and asserts the gate names it.
//
// Run: node --test scripts/

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
import { test } from "node:test";

import { checkAppStoreConfig } from "./check-appstore-config.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/** The real files, with the named ones replaced. */
function readerWith(overrides) {
  return (path) =>
    Object.hasOwn(overrides, path)
      ? overrides[path]
      : readFileSync(join(root, path), "utf8");
}

const WIDGET_ENTITLEMENTS = "app/widget/Entitlements.widget.plist";
const WIDGET_INFO = "app/widget/Info.plist";

test("the unmodified repository passes", () => {
  checkAppStoreConfig(readerWith({}));
});

test("a widget that can reach the network is refused", () => {
  const original = readFileSync(join(root, WIDGET_ENTITLEMENTS), "utf8");
  const read = readerWith({
    [WIDGET_ENTITLEMENTS]: original.replace(
      "<key>com.apple.security.app-sandbox</key>",
      "<key>com.apple.security.network.client</key>\n  <true/>\n  <key>com.apple.security.app-sandbox</key>",
    ),
  });
  assert.throws(() => checkAppStoreConfig(read), /network/);
});

test("a widget that asks for file access is refused", () => {
  const original = readFileSync(join(root, WIDGET_ENTITLEMENTS), "utf8");
  const read = readerWith({
    [WIDGET_ENTITLEMENTS]: original.replace(
      "<key>com.apple.security.app-sandbox</key>",
      "<key>com.apple.security.files.user-selected.read-only</key>\n  <true/>\n  <key>com.apple.security.app-sandbox</key>",
    ),
  });
  assert.throws(() => checkAppStoreConfig(read), /file access/);
});

test("a widget in a different app group than the host is refused", () => {
  const original = readFileSync(join(root, WIDGET_ENTITLEMENTS), "utf8");
  const read = readerWith({
    [WIDGET_ENTITLEMENTS]: original.replace(
      "$TEAM_ID.com.kutluhangil.quotadeck.shared",
      "$TEAM_ID.com.kutluhangil.somethingelse",
    ),
  });
  assert.throws(() => checkAppStoreConfig(read), /same app group/);
});

test("a widget bundle identifier that is not the host's plus .widget is refused", () => {
  const original = readFileSync(join(root, WIDGET_INFO), "utf8");
  const read = readerWith({
    [WIDGET_INFO]: original.replace(
      "com.kutluhangil.quotadeck.widget",
      "com.kutluhangil.quotadeck.extension",
    ),
  });
  assert.throws(() => checkAppStoreConfig(read), /bundle identifier/);
});

test("an Info.plist naming a different app group than the entitlements is refused", () => {
  const original = readFileSync(join(root, WIDGET_INFO), "utf8");
  const read = readerWith({
    [WIDGET_INFO]: original.replace(
      "<string>$TEAM_ID.com.kutluhangil.quotadeck.shared</string>",
      "<string>$TEAM_ID.com.kutluhangil.other.shared</string>",
    ),
  });
  assert.throws(() => checkAppStoreConfig(read), /different app group/);
});
