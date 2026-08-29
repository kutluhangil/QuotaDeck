# Store submission

What the listings say, why they say it that way, and the parts that need an account rather than
a commit. Blueprint §8 is the decision record; this is the copy and the checklist.

The account-side work is written out step by step, in order, in `docs/RELEASE_CHECKLIST.md`.
This file is the reference it points at.

## 1. Naming — the rejection this avoids

Apple's metadata rule is specific: putting another company's product name in your title to catch
its search traffic is a documented App Review rejection. Brand names may appear in the
description as references ("works with…"), never in the name, subtitle, icon or keywords.
CUStats learned this the expensive way — its v1.8.0 release note reads "Fix App Store metadata
compliance — restricted third-party and Apple terms removed."

| | Never | Ship |
|---|---|---|
| Name | "Claude Usage Tracker" | **Quota Deck** |
| Subtitle | "Claude & Codex limits" | "See your AI coding quotas" |
| Icon | Provider logos | The Horizon glyph |
| Keywords | Brand stuffing | quota, usage, tokens, menu bar, developer tools |
| Screenshots | Provider logos | Letter-spaced text labels only |

The same rule holds **inside** the app. Providers are set as letter-spaced uppercase text, never
as vendor marks (§7.3) — legally clean, and the reason the design language is consistent.

## 2. Description

> Quota Deck reads the session logs that AI coding tools already write to your disk and shows how
> much of each rolling quota window you have used.
>
> No account. No login. No network requests — the desktop dependency tree contains no HTTP
> client and CI checks the macOS, Windows and Linux builds. The Mac App Store signature also
> carries no outbound-network entitlement.
>
> • A menu bar item that stays quiet until a quota is genuinely at risk
> • A timeline of where the quota went, per tool
> • A pace forecast that says what it is: a projection, never a reading
> • Threshold warnings you choose, once per limit per window
> • A month of history on this device, and nowhere else
>
> Works with the local session logs of Claude Code, Codex and Copilot CLI. Measured limits where
> the tool reports one; a clearly marked estimate where it does not.

Every number in the app carries a confidence badge. That is the product, not a detail: presenting
a guess as a measurement is the fastest way to lose a user in this category.

## 3. Privacy — "Data Not Collected"

Declare **Data Not Collected** on every question in App Store Connect's privacy questionnaire,
and the equivalent in Partner Center. This is true, and the architecture is what makes it
checkable:

- No HTTP client in any shipping desktop dependency tree. CI checks macOS, Windows and Linux.
- No outbound-connection entitlement in `app/Entitlements.plist`. CI fails if one is added.
- No listening socket — the OTLP telemetry route was tested and rejected in Phase 0
  (`docs/DISCOVERY.md` §4) partly for this reason.
- Keychain and Credential Manager are never read. Provider auth files are never opened, listed,
  or probed for existence. CI greps for this.
- Reads are limited to session and telemetry logs plus `statusLine.command` in Claude Code's
  settings for the optional integration; provider credential files remain excluded.
- The App Store build never writes outside its own container. Its Claude Code status line setup
  shows and copies the exact chained command, but the user applies it manually; the home-folder
  entitlement remains read-only. After the user asks to copy the manual setup JSON, the complete
  prior `statusLine` object is stored inside the app container so the exact manual restore JSON
  remains available. Unsandboxed builds can apply and revert the same change only after explicit
  consent, keeping the same local snapshot for exact one-click restoration.

## 4. Pricing

One-time purchase, $7.99–$12.99. No subscription — a tool that tells you what you already paid
for should not bill monthly. There is no server and no account, so all digital sales go through
Apple IAP / Microsoft IAP; an external purchase link is a rejection.

The sample deck is required, not a nicety. Someone evaluating the app on a machine with no
supported tool installed sees an empty panel, and an empty panel is indistinguishable from a
broken app. Settings → Sample deck, and offered directly from the empty state.

## 5. Mac App Store

The sandbox story is Phase 9 and it is already built: `scripts/sandbox-check.sh` signs an ad-hoc
bundle with the shipping entitlements and asserts the four things the sandbox changes. It runs in
CI on every push. What is left needs an Apple account:

- [ ] Apple Distribution certificate in the login keychain
- [ ] 3rd Party Mac Developer Installer certificate in the login keychain
- [ ] Mac App Store provisioning profile for `com.kutluhangil.quotadeck`, saved as
      `app/MacAppStore.provisionprofile`
- [ ] App Store Connect API key exported as `APPLE_API_KEY` / `APPLE_API_ISSUER`
- [ ] `TEAM_ID=... scripts/appstore.sh`

`scripts/appstore.sh` verifies the two things that fail late and expensively: that the sandbox
entitlement actually made it into the signature (Asset Validation error 90296 is what its absence
looks like from Apple's side), and that no network capability crept in.

The `.app` is never uploaded directly — the store takes a `.pkg`.

## 6. Microsoft Store

Quota Deck uses Microsoft's unpackaged Win32 route: signed NSIS `.exe` installers for x64 and
arm64. Tauri's built-in bundle targets do not include MSIX, so the previous `--bundles msix`
command could never produce the package it promised. The Store accepts MSI or EXE installers
through versioned HTTPS URLs, but it requires the installer and every portable executable it
installs to carry a valid CA-backed code signature. The Store does not add that signature for
this route.

`scripts/msstore.ps1` uses the repository-pinned Tauri CLI, reads the installed signing identity
from `WINDOWS_CERTIFICATE_THUMBPRINT`, timestamps the signature, builds both architectures with
the offline WebView installer, locates exactly one NSIS artifact per target, and verifies both
the installer and application executable. `-AllowUnsignedLocalBuild` is only for local testing.
Partner Center's silent-install flag is `/S` (capital S).

Left for Partner Center:

- [ ] Reserve the product name. The publisher display name may not equal the product name.
- [ ] Configure Tauri Windows signing with a CA-issued code-signing certificate
- [ ] Host both signed installers at immutable, versioned HTTPS URLs
- [ ] Submit the x64 and arm64 URLs as EXE installers with silent switch `/S`
- [ ] Listing from §2 and §3 above

### Launch at sign-in

The Windows settings screen includes an explicit Launch at sign-in choice. It writes only the
current user's `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value after the user turns
it on, reads the exact executable path back for verification, and removes the value when turned
off. Nothing is registered during installation or first launch. A real Windows runtime check is
still required before submission.

## 7. Linux

No store. Flathub and Snap each want an account and a review queue, and neither buys anything
here: there is no sandbox grant to declare, no network capability to justify, and no update
channel the two package managers do not already provide. `scripts/linux.sh` builds all three
formats — `.deb` and `.rpm` for the families that install and update through a package manager,
and an AppImage as the single file that runs on a distribution covered by neither, without root.

Runtime dependencies are declared in `app/tauri.linux.conf.json`: `libwebkit2gtk-4.1-0` for the
webview, `libgtk-3-0`, and `libayatana-appindicator3-1` for the tray. The last one is not
optional — without it the item is not drawn at all on desktops using the StatusNotifierItem
protocol, which is most of them.

Three platform differences the listing should not promise away:

- **No click on the tray icon.** The protocol carries no click event, so the left button opens
  the menu and the menu's first entry opens the panel.
- **No icon geometry.** Nothing can be positioned under the item, so the panel is placed at the
  top right — where the indicator area sits on GNOME, Cinnamon, Budgie and XFCE. KDE's default
  tray is bottom right, and the panel does not follow it.
- **Transparency needs a compositor.** Without one the panel's rounded corners fall back to
  opaque. Every current desktop composites by default; a bare window manager does not.

Verified in CI on `ubuntu-latest` (compile, clippy, tests, perf budget). Not yet run by hand on
a real desktop session — that needs a Linux machine.

## 8. Screenshots

Six per platform, all from the sample deck so no real usage or path is shown:

1. The menu bar item, at rest — the glyph with no colour
2. The panel, two tools reporting, one at risk
3. The Horizon strip with a slice under the cursor
4. The dashboard, week range, heatmap visible
5. Settings — the confidence explanation and the status line before/after
6. A threshold notification

No provider logos in any of them (§1).

## 9. The command line — `quotadeckctl`

A **separate artifact**, and a separate crate. It is not inside the Mac App Store `.app` or the
`.pkg`, the GUI does not install it, and the store listing must not promise it: an App Store
application cannot put an executable on `PATH`, and claiming otherwise is a review rejection as
well as an untruth. Anyone who wants it builds it from this repository:

```
cargo build --release -p quotadeck-cli --bin quotadeckctl
```

`quotadeck-cli` is its own crate rather than a second `[[bin]]` in `quotadeck-app` because the
Tauri bundler copies **every** binary of the packaged crate into `Contents/MacOS`. While the
command line lived in the app crate the paragraph above was false, and nothing said so.
`scripts/check-appstore-config.mjs` now fails the build if `app/Cargo.toml` grows a second
`[[bin]]`, so the sentence stays true without anyone having to remember it.

It adds no capability. It reads the same local files the panel reads, writes nothing outside the
app's own data directory, and opens no socket.

### Commands

```
quotadeckctl providers                        compiled providers, their level and roots
quotadeckctl status [--provider <key>] [--plan <id>]
                                              parse the logs and print every window
quotadeckctl export [--json|--csv] [--provider <key>] [--from <RFC3339> --to <RFC3339>]
                                              the deck to stdout
quotadeckctl config show                      the stored settings, as they are on disk
quotadeckctl config validate                  resolve them against this build's registry
quotadeckctl guard                            resolved home, data directory, per-root access
quotadeckctl tray <key>                       draw the menu bar item for that provider
quotadeckctl statusline preview|install|revert
```

`config` is read-only; the panel owns every settings write, and a second writer would race it.
`statusline install` and `statusline revert` are spelled out rather than defaulted into, and both
refuse to run inside the App Sandbox (§5) — there the panel shows a copyable command instead.
`guard` is what `scripts/sandbox-check.sh` compares between a sandboxed and an unsandboxed run.

Data goes to stdout and diagnostics to stderr, on every command, so a warning never lands in the
file the caller is writing. A reader that hangs up early — `export --csv | head` — is that
reader's decision, not a failure of the export: the broken pipe is absorbed and the quota status
is still what the process reports.

### Exit codes

`export` reports the deck's worst reading through its exit status, so a shell can branch on the
quota without parsing anything.

| Code | Meaning |
|---|---|
| `0` | ok — every window was read and none is near its limit |
| `10` | near the limit — at least one window at or past 90%, the same point `PaceRisk` calls at risk |
| `11` | limit hit — at least one window reporting 100% or more |
| `20` | indeterminate — nothing reported a percentage, or the first pass had not finished |
| `1` | the command itself failed: a refused argument, an unreadable settings file, an unknown or disabled provider key, a scan error |

`20` is deliberately not `0`. A machine with no supported tool installed, or one whose logs are
not readable from a sandboxed process, has no reading to give — and a script that read that as
"plenty left" would be wrong in exactly the case the user most needs to know about.

### The JSON schema

The JSON export's first field is `schemaVersion`, currently `1`. It is bumped only when a
consumer would have to change: a field removed, renamed, or given a different meaning. A new
optional field does not bump it. A script that reads the number knows which contract it is
holding; one that cannot find it is reading an export written before there was a contract.

Version `1` carries `schemaVersion`, `updatedAt`, `scanning`, `providers`, `health`, `retention`
and `history`. `health` is there because a tool that could not be read must not be
indistinguishable from one that was idle, and `retention` because how far back the numbers go is
part of reading them.

Both writers take the same snapshot the panel renders:

```
quotadeckctl export --json                  the whole deck and its retained history
quotadeckctl export --csv                   one row per hour, per dimension, per label
quotadeckctl export --csv --provider codex  one tool only
```

The CSV leaves the cost cell empty where nothing in the row carried a price, rather than writing
`0` — a model released after this build has no published rate, and a zero in a spreadsheet reads
as free. Its `unpricedTokens` column carries what that row spent instead. `labelsDropped` repeats
the count of labels the breakdown refused, on every row of the dimension that refused them.
