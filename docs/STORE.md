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
