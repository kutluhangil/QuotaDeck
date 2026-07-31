/**
 * The English copy, and the shape the Turkish file is held to.
 *
 * Same discipline as `ui/src/i18n/en.ts`: `Copy` is derived from this object, so a key added
 * here fails the Turkish build until it is translated.
 *
 * Every claim on this page has to be true of the shipped code. Where a number appears, it is a
 * number CI asserts (`core/tests/perf.rs`) or a decision recorded in `docs/STORE.md` — not a
 * marketing round figure.
 */

export const en = {
  lang: "en",
  locale: "en",

  meta: {
    title: "Quota Deck — see your AI coding quotas",
    description:
      "A menu bar app that reads the session logs your AI coding tools already write, and shows how much of each rolling quota window you have used. No account, no login, no network requests.",
  },

  nav: {
    home: "Quota Deck",
    download: "Download",
    privacy: "Privacy",
    /** Each language names itself, so someone who lands in the wrong one can get out. */
    otherLanguage: { href: "/tr/", label: "Türkçe" },
  },

  hero: {
    title: "See your AI coding quotas.",
    lede: "Quota Deck reads the session logs your coding tools already write to your own disk, and shows how much of each rolling window you have used — before the tool stops answering.",
    claim: "No account. No login. No network. Ever.",
    claimNote:
      "The app ships without the entitlement that would allow an outbound connection, so that line is enforced by its code signature rather than asserted on this page.",
    download: "Download",
    how: "How it can be true",
    shot: {
      src: "/panel.png",
      alt: "The Quota Deck panel showing two tools, each with its five-hour, weekly and pace rows.",
      caption: "The panel, showing the sample deck. No real usage or path is in this image.",
    },
  },

  features: {
    title: "What it does",
    items: [
      {
        title: "Quiet until it isn't",
        body: "The menu bar item is a single monochrome glyph. It only takes colour past 85% — an item that is permanently lit is the reason people remove menu bar apps.",
      },
      {
        title: "Every limit, one row",
        body: "A tool can report several windows at once: five hours, a week, a month. Each gets a row of the same four columns, because a weekly ceiling stops the work exactly as hard as a five-hour one.",
      },
      {
        title: "The Horizon",
        body: "A quota is a tide, not a battery. Usage enters at the right, slides left, and falls off the window edge as the quota comes back. One strip per tool that shows where it went and teaches the rolling window at the same time.",
      },
      {
        title: "A forecast that says what it is",
        body: "The pace row is drawn on a hollow track and labelled as a projection, never as a reading. Presenting a guess as a measurement is the fastest way to lose someone in this category.",
      },
      {
        title: "Warnings you chose",
        body: "A notification when a limit crosses a level you picked, once per limit per window, mutable for an hour or until tomorrow.",
      },
      {
        title: "A month of history, here",
        body: "Hourly totals and equivalent API cost, kept on this device for a month and nowhere else.",
      },
    ],
  },

  trust: {
    title: "Measured, or clearly estimated",
    body: "Some tools write their real remaining percentage into their own logs. Some do not. The panel never blurs the two: a filled blue mark means the tool stated that number itself, and a hollow ring means we worked it out from token counts and the plan you picked. Nothing is guessed silently — a tool with no plan set shows no percentage at all.",
    measured: "measured — the tool reported this",
    estimated: "estimated — worked out on this machine",
  },

  privacy: {
    title: "How it can be true",
    lede: "The privacy claim is not a policy. It is four things about the build, each of which fails CI if it stops being true.",
    items: [
      {
        title: "No HTTP client in the dependency tree",
        body: "Not reqwest, not hyper, not ureq. CI greps the dependency tree on every push and fails the build if one appears.",
      },
      {
        title: "No outbound-connection entitlement",
        body: "The macOS entitlements file does not carry com.apple.security.network.client. On Linux, where no entitlement backs the claim, the packaging script checks the dependency tree instead.",
      },
      {
        title: "Credentials are never read",
        body: "The Keychain and the Windows Credential Manager are never opened. Provider auth files are never opened, listed, or probed for existence — a rule CI also greps for.",
      },
      {
        title: "Reads are read-only",
        body: "Session and telemetry logs, never written to. The one write outside our own folder is the opt-in Claude Code status line shim, which shows the exact before and after, chains whatever command you already had, and reverts in one click.",
      },
    ],
    why: {
      title: "Why it was built this way",
      body: "The obvious architecture — read a provider's OAuth token out of the Keychain and ask its usage API — breaks the consumer terms of every provider it touches. Anthropic enforced exactly that in January 2026 and third-party tools stopped working overnight. The risk of that approach does not land on the developer; it lands on the accounts of the people who paid for the app. Local log files are the user's own data on the user's own disk, and there is no request to make.",
    },
  },

  performance: {
    title: "The budget is a feature",
    lede: "This category has a reputation for battery drain, so the limits are asserted in CI rather than hoped for. A red budget blocks the merge.",
    columns: { metric: "What", budget: "Budget", measured: "Measured" },
    note: "Measured on a 160 MB synthetic corpus of 500 files. The memory figure is the reader's own peak, not the whole app — a Tauri app carries a system WebView, which is what the 60 MB ceiling is for.",
    rows: [
      { metric: "Cold parse, 160 MB of logs", budget: "3 s", measured: "65 ms" },
      { metric: "Tick across 500 open cursors", budget: "20 ms", measured: "3 ms" },
      { metric: "Disk read per hour of watching", budget: "5 MB", measured: "65 KB" },
      { metric: "Peak resident memory", budget: "60 MB", measured: "7.3 MB" },
      { metric: "Network, ever", budget: "0 bytes", measured: "0 bytes" },
    ],
  },

  providers: {
    title: "Tools it reads",
    lede: "A tool that is not installed reports exactly that. Nothing is synthesised for it, and no percentage is invented for a tool that has not reported one.",
    shipping: "Reading now",
    plannedNote:
      "Each new tool is one file, one fixture test and one line of registration. They are listed here only once their log format has been confirmed against a real file.",
    list: [
      { name: "Claude Code", detail: "Measured, once the status line is connected. Estimated before that." },
      { name: "Codex", detail: "Measured. Codex writes its real rate limits into its own logs." },
      { name: "Copilot CLI", detail: "Exact ceiling from the published plan allowance; the spend is command-line sessions only." },
    ],
  },

  platforms: {
    title: "Three desktops",
    items: [
      {
        name: "macOS",
        body: "A menu bar item with no dock icon. Sandboxed for the Mac App Store, which means one folder grant, asked for once, in the system's own panel.",
      },
      {
        name: "Windows",
        body: "A taskbar tray item. MSIX through the Microsoft Store, so the Store signs it and updates come through the Store.",
      },
      {
        name: "Linux",
        body: "A tray indicator, as .deb, .rpm and AppImage. No Flathub or Snap: neither buys anything here, since there is no sandbox grant to declare and no network capability to justify.",
      },
    ],
    caveat: {
      title: "What Linux does differently",
      body: "The protocol behind the Linux tray carries no click event and no icon position. So the left button opens the menu, the menu's own entry opens the panel, and the panel is placed at the top right rather than under an item nobody reports the position of. On KDE, whose tray sits bottom right by default, the panel does not follow it.",
    },
  },

  faq: {
    title: "Questions",
    items: [
      {
        q: "Does it need my API key?",
        a: "No. There is nowhere to enter one, and no request it could be used for.",
      },
      {
        q: "Will this get my provider account suspended?",
        a: "No — that risk comes from tools that authenticate as you against a provider's API. This app never authenticates and never connects. It reads files your own tools wrote to your own disk.",
      },
      {
        q: "Why does macOS ask for my home folder?",
        a: "Because a sandboxed app can read nothing outside its own container until you hand it something. It is one grant, asked for once, in the system's own panel, and it can be revoked from Settings at any time. Nothing leaves the device either way.",
      },
      {
        q: "Why is a number sometimes an estimate?",
        a: "Because not every tool publishes its remaining quota. Where one does, the panel shows the measurement. Where one does not, it shows an estimate and says so on the same line. It never shows one dressed as the other.",
      },
      {
        q: "Subscription?",
        a: "No. One-time purchase. A tool that tells you what you already paid for should not bill monthly.",
      },
      {
        q: "Is the sample data real?",
        a: "No, and the app says so while it is on. It exists so the app can be seen working on a machine with no supported tool installed — an empty panel is indistinguishable from a broken one. The menu bar keeps reporting your real usage the whole time.",
      },
    ],
  },

  download: {
    title: "Download",
    lede: "Nothing is released yet. The three build routes are done and run in CI; what is left needs a developer account and a machine of each kind, and neither is something a commit can produce.",
    statusPending: "Not released yet",
    build: "Build it yourself",
    buildLede:
      "The repository builds on all three. Rust and Node are the only prerequisites; the Linux build additionally needs the WebKitGTK and AppIndicator development packages, which the script names.",
    items: [
      {
        name: "macOS",
        detail: "Mac App Store, as a signed .pkg. Sandboxed, with the folder grant asked for once.",
        blocked: "Waiting on: Apple Distribution certificate and an App Store Connect submission.",
        command: "TEAM_ID=... scripts/appstore.sh",
      },
      {
        name: "Windows",
        detail: "Microsoft Store, as an MSIX bundle for x64 and arm64. The Store signs it, so no certificate has to be bought.",
        blocked: "Waiting on: a Partner Center account and a reserved product name.",
        command: "scripts/msstore.ps1",
      },
      {
        name: "Linux",
        detail: ".deb and .rpm for the two package-manager families, plus an AppImage that runs without root on a distribution covered by neither.",
        blocked: "Waiting on: a hand-run on a real desktop session. CI builds and tests it on every push.",
        command: "scripts/linux.sh",
      },
    ],
    sourceTitle: "Source",
    sourceBody: "The repository, including the blueprint the whole thing was built from.",
    sourceLink: "github.com/kutluhangil/QuotaDeck",
    sourceHref: "https://github.com/kutluhangil/QuotaDeck",
  },

  privacyPage: {
    title: "Privacy",
    lede: "Quota Deck collects nothing, because there is nothing in it that could. This page is the long version of that sentence.",
    sections: [
      {
        title: "What leaves your device",
        body: "Nothing. There is no HTTP client in the dependency tree and no entitlement that would permit an outbound connection, so this is not a promise about intent — it is a property of the binary, and CI fails the build if either changes.",
      },
      {
        title: "What is read",
        body: "The session and telemetry log files that supported tools write to your home folder, read-only, from a byte offset rather than from the beginning. Nothing else in your home folder is opened.",
      },
      {
        title: "What is never read",
        body: "The Keychain. The Windows Credential Manager. Any provider auth file — auth.json, credentials.json, .credentials — which are never opened, never listed, and never probed for existence.",
      },
      {
        title: "What is written",
        body: "Hourly usage totals and your settings, inside the app's own data directory. The single exception is the Claude Code status line shim, which you turn on yourself after seeing the exact before and after; it chains whatever command you already had rather than replacing it, and one click puts it back.",
      },
      {
        title: "Analytics, crash reports, telemetry",
        body: "None of the three. There is no endpoint for them to go to.",
      },
      {
        title: "The store declaration",
        body: "Data Not Collected, on every question in App Store Connect's privacy questionnaire and its Partner Center equivalent.",
      },
    ],
  },

  footer: {
    tagline: "Local, offline, and quiet about it.",
    source: "Source",
    privacy: "Privacy",
  },
};

export type Copy = typeof en;
