/**
 * The English copy, and the shape the Turkish file is held to.
 *
 * Same discipline as `ui/src/i18n/en.ts`: `Copy` is derived from this object, so a key added
 * here fails the Turkish build until it is translated.
 *
 * Every claim on this page has to be true of the shipped code. Where a number appears, it is a
 * number CI asserts (`core/tests/perf.rs`) or a decision recorded in `docs/V2_PLAN.md` — not a
 * marketing round figure.
 */

export const en = {
  lang: "en",
  locale: "en",

  meta: {
    title: "Quota Deck — see your AI coding quotas",
    description:
      "A menu bar app that reads the session logs your AI coding tools already write, shows how much of each rolling quota window you have used, and on macOS watches your Mac and Docker too. Open source. No account, no login, no network requests.",
  },

  nav: {
    home: "Quota Deck",
    download: "Download",
    privacy: "Privacy",
    skip: "Skip to content",
    /**
     * Both languages are always on screen and the current one is marked, so the way out of a
     * language you cannot read is visible rather than something to be worked out. Each entry
     * links to the same page in the other language, not to its home page.
     */
    language: { label: "Language", en: "English", tr: "Türkçe", enShort: "EN", trShort: "TR" },
    /** The panel has the same three choices in its own settings; the words match. */
    theme: { label: "Theme", system: "Follow the system", light: "Light", dark: "Dark" },
  },

  hero: {
    title: "See your AI coding quotas.",
    lede: "Quota Deck reads the session logs your coding tools already write to your own disk, and shows how much of each rolling window you have used — before the tool stops answering.",
    claim: "No account. No login. No network. Ever.",
    claimNote:
      "No HTTP client is permitted in a shipping desktop dependency tree; CI checks macOS, Windows and Linux separately. The signed macOS build carries no network entitlement.",
    download: "Download",
    how: "How it can be true",
    shot: {
      src: "/popover.png",
      alt: "The Quota Deck popover: Mac health, Claude and Codex quota, CPU, memory, disk, Docker, fan, power, network and a thermal strip.",
      caption: "The popover, showing sample data. No real usage or path is in this image.",
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
      {
        title: "Your Mac, too",
        body: "On macOS: CPU, GPU, SSD and SoC temperatures, fan, power, memory pressure, disk, network, processes and a health score — read-only, and nothing a Mac does not expose is ever estimated.",
      },
      {
        title: "Docker, read-only",
        body: "Containers grouped by Compose project with health, CPU and memory, through docker info, ps, stats and system df only. It never starts, stops or removes anything.",
      },
    ],
  },

  trust: {
    title: "Measured, or clearly estimated",
    body: "Some tools write their real remaining percentage into their own logs. Some do not. The panel never blurs the two: a filled blue mark means the tool stated that number itself, and a hollow ring means we worked it out from token counts and the plan you picked. Nothing is guessed silently — a tool with no plan set shows no percentage at all.",
    measured: "measured — the tool reported this",
    estimated: "estimated — worked out on this machine",
    /**
     * The panel's row, drawn on the page in the same four columns and the same ink. Someone
     * who reads it here can read the app before installing it — and it is drawn in CSS, so it
     * follows whichever theme the reader picked rather than being a second screenshot.
     */
    anatomy: {
      title: "One row, four columns",
      lede: "A tool can report several windows at once. Each gets a row of the same shape, because a weekly ceiling stops the work exactly as hard as a five-hour one.",
      sample: {
        tool: "Codex",
        window: "5-hour window",
        percent: "78%",
        countdown: "2h 14m",
        caption: "Sample values. The bar takes colour from the level, and nothing else on the row does.",
      },
      columns: [
        { label: "Source", body: "Filled means the tool stated this number. Hollow means it was worked out here." },
        { label: "Level", body: "How full the window is. The one place colour is allowed to mean something." },
        { label: "Used", body: "The same figure in numbers, set in tabular figures so it cannot shift the row as it ticks." },
        { label: "Resets in", body: "When the window slides far enough for the work to continue." },
      ],
    },
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
        body: "The signed macOS build carries no com.apple.security.network.client entitlement. On Linux, where no entitlement backs the claim, the packaging script checks the dependency tree instead.",
      },
      {
        title: "Credentials are never read",
        body: "The Keychain and the Windows Credential Manager are never opened. Provider auth files are never opened, listed, or probed for existence — a rule CI also greps for.",
      },
      {
        title: "Reads are read-only",
        body: "Session and telemetry logs and the Mac's sensors, never written to. The optional Claude Code status line is changed only with your consent, chained to what was there, and revertible in one click.",
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
      { name: "Antigravity", detail: "Measured: when a model runs out and when it reopens, from the CLI's own log. Weekly percentages if you turn on live quota, which runs agy." },
      { name: "Copilot CLI", detail: "Exact ceiling from the published plan allowance; the spend is command-line sessions only." },
    ],
  },

  platforms: {
    title: "Three desktops",
    items: [
      {
        name: "macOS",
        body: "A menu bar item with no dock icon and a Liquid Glass popover. Signed with a Developer ID and notarized by Apple. The Mac monitor, Docker and fan modes are macOS only.",
      },
      {
        name: "Windows",
        body: "A taskbar tray item for the AI quota side, built from source with the Tauri bundler. Launch at sign-in is controlled in the app.",
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
        q: "Why does macOS ask for approval when I pick Cool or Max?",
        a: "Fan speed can only be set by a privileged helper. It is installed the first time you ask for it — never at install — and macOS asks you once. It writes fan keys and nothing else, and hands the fans back to Auto when the app quits, the Mac sleeps or the app stops checking in.",
      },
      {
        q: "Why is a number sometimes an estimate?",
        a: "Because not every tool publishes its remaining quota. Where one does, the panel shows the measurement. Where one does not, it shows an estimate and says so on the same line. It never shows one dressed as the other.",
      },
      {
        q: "What does it cost?",
        a: "Nothing. It is open source under GPL-3.0. A tool that tells you what you already paid for should not bill you again.",
      },
      {
        q: "Is the sample data real?",
        a: "No, and the app says so while it is on. It exists so the app can be seen working on a machine with no supported tool installed — an empty panel is indistinguishable from a broken one. The menu bar keeps reporting your real usage the whole time.",
      },
    ],
  },

  download: {
    title: "Download",
    lede: "Releases are published on GitHub. The macOS download is a signed, notarized DMG; Windows and Linux are built from source.",
    statusPending: "Not released yet",
    copyCommand: "Copy",
    copiedCommand: "Copied",
    build: "Build it yourself",
    buildLede:
      "The repository builds on all three. Rust and Node are the only prerequisites; the Linux build additionally needs the WebKitGTK and AppIndicator development packages, which the script names.",
    items: [
      {
        name: "macOS",
        detail: "A Developer ID-signed, notarized DMG on GitHub Releases, with the fan helper inside.",
        blocked: "Waiting on: the first tagged release.",
        command: "TEAM_ID=... scripts/release-macos.sh",
      },
      {
        name: "Windows",
        detail: "An NSIS installer from the Tauri bundler, AI quota only.",
        blocked: "No signed installer is published; build it yourself.",
        command: "npm --prefix ui exec tauri -- build",
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
        body: "Nothing. There is no HTTP client in any shipping desktop dependency tree, and CI checks the macOS, Windows and Linux builds separately. The signed macOS build carries no network entitlement. The one exception is yours to make: Antigravity live quota, off by default, runs agy, and agy talks to Google with your account.",
      },
      {
        title: "What is read",
        body: "The session and telemetry log files that supported tools write to your home folder are read-only, from a byte offset rather than from the beginning. On macOS, the Mac's sensors (SMC, IOReport, HID) and the docker CLI's info, ps, stats and system df output are read as well. To inspect the optional Claude Code status-line integration, ~/.claude/settings.json is also opened and statusLine.command is used. Before either setup flow changes, or asks you to change, that object, the complete prior statusLine value is stored in Quota Deck's local data directory for exact restoration.",
      },
      {
        title: "What is never read",
        body: "The Keychain. The Windows Credential Manager. Any provider auth file — auth.json, credentials.json, .credentials — which are never opened, never listed, and never probed for existence.",
      },
      {
        title: "What is written",
        body: "Hourly usage totals, a week of one-minute Mac readings, your settings and — after you choose to connect the optional integration — the complete prior statusLine value are written inside the app's own data directory. The fan helper, once you approve it, writes fan speed keys to the SMC and nothing else.",
      },
      {
        title: "Analytics, crash reports, telemetry",
        body: "None of the three. There is no endpoint for them to go to.",
      },
      {
        title: "Open source",
        body: "Every line of this is on GitHub under GPL-3.0, so each claim on this page can be checked rather than trusted.",
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
