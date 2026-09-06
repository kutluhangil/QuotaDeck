# macOS Widget — Design

**Status:** accepted 2026-09-06. Blueprint Faz 13, "Menü çubuğu widget'ları".

**Goal:** answer one question from outside the app — *when can I work again* — for a user whose
quota is spent. The tray item already answers *how full is the worst provider*; a widget that
repeated it would earn nothing.

**Scope:** macOS only, App Store build only. Windows and Linux are unaffected and no code on
those platforms changes.

**Tech:** Swift 6 / WidgetKit extension, hand-assembled `.appex`; Rust writer in `app/src`;
data shared through an App Group container.

---

## 1. Why a widget at all

Blueprint §7.5 is already implemented: the menu bar item renders a glyph, a compact percentage
or a mini Horizon strip, and it is always on screen. What it cannot do is carry a countdown —
sixteen logical pixels do not hold `0s 41dk`, and the strip mode is a history, not a forecast.

`QuotaWindow::resets_at` is an absolute instant, reported by the provider as a Unix epoch. That
is exactly the input WidgetKit's `Text(_:style: .timer)` wants: the system ticks the countdown
itself, with no process of ours awake and no timeline refresh spent. The one thing the app is
badly placed to show is the one thing the platform renders for free.

## 2. Architecture

Three new parts. None of them changes an existing data path.

```
core (unchanged)
  └── app/src/widget.rs                         [new]  DeckState -> snapshot, atomic write
        └── <TeamID>.com.kutluhangil.quotadeck.shared/
              snapshot.json                     [new]  ~400 bytes
                    └── QuotaDeckWidget.appex   [new]  Swift, read-only
```

### 2.1 The writer

`app/src/widget.rs` derives the snapshot from `DeckState` on each tick, for enabled instances
in `Settings::provider_order`, and writes it into the App Group container.

Written fields, and nothing else:

| Field | Source |
|---|---|
| `instance` | `ProviderInstanceId::key` |
| `name` | the instance label, or the tool's own name |
| `used_percent` | `QuotaWindow::used_percent` |
| `resets_at` | `QuotaWindow::resets_at`, RFC3339 |
| `state` | `measured` \| `derived` \| `stale`, from `Confidence` |
| `captured_at` | when this snapshot was produced |

No log content, no file path, no model name, no cost, no token count. A widget that leaked a
path would leak it to every screenshot the user ever takes of their desktop.

Which window a provider contributes: the one closest to exhaustion, preferring windows that
carry a `resets_at` over windows that do not. Where no window carries one, the instance is
still included and its `resets_at` is null — a provider that reports a level but no reset
instant is a real state, and dropping it would make the widget quieter than the truth. A
provider with several windows is one row, not several: the widget answers one question, and a
row per window would turn it into a table.

`state` is only ever `measured`, `derived` or `stale`. `Confidence::Unavailable` has no
representation because those instances never reach the snapshot.

**Atomicity.** Write to a temporary file in the same directory, then `rename`. The widget
process may read at any moment, and a half-written JSON is an empty widget.

**Refresh.** `WidgetCenter.reloadTimelines` is called only when the set of `resets_at` instants
changes — once per window, not once per tick. Percentages move constantly and the countdown is
the system's job; reloading on every tick would spend the daily refresh budget on redrawing a
number the widget can already extrapolate.

### 2.2 The reader

The extension reads the JSON and draws it. No parsing of provider formats, no computation, no
network. It cannot wake the app and it cannot read a log; the App Group container is the only
thing it can see, and `Entitlements.widget.plist` grants nothing else.

Localization is resolved by the writer: the snapshot carries display strings already in the
user's language. The widget translates nothing, which keeps the single i18n registry in
`app/src/i18n.rs` and lets the existing catalogue parity test cover the widget's strings too.
The countdown is the exception and needs no help — `Text(style: .timer)` is localized by the
system.

### 2.3 When the app is not running

The widget still renders, because the extension is loaded by the system rather than by us. The
countdown remains correct — an absolute instant does not go stale until it passes. The
percentage does go stale, and says so: the snapshot carries `state` and `captured_at`, and the
widget marks anything the writer stamped `stale` with the same `◷` convention the panel and
tray already use. Nothing is extrapolated and nothing is invented.

## 3. What it looks like

Two families. `systemLarge` is deliberately absent — it would only be the panel, smaller.

**Small** — the instance closest to exhaustion, preferring one whose reset instant is known:

```
Codex
██████████ 94%
41:12
```

**Medium** — up to three instances in the user's own order:

```
Claude Code   ████░░ 68%     2:14:09
Codex         ██████ 94%       41:12
Copilot CLI   █░░░░░ 12%     5:02:44
```

**Colour.** The same discipline as the tray: monochrome below 85%, `--level-critical` only
above it. A widget sits on the desktop permanently, so a permanently lit widget is the same
failure as a permanently lit menu bar item, and for the same reason.

**No reset instant known.** The row shows the percentage and an em dash. A countdown is never
synthesized from `window_minutes` — an estimate presented as a clock is the exact trust failure
`CLAUDE.md` names.

**Nothing to show.** Instances that are unavailable or disabled never enter the snapshot. If
that leaves it empty, the widget says so in one sentence and links to the app, in the tone
blueprint §7.6 sets: directive, not apologetic.

**Tap** opens Quota Deck through `widgetURL`.

## 4. Build and signing

### 4.1 Producing the appex

Hand-assembled by `scripts/widget.sh`: `swiftc` produces the binary, the script lays out the
bundle around an explicit `Info.plist` carrying
`NSExtensionPointIdentifier = com.apple.widgetkit-extension`. Roughly sixty readable lines.

The alternative — committing an `.xcodeproj` — buys a well-trodden path at the cost of five
hundred lines of generated `pbxproj` that cannot be reviewed in a diff. If the hand-assembled
bundle fails to load, that is the fallback, and the decision gets recorded here rather than
quietly reversed.

### 4.2 Entitlements

| File | Change | Why |
|---|---|---|
| `app/Entitlements.plist` | **none** | `scripts/sandbox-check.sh` ad-hoc signs this and passes 9/9 without an account. A team-prefixed group key here would make the harness sign an entitlement it cannot satisfy. |
| `app/Entitlements.appstore.plist` | `+ com.apple.security.application-groups` = `$TEAM_ID.com.kutluhangil.quotadeck.shared` | `$TEAM_ID` is already substituted into a temporary copy by `scripts/appstore.sh`. No new handling of account data. |
| `app/widget/Entitlements.widget.plist` | new: sandbox, the same group, `$TEAM_ID.com.kutluhangil.quotadeck.widget` | No network, no `user-selected` files, no bookmarks. The container is all it can reach. |

On macOS an App Group identifier must carry the Team ID as its prefix — the `group.` form is
iOS. That makes the group identifier account data, which is why it uses the same `sed`
substitution as `application-identifier` rather than being committed.

The App Store build is the only one that ships the widget. Additional log folders and named
instances are excluded from that build because one security-scoped bookmark reaches one folder;
the widget is the inverse — it needs an entitlement only a signed team build can carry.

### 4.3 Signing order

`scripts/appstore.sh` currently signs once with `--deep`. `--deep` re-signs nested code with the
*host's* entitlements, which would give the appex `files.user-selected.read-only` — a
capability it has no claim to and which Asset Validation rejects for disagreeing with the
widget's own profile.

Correct order is inside-out: the appex with its own entitlements first, then the bundled
frameworks, then the host without `--deep`. This modifies a script that currently works, so
each step reports its own `codesign --verify` output rather than being declared done.

### 4.4 Gates

`scripts/check-appstore-config.mjs` gains three assertions:

1. the widget entitlements contain no `.network.` key;
2. host and appex declare the same App Group;
3. the appex bundle identifier is the host identifier plus `.widget`.

CI's existing `macos-latest` job compiles the appex unsigned, so a Swift compile error is caught
without an account. `sandbox-check.sh` is untouched and keeps passing 9/9.

## 5. Testing

- **Rust.** `widget.rs` is split so the snapshot is a pure function of `DeckState` plus
  `Settings`, tested without a filesystem: window selection, ordering, exclusion of disabled and
  unavailable instances, the absent-`resets_at` case, and the stale stamp. The write itself is
  tested against a temporary directory, including that a failed write leaves the previous
  snapshot intact.
- **Reload discipline.** A test asserts that a tick which changes only percentages requests no
  reload, and that a tick which moves a `resets_at` requests exactly one.
- **i18n.** The widget's strings enter `app/src/i18n.rs`, so the existing catalogue test covers
  all four languages with no new test.
- **Swift.** Compile is enforced by CI. Rendering is verified by hand on a signed build; there
  is no unit-testable logic in the extension by construction, because all of it was moved to the
  writer.

## 6. What blocks shipping

Not the code. `docs/USER_ACTIONS.md` §1 gains three items, all of them account work:

- register the App Group;
- register a separate App ID for the widget;
- create a provisioning profile for it — an appex embeds its own `embedded.provisionprofile`
  and is not covered by the host's.

Until those exist the extension compiles in CI but cannot be loaded, so §3's appearance stays
unverified and must be reported as such rather than assumed.

## 7. Explicitly out of scope

- `systemLarge`, Control Center, and Lock Screen surfaces.
- Any Windows or Linux equivalent.
- Reading quota data inside the extension.
- Interactive widgets (App Intents). The widget shows; the app acts.
