# Changelog

## 2026-08-25

- Pricing: apply official Anthropic rates by event date, version price-dependent checkpoints, and rebuild only providers whose pricing revision changed.
- Alerts: added reliable projected-exhaustion notifications with health gating, reset-aware re-arming, mute/first-pass silence, and same-window threshold coalescing.
- Tray: added ordered local provider summaries plus Dashboard and single-flight Refresh actions, rebuilding the native menu only when its pure model changes.
- Refresh: exposed ordered provider health and generation-based manual refresh across the read-loop owner, panel, and dashboard while preserving the last successful snapshot through later failures.
- Release: force-register each recreated sandbox probe bundle with LaunchServices before a fresh launch, eliminating stale executable lookups in repeated verification runs.
- Providers: return saved provider policy as authoritative with a visible watcher-sync warning instead of rolling the UI back after post-persist failures.
- Providers: serialized filesystem-watch reconciliation with provider-policy commits and made policy setters wait until disabled roots are no longer watched.
- Providers: added atomic enable/order settings backed by the compiled registry, stopped disabled tools at scan/watch/state/alert/tray/export boundaries, and added keyboard-accessible controls with optimistic-save rollback.
- Release: removed the duplicate declarative tray icon so `app/src/tray.rs` is the sole owner, added a tray guard across every release override and an Astro semantic check, and refreshed release evidence while retaining unresolved account, runtime, accessibility, soak, price, and domain gates.
- Release: launch the ad-hoc App Sandbox integration probe through macOS LaunchServices so current macOS completes secinit while preserving observable stdin, stdout, and stderr assertions.

## 2026-08-05

- Dashboard: added the model breakdown — what the selected range was actually spent on, per provider. The totals above it said how much; nothing said what for, even though the app has priced every record by model since Phase 5 and then dropped the model on the floor when folding to the hour. An Opus output token and a Haiku cache read differ by fifty times at published rates, so "1.2M tokens" was a number the user could not act on.
- Dashboard: the breakdown bar is share of spend and takes the heat ramp, never the level ramp. Spend is volume, not fullness — the same argument that kept the heatmap off the level ramp in Phase 7. A 90% share drawn in the critical colour would read as a quota about to run out.
- A model the tool never named stays unnamed. Codex writes no model in any record, so its whole breakdown is one row that says the model was not reported, set in italic tertiary ink so it is not read as a model called "Not reported". The sample deck ships that state rather than inventing a name for it, because it is what a real Codex install looks like.
- Rows are ranked on cost where the range was fully priced and on tokens where it was not. An unpriced model bills at zero dollars, and ranking on cost alone would sort the heaviest consumer in the range last — the exact row worth seeing. For the same reason its cell prints a token count instead of `$0.00`.
- Added `core/src/breakdown.rs`: usage folded to the hour and split by an interned label. Hourly rather than at bucket resolution, because a month of five-minute buckets multiplied by every distinct label is the memory budget spent on a surface nobody has open. Interned because `add` runs once per counted record across a whole corpus, and keying the map by label text would allocate a `String` per record — measured at 107 ms against 69 for a cold parse before interning, and 72 after.
- Distinct labels are capped at 64 and the overflow is counted, not merged. Folding the rest into an "other" row would under-report a real model without admitting it, which is the failure `CostRange::unpriced_tokens` already exists to prevent. The dashboard prints the refused count outright.
- The breakdown is filled behind the same guard as the bucket series, so a duplicate, a zero delta or a backwards cumulative total never reaches it. Two places counting the same record under different rules is how a total starts disagreeing with the sum of its own parts; a test asserts the two agree.
- The checkpoint field is `#[serde(default)]`, so a checkpoint written before this existed still restores — the breakdown comes back empty and refills from the next tick rather than forcing a re-read of every log on disk. The label cap is re-applied on restore, so a checkpoint from a build with a larger cap cannot smuggle more labels in.
- Perf after the change: cold parse 73–93 ms against 3000, quiet tick 3.4–5.3 ms against 20, peak RSS 7.3 MB against 60. All four budgets green.
- Added `docs/superpowers/plans/2026-08-05-usage-breakdown.md` — the four phases this is the first of.
- Dashboard: added the project breakdown — which directory the range was spent in, for all three tools. A monthly total answers "how much"; this answers "what for", which is the cut a user bills a client or kills a runaway session on.
- The directory is read from the log, never derived from a file path. Claude Code writes `cwd` on every usage row (9655 of 9655 measured here); Codex writes it in the `session_meta` record that opens a rollout file; Copilot writes it in `session.start`. Decoding it out of Claude Code's `projects/-Volumes-Vault-QuotaDeck` folder name was rejected: that encoding maps `/` and `.` onto the same `-` a real directory name may contain, so reversing it would invent a project rather than report one.
- The two tools that name the directory once, in a record carrying no usage at all, are handled by a session-to-project mapping in `EventIndex` rather than by parser state. A byte-offset cursor is normally long past that opening line, so parser-local state would lose the attribution on the first restart. The mapping is persisted with the checkpoint, bounded at 20 000 sessions with the oldest evicted, and deliberately not pruned with the retention horizon — a session that opened five weeks ago may still be writing.
- Usage a tool named no directory for stays unattributed and says so, in the same italic tertiary ink the unreported model row already uses. Both new checkpoint fields are `#[serde(default)]`.
- A project label is an absolute path, so the list shows the shortest trailing segments that still tell the rows apart — `Archives/app` and `Ledger/app` rather than two rows both reading `app`. The whole path stays in the row's title and in the name the meter announces, and the label column clips rather than eating the bar it is read against.
- Fixed a NUL byte that had landed in `BreakdownList.tsx` where the unreported row's React key wanted a leading space. It rendered fine and made git treat a source file as binary, which hides every future diff of it.
- `BreakdownList` is reused rather than copied: the unreported wording and the overflow line are props now, so one component draws both dimensions and there is one place where the share bar can be put on the wrong ramp.
- Perf after the change: cold parse 73–91 ms against 3000, quiet tick 3.6–3.9 ms against 20, peak RSS 7.7–8.5 MB against 60. All four budgets green. Verified against this machine's real logs: 57 directories for Claude Code, 11 for Codex, 8 for Copilot.
- Alerts: added the runaway-agent warning. Claude Code writes a subagent's transcript to its own file and a workflow agent's to another below that; that spend was already counted, but a folded record no longer said which of the three it came from, so an agent still running after the person who started it walked away looked exactly like work somebody was watching. Measured here: 47M subagent and 36M workflow tokens against 2.9B on the main thread over 30 days.
- The threshold is the user's own history, never a number of tokens. A fixed figure is wrong for everybody: set it where a heavy Max user does not trip it daily and a Pro user's runaway workflow burns their week in silence. The comparison is against the **median** hour of agent spend in retained history — a mean would let one runaway afternoon raise the bar enough to hide the next one. Four times a typical hour, over a 200k-token floor, with at least six hours of profile before any claim is made at all.
- Those constants were calibrated against real logs, not chosen: this machine has **7** hours carrying agent spend across a whole 32-day window, because agents run in bursts of an afternoon rather than continuously. The first draft required twelve, which would have made the rule dead code on the machine it was written for. As it stands the rule fires on exactly 1 of the last 768 hours here — a 38.3M-token hour at 14.4× the median, $9.93 of equivalent cost — and stays silent for the other 767.
- The burst is announced once per episode, not once per tick. The hour it is measured over slides forward continuously, so without that a single runaway workflow would notify every minute it ran. It re-arms when the burst ends, and like every other notification it is silent on the first pass and while muted.
- The panel draws it as a sentence spanning the card, marked with a rule in the neutral heat ink — never on the level ramp. Nothing about a burst says a quota is close to full; borrowing the critical colour would teach the ramp a second meaning. The copy always names what the figure is measured against, because that comparison is the whole claim.
- The dashboard gains the third cut: main thread vs subagents vs workflow agents, on the same reused `BreakdownList`. Codex and Copilot report only the main thread, because neither writes a separate transcript per agent and inventing that distinction would be a claim about their logs that is not true.
- `AgentOrigin` is derived from the path shape alone — the row itself says nothing about it, and `isSidechain` marks a forked conversation rather than an agent. Recognised by the `subagents` and `workflows` directory names rather than by depth below a root, with the inner shape winning, so a workflow agent is never counted as an ordinary subagent.
- Raised the breakdown label cap from 64 to 128. It exists for the adversarial, not to bind on real usage, and 64 nearly did: 57 distinct project directories over 32 days here means a handful of new projects would have started refusing real ones.
- Perf after this phase: cold parse 67–69 ms against 3000, quiet tick 3.2 ms against 20, peak RSS 8.1–8.7 MB against 60. All four budgets green. The third series and the burst scan cost nothing measurable at the tick.
- CLI: added `quotadeck-debug export`, which writes the whole deck to stdout as JSON or as CSV. Everything the three breakdowns answer was locked inside a window nobody can pipe; the export makes the same numbers scriptable without adding a single capability — no network, no listening socket, and no file written anywhere outside the app's own data directory.
- The exit status is the deck's worst reading: `0` ok, `10` at or past 90%, `11` spent, `20` indeterminate, `1` for a failure of the command itself. `20` is deliberately not `0` — a machine with no supported tool, or one whose logs a sandboxed process cannot read, has no reading to give, and a script treating that as "plenty left" would be wrong in exactly the case that matters. A partial first pass reports `20` for the same reason: its peak is a lower bound, not a measurement. The table lives in `docs/STORE.md` §9 and a test fails if the code and the document disagree.
- The CSV is long rather than wide: one row per hour, per dimension, per label. A column per model would give two machines incompatible files, since the label set is a property of the install. `total`, `model`, `project` and `agent` share one row shape.
- An unpriced row leaves the cost cell empty and reports its tokens, never `0`. A `0` in a spreadsheet column reads as free rather than as unknown — the same distinction `CostRange::unpriced_tokens` exists to keep, carried into the one format where somebody will sum the column. A row priced at exactly zero still prints `0.000000`, because a measured zero and an unknown price are different claims. Verified against this machine: every Codex hour exports with an empty cost and its full token count, which is what a tool that names no model in any record actually looks like.
- The refused-label count rides on every row of the dimension that refused it. A CSV has nowhere else to put it, and a truncated table that does not say it was truncated reads as complete.
- Labels are RFC 4180 quoted. A project label is an absolute directory path, and a comma or a quote in one would otherwise split the row it appears in.
- `print!` was replaced with an explicit stdout write, because it panics on a broken pipe and this command exists to be piped — `export --csv | head` aborted with a stack trace. A reader that stopped reading is its own decision, so the quota status is still what gets reported.
- Perf after this phase: cold parse 68–71 ms against 3000, quiet tick 3.2–3.8 ms against 20, peak RSS 7.7–8.2 MB against 60. All four budgets green. The export touches no read path, so nothing was expected to move, and nothing did.

## 2026-08-01

- Windows/release: replaced the non-functional MSIX command with signed x64/arm64 NSIS Store packaging, added user-controlled launch-at-sign-in registration, native OS locale detection, explicit settings/tray/window errors, and deterministic app-icon verification.
- Core: persisted versioned provider checkpoints for restart-safe incremental reads, bounded and hardened watcher state, corrected out-of-order/dedup retention, surfaced malformed provider records, and fixed Claude/Copilot quota estimation and token accounting.
- App Store: removed macOS private-window APIs, moved Claude quota capture into the bundled executable, made sandbox setup explicitly manual and read-only, preserved and conflict-checked the complete prior Claude status-line setting, and made preference writes atomic and backend-authoritative.

## 2026-07-31

- Site: added a theme switch — follow the system, light, dark. The choice is stored and applied by a small inline script before the first paint, because a stored light theme applied one frame late is a dark page flashed at the reader, which is the exact thing the choice was made to avoid. The control is hidden until that script runs: with no JavaScript the page still has both themes, it just follows the system, and a control that cannot act is worse than no control.
- Site: the language control now shows both languages with the current one marked, and each entry links to the same page in the other language rather than to its home page. The `hreflang` pair is per page for the same reason, with an `x-default` alongside.
- Site: added the panel's row drawn in CSS — source mark, level bar, percentage, countdown — with the four columns named underneath. It is a diagram rather than a second screenshot so it takes the theme the reader picked, and the sample row is `aria-hidden` with the same four facts written out below it, so a screen reader gets the explanation instead of a recital of invented numbers.
- Site: build commands on the download page gained a copy button. It renders hidden and the script reveals it, and the confirmation is the button changing its own word — not green, because the level ramp means one thing in this product and a copied command is not a quota.
- Site: the CSP gained `script-src 'self'` plus a pinned sha256 for each inline script, and `scripts/check-csp.mjs` now runs after every build. It reads the emitted HTML rather than the source, so an edited script that was never re-pinned fails the build — a mismatch `astro dev` cannot show, since it serves no policy at all.
- Site: the shadow under the screenshot became a token with a value per theme. A 45% black shadow reads as depth on the dark page and as dirt on the light one.
- Site: added a skip link, and the header bar now wraps instead of clipping — it carries five controls before the content starts.
- Version raised from `0.1.0` to `1.0.0` in `app/tauri.conf.json` and the workspace `Cargo.toml`. The App Store refuses a version number it has already accepted, so the first submission has to carry the number the product ships under.
- `docs/RELEASE_CHECKLIST.md` now opens with the verified state of the code side — clippy, 240 tests, the four performance budgets with their measured figures, the panel typecheck, the site build and the six local routes — so the list of what is left is read against what is already proven.

- Panel: every limit is now a row of its own — four aligned columns of source mark, bar, percentage and countdown. The worst window used to get a 28px display number and the rest a bare list, which made two windows of one limit look like two different kinds of fact. A weekly ceiling stops the work exactly as hard as a five-hour one.
- Panel: the grid lives on the list rather than on each row, so the percentage column holds still down the whole card while the readings tick.
- Panel: rows are ordered shortest window first. The backend's order is the order the provider wrote its log in, which is not an order.
- Panel: the pace projection is a row like the others, drawn on a hollow track at 70% opacity. Everything above it was read off this disk; that one is a line drawn forward from those. The ramp still reaches its fill — a projected 90% earns the same red as a measured one.
- Panel: added the status word (Good / Caution / Critical) beside each provider's name, taken from its fullest window. Green stays neutral: a "Good" lit on every card all day spends the ramp on good news, which is the same argument `PaceBadge` already made.
- Panel: added a provider identity mark — a coloured square, where every mark that carries a reading is a circle. Its four hues sit in the violet-to-cyan arc, outside the ramp's bands, but shape is what actually keeps the two apart: a cyan identity and a green level are close enough on a white card that one would eventually be read as the other.
- Panel: added filter chips, drawn from two reporting tools up. A narrowing to a tool that stops reporting falls back to "all" rather than emptying the panel with no visible reason.
- Panel: installed-but-quiet tools are pills, always visible; tools that are not on this machine stay behind the collapsed disclosure. Sixteen providers and most people have two — fourteen pills would bury the cards above them. This is the spot a competitor fills with service-status badges fetched from a status page; this app makes no network request, so it reports what can be seen from this disk.
- Panel: the footer is an action bar — reading on the left, Dashboard and Quit on the right. Quit was only ever in the tray menu, which is a right click on macOS and Windows and a left click on Linux: three gestures for the one action every user eventually wants, none of them written down.
- Panel: added the `quit_app` command. With no dock icon and no window in the switcher, an app nobody can work out how to close is an app that gets force-quit.
- `ProviderId` is now derived from a `PROVIDER_IDS` array rather than declared as a bare union. The order is load-bearing — identity colours are assigned by position, and a colour that moved when a provider was inserted above it would repaint tools the user had already learned.
- Blueprint §7.2 records the one exception to "the ramp never touches a heading": the status word is a reading, not a label. §7.4 carries the new layout.

- Phase 12: added `site/` — a static marketing site in English and Turkish. Astro rather than the app's own Vite/React chain: the page ships no JavaScript by default, and a page whose headline claim is that the app costs nothing to run should not open with a 78 KB bundle.
- Phase 12: `ui/src/styles/tokens.css` moved to `shared/tokens.css`. The panel and the site now read the same palette, type scale and spacing step, so the two cannot drift apart by hand. Only Vite's dev server needed telling — `server.fs.allow` on both sides; the bundler follows the import either way.
- Phase 12: the Turkish copy is typed as the English object, the same discipline `ui/src/i18n` uses. A key added on one side and not the other fails the build rather than shipping an English sentence inside a Turkish page.
- Phase 12: fonts are self-hosted, and CI greps the built HTML and CSS for third-party origins. A page that pulled two files off a font CDN while claiming the app makes no network request would undo the claim it was making.
- Phase 12: `/download` states that nothing is released and names what each platform is waiting on, beside the script that builds it. A disabled control that looks like a download link is worse than a sentence.
- Phase 12: every figure on the page is one CI asserts. The performance table prints the budget beside the number `core/tests/perf.rs` measured, and says outright that the 7.3 MB is the reader's own peak rather than the whole app — a Tauri app carries a system WebView, which is what the 60 MB ceiling is for.
- Phase 12: the panel screenshots are the real panel at 380px with the sample deck, captured in both languages. No real usage or path is in either image, and no Windows or Linux screenshot is shown, because neither machine exists to take one on.
- Phase 12: added `.github/workflows/site.yml`, on its own path filter. A marketing page failing to render should not sit behind a three-platform Rust matrix, and a broken Rust build should not stop the page going out.
- Phase 12: added `site/vercel.json` with `default-src 'none'`. The page's claim about the app, restated as a header about the page.

- The dashboard now draws its windows with the panel's grammar: the same four-column rows, the same identity square, the same status word. Phase 12 changed one surface and left the other reading the same limits a different way, which is two things to learn for one fact. `sortedWindows` and `worstWindow` moved to `types.ts` so both surfaces share one implementation rather than two copies that drift.
- The dashboard's window rows reuse `.card__rows` from `panel.css` rather than restating the grid, with only the column gaps widened for the card that has the room.
- Blueprint Ek A: the acceptance criteria now say which are green and name the CI step that proves each. Six of eleven were already true and unticked, which made the list read as if nothing had been verified. The five that are left are the ones that need a human — a 72-hour soak, its memory reading, and VoiceOver.
- Added `docs/RELEASE_CHECKLIST.md` — everything left that a commit cannot do, in the order it should be done, with the reason each one needs an account, a machine or a decision.
- `app/MacAppStore.provisionprofile` is gitignored. It carries the Team ID and the chain it was issued against; `scripts/appstore.sh` reads it from the working tree, not from the repository.

- Phase 11: Linux is the third desktop. No path resolution changed — `core/src/paths.rs` already followed XDG, and the provider roots (`~/.claude`, `~/.codex`, `~/.copilot`) are the same directories there.
- Phase 11: the tray item is a different object on Linux. StatusNotifierItem carries no click event — Tauri documents `TrayIconEvent` as never emitted — so the left button opens the menu and the menu's own entry opens the panel. An indicator with no menu is frequently not drawn at all, so the menu is load-bearing twice over.
- Phase 11: no icon geometry either, so nothing can be positioned under the item. The panel goes to the top right, where the indicator area sits on GNOME, Cinnamon, Budgie and XFCE. KDE's default tray is bottom right and the panel does not follow it — stated in `docs/STORE.md` §7 rather than left to be discovered.
- Phase 11: compact mode no longer drops the glyph off macOS. Linux only draws a tray title when an icon anchors it, and Windows does not draw one at all — dropping the icon left an empty Windows tray item, which is a bug that shipped in Phase 10.
- Phase 11: the tray glyph's monochrome ink argument now covers Linux as well. Most desktops ship a dark panel, so the same mid grey holds; unlike Windows there is not even a `SystemUsesLightTheme` equivalent to read.
- Phase 11: added `ui/src/platform.ts`. Two catalogue strings name the surface the tray item lives in, and the three platforms call it three different things — a menu bar, the taskbar, a tray. Read from the user agent rather than through `@tauri-apps/plugin-os`: a dependency, a capability entry and an IPC round trip to learn one word.
- Phase 11: both catalogues take the platform as a parameter for those two strings, so each language writes its own word rather than receiving an English one interpolated into a translated sentence.
- Phase 11: added `app/tauri.linux.conf.json` and `scripts/linux.sh` — `.deb` and `.rpm` for the two package-manager families, an AppImage for the distributions covered by neither. Runtime dependencies are declared, `libayatana-appindicator3-1` included, because without it the item is not drawn.
- Phase 11: no Flathub or Snap submission. Both want an account and a review queue, and neither buys anything here — there is no sandbox grant to declare, no network capability to justify, and no update channel the package managers do not already provide.
- Phase 11: `scripts/linux.sh` greps `cargo tree` for an HTTP client and fails the build if one is there. On macOS the entitlement file enforces the listing's privacy claim; on Linux nothing does, so the dependency tree is what stands in for it.
- Phase 11: CI runs `ubuntu-latest` in the matrix — compile, clippy, tests, perf budget. Hand verification on a real desktop session still needs a Linux machine and stays open in the blueprint.

## 2026-07-30

- Perf gate: added `core/tests/perf.rs`. The criterion bench measured the two hot paths and asserted nothing, and CI only compiled it — so blueprint §5.5's "red means no merge" was a sentence rather than a gate. All four budgets are now assertions: cold parse of a 160 MB corpus in 65 ms against 3000, a tick over 500 established cursors in 3 ms against 20, an hour of watching costing 65 KB against 5 MB, and 7.3 MB peak resident against 60.
- Perf gate: the hourly figure is asserted as an equality against the bytes actually appended, not just against the ceiling. A cursor that silently reset would still come in under 5 MB on a quiet hour; only the equality catches it.
- Perf gate: the budget runs in release and with `--test-threads=1`. A debug build fails a wall-clock budget for reasons that are not regressions, and `ru_maxrss` is a process-wide high-water mark that concurrent tests would misattribute.
- Perf gate: peak memory is measured through `getrusage`, which macOS reports in bytes and Linux in kilobytes — the same field with two meanings. Windows has no `getrusage`, and the budget is a property of the reader rather than of the platform, so it is asserted where it is free and the gap is stated rather than left to be found.
- Blueprint: Faz 0–5 checkboxes now match what shipped, with the file or discovery section that proves each. The Horizon's "returning capacity" ghost layer is struck through rather than ticked — it was dropped in Phase 4 for a measured reason, and an unticked box implies it is still owed.

- Phase 9: `paths::real_home` now reads `pw_dir` from the password database rather than `$HOME`. Inside the App Sandbox `$HOME` is rewritten to the container, which would report every installed tool as absent; the two agree everywhere else, so the change costs nothing until it is the only thing telling the truth.
- Phase 9: our own data directory deliberately still follows `$HOME`, so under the sandbox it lands in the container — the one place the app can write with no permission at all.
- Phase 9: added `app/src/sandbox.rs` — `NSOpenPanel` for the single grant the user makes, and a security-scoped bookmark so it survives a relaunch. The home directory is what gets picked, once; `~/.claude` and `~/.codex` are hidden, and telling someone to press ⌘⇧. in an open panel is not an onboarding flow.
- Phase 9: the grant is a `ScopedAccess` whose `Drop` calls `stopAccessingSecurityScopedResource`. Enough unbalanced starts and the kernel stops granting the process anything at all until it is relaunched, so the pairing is a type invariant rather than a discipline.
- Phase 9: a stale bookmark is rewritten as soon as it resolves. It still works today; leaving it costs the grant eventually.
- Phase 9: the popover no longer dismisses itself while a modal it opened holds the focus — asking for the folder used to close the window that asked.
- Phase 9: added `app/Entitlements.plist` with the sandbox and the two file capabilities that are actually used. There is no outbound-connection capability, so the store listing's privacy claim is enforced by the code signature; CI already fails if one is added.
- Phase 9: added `scripts/sandbox-check.sh`. `sandbox-exec` was rejected — it is deprecated and its profile language is not the App Sandbox. An ad-hoc signature carrying the shipping entitlements is, and it proved the four things that matter: `$HOME` becomes the container, `real_home` does not, our writes stay inside it, and every provider root reports `denied` rather than `missing`. Runs in CI on macOS.
- Phase 9: a bare Mach-O signed with the sandbox entitlement is killed with SIGTRAP at launch, because the container is named after `CFBundleIdentifier`. The check wraps the debug binary in the smallest bundle that satisfies that.
- Phase 9: added `quotadeck-debug paths`, which is what the sandbox check asserts on.
- Phase 9: added the sample deck, off by default and offered from the empty state. A machine with no supported tool shows an empty panel, and an empty panel is indistinguishable from a broken app. The menu bar keeps reporting the real reading — a fabricated percentage outside the window that admits it is a sample would be a claim, not a demo.
- Phase 9: added `scripts/appstore.sh`. It verifies that the sandbox entitlement actually reached the signature — Asset Validation error 90296 is what its absence looks like from Apple's side — and that no network capability crept in.
- Phase 9: added `docs/STORE.md` — listing copy, the metadata rule that gets apps rejected for brand stuffing, and the "Data Not Collected" declaration with the architectural reason each answer is true.
- Phase 10: the tray glyph is grey rather than black off macOS. Windows has no template images, and a black glyph is invisible against the dark taskbar almost everyone runs.
- Phase 10: added `app/tauri.msstore.conf.json` (`webviewInstallMode: offlineInstaller`, a Store condition) and the initial Windows packaging path; corrected on 2026-08-01 to the Store's signed NSIS/Win32 route.

## 2026-07-29

- Phase 8: added the two-language catalogue (`ui/src/i18n/`) with English and Turkish. `Catalogue` is derived from the English object, so a key added there fails the build until it is translated.
- Phase 8: a test compares the two catalogues by key path *and* by function arity — a translation that drops a placeholder type-checks fine and silently loses the number the sentence was written around.
- Phase 8: separated the two questions a locale answers. The catalogue decides the words and the user picks it; numbers, dates and clock times stay on the system's regional settings unless a language is picked explicitly.
- Phase 8: percentages, token counts and costs go through `Intl` rather than string concatenation. Turkish writes %76, not 76%, and 1,2M rather than 1.2M.
- Phase 8: added `app/src/i18n.rs` — the backend's own catalogue for the two surfaces the panel cannot speak for: the notifications, raised from the read loop while the webview may be suspended, and the tray menu, which is the only way to quit an accessory app.
- Phase 8: `Locale::System` resolves from `LC_ALL` / `LC_MESSAGES` / `LANG` in POSIX order, matched on the primary subtag. A language with no catalogue is not a match, so the search continues rather than settling for a half-translated app.
- Phase 8: the alert tests name their language. `Locale::System` reads the environment, and a default would have made every assertion on the wording depend on the machine running CI.
- Phase 8: Escape steps out of settings, then out of the panel, through a new `hide_panel` command that sets the same open flag the click-away path does.
- Phase 8: the dashboard's range picker became a radio group with a roving tabindex and arrow keys — one choice, announced as one of three, with only the chosen option in the tab order.
- Phase 8: focus moves with the view. Toggling to settings replaces everything under the header, and focus that stayed on the button left a keyboard user at the top of a screen that was no longer there.
- Phase 8: the focus ring moved from the 13px native radio to the chip around it, so the focused thing looks like the thing you would click.
- Phase 8: the printed percentage is now hidden from assistive technology and the bar beneath it carries the reading as a `meter` with `aria-valuetext`, rather than the pair being announced twice.
- Phase 8: the chosen dashboard range and the pace meter carry a shape as well as a tint — a dot that is always in the box and only changes colour, and the same two fill patterns the usage bar uses.
- Phase 8: `<html lang>` follows the resolved language, because it is what a screen reader reads its pronunciation rules off.

- Phase 7: added `core/src/pace.rs` — a pace forecast per quota window, wired into every provider through `snapshot_with_windows` so no provider file learns about it.
- Phase 7: two horizons, as the blueprint specifies. A session window is projected from an exponentially weighted burn rate (α = 0.3, fifteen-minute bins over four hours); a weekly or monthly one is weighted by the user's own hour-of-week profile over the retained history, scaled by how much heavier the current window has run than that profile predicted.
- Phase 7: the projection reports the peak of the trajectory, not its endpoint. Against a rolling window usage falls off the far edge as the near edge advances, so a trajectory can cross the ceiling and fall back, and the endpoint would hide exactly the crossing worth warning about.
- Phase 7: a stale reading never anchors a projection. Codex writes its monthly limit once per plan period, and extrapolating a fortnight-old 72% against a month of spend counted since produced a 999% projection on real logs.
- Phase 7: a fresh reading anchors its ceiling at the instant it was taken, so twenty minutes of heavy work between the reading and now moves the starting point rather than being discarded.
- Phase 7: forecasts are counted in equivalent API cost where the window was fully priced and in tokens where it was not. Codex names no model in any record, and a cost-only forecast would exclude the provider whose measured windows make one worth having.
- Phase 7: nothing is projected below 5% used, from a window with no counted consumption, or from a window whose reported reset has already passed.
- Phase 7: added the pace badge to the panel — a miniature of the bar it is projecting, with the risk word beside it, so the reading survives colour blindness and greyscale.
- Phase 7: added the dashboard window (960 × 640), opened on demand from the panel and never created at launch. One bundle serves both surfaces; the window's own label decides which mounts.
- Phase 7: added `core/src/history.rs` — retained usage folded to the hour, with the empty hours dropped. A month of five-minute buckets is 8640 points per provider and nearly all of them are empty.
- Phase 7: history is pulled by the dashboard rather than pushed with the snapshots. The panel never renders it, and a month of history on every five-second tick would charge a closed surface to the channel the panel depends on.
- Phase 7: dashboard ranges are rolling — last 24 hours, 7 days, 30 days — matching the panel's rolling day. Heatmap cells are local calendar days, folded in the frontend because only it knows the viewer's zone.
- Phase 7: the heatmap is shaded on a neutral ink ramp, never the level ramp. Volume is not fullness, and a colour in this app means exactly one thing.
- Phase 7: every window gets its own pace badge on the dashboard. The panel has room for one and gives it to the window closest to running out.
- Phase 7: added threshold notifications at 70 / 85 / 95%, adjustable per provider, through `tauri-plugin-notification`. On macOS the plugin resolves to `notify-rust` → `mac-notification-sys`; the zbus/zvariant crates in the lockfile are the Linux path and never compile for our targets.
- Phase 7: a notification is about a crossing, not a state. The first complete pass seeds the state silently — launching on a Friday afternoon must not fire one per provider before any work is done — and each threshold is announced once per window period.
- Phase 7: three points of hysteresis before an announced threshold can fire again. A rolling window drifts across its own boundary as old usage expires, and without it someone working at 85% is notified on every tick.
- Phase 7: a stale reading never raises a notification. Copilot's exhaustion event sits on the card for the rest of the month and is not news for any of it.
- Phase 7: added mute for an hour or until tomorrow. The instant is computed from a duration the panel supplies, so "tomorrow" means the viewer's tomorrow and the backend stays UTC-only. Muting holds the crossing state, so lifting it does not release a burst.
- Phase 7: notification copy lives in Rust because the panel is usually closed and its webview may be suspended. Phase 8 gives this module its own localised catalogue.
- Phase 7: fixed two zustand selectors that built a fresh array per call — a new snapshot on every render, which left the settings view blank.

## 2026-07-28

- Phase 6: added the GitHub Copilot CLI provider — reads `~/.copilot/session-state/<uuid>/events.jsonl`, where usage is written once per session at shutdown and an in-flight session is invisible until it exits.
- Phase 6: cost comes from Copilot's own credit meter (`totalNanoAiu`) at GitHub's published 1 credit = $0.01, so this is the one provider that needs no price table and leaves nothing unpriced. `totalPremiumRequests` is the retired unit and is carried through but never used as a ceiling.
- Phase 6: corrected `docs/DISCOVERY.md` §7 — premium requests stopped being the billing unit on 2026-06-01, when GitHub moved to AI credits.
- Phase 6: token counts are read from `modelMetrics`, not `tokenDetails`. The latter excludes cache reads and writes and disagreed with the real input total in 33 of 171 sessions, once reporting 9 against 27 758.
- Phase 6: added the Copilot plan picker (Pro / Pro+ / Max / Business / Enterprise) against GitHub's published credit allowances. The promotional Business and Enterprise amounts are deliberately not encoded — they expire on 2026-09-01 and a ceiling that silently lapses starts over-reporting.
- Phase 6: the Copilot window is a calendar month resetting on the 1st at 00:00 UTC, not a rolling thirty days. A rolling sum would report spend the user has already been forgiven.
- Phase 6: a `quota_exceeded` session error is read as a measured 100%, and dropped once the month it was observed in has ended. On real logs the CLI's own spend reads 15% of a Pro plan while the account was in fact exhausted — editor and web credits never reach this machine, so the derived figure is a floor and this event is the only thing that reveals the rest.
- Phase 6: the plan hint is now per provider. GitHub publishes an exact ceiling and Anthropic publishes none, so one sentence covering both would say nothing.
- Phase 6: Kimi, Gemini CLI, Qwen, OpenCode, Amp, Droid, Goose and the rest remain unimplemented — none are installed on this machine and no parser ships against a guessed schema (`docs/DISCOVERY.md` §10).

- Phase 5: added the Claude Code provider — reads `~/.claude/projects/**`, including subagent and workflow-agent transcripts, whose calls bill to the same subscription.
- Phase 5: deduped usage on `(message.id, requestId)`. One API response is written as several rows, one per streamed content block, each repeating the whole `usage` object; 45.8% of 3412 real rows were repeats. Deduping on `uuid` instead would have caught none of them — it is regenerated per row.
- Phase 5: quota estimates are denominated in equivalent API cost rather than tokens. At published rates an Opus output token and a Haiku cache read differ by 50x, so a rolling token count cannot be compared against one ceiling.
- Phase 5: embedded a LiteLLM-derived Anthropic price table at build time. Lookup takes the longest matching model prefix, never the family — `claude-opus-4-1` bills at three times `claude-opus-4-5` and the shorter id prefixes the longer one.
- Phase 5: a model with no known price is counted as unpriced, never as free. The remainder is carried to the UI and blocks calibration, because a numerator short by an unknown amount must not anchor a ceiling.
- Phase 5: added the plan picker (Pro / Max 5x / Max 20x). "Not set" is the default and produces no estimate at all; a tier nobody picked would put an unrequested percentage under an estimated badge.
- Phase 5: one measured window now calibrates the tiers the tool did not report. The seeded ceilings are assumptions — only the published 1 : 5 : 20 tier scaling is taken as given — and a real reading replaces them.
- Phase 5: added the opt-in statusline shim, which reads Claude Code's real `five_hour` and `seven_day` percentages. It chains the user's existing command instead of replacing it, records `rate_limits` and the version only, preserves every other key in `settings.json`, and reverts in one click.
- Phase 5: settings are now persisted to the app data directory, so a chosen plan survives a restart.
- Phase 5: fixed the Horizon strip drawing a seven-day axis over one day of buckets. The series is trimmed to the longest window, and an estimated window added after the snapshot was built never reached that calculation — found on real logs, where a 91% weekly reading sat beside a near-empty strip.
- Phase 5: `quotadeck-debug` gained `statusline` (the exact before and after of the settings change) and an optional plan argument on `debug`, so the estimate can be checked against real logs from a terminal.
- Phase 5: CI now enforces that only the shim module touches `settings.json`, and that the shim records no payload field beyond the rate limits.

- Phase 4: added the Horizon strip — the timeline behind each provider's headline window, drawn as SVG from the five-minute bucket series.
- Phase 4: the strip's time axis comes from the reported `window_minutes`, so it is a week for a Codex weekly limit and five hours for a session one. No window length is assumed anywhere.
- Phase 4: dropped the blueprint's "returning capacity" ghost layer. It only holds for a sliding window, and the two providers measured in Phase 0 disagree — Claude Code resets on clean half-hour boundaries while Codex resets at an arbitrary instant, and `resets_at - window_minutes` lands 3.6 hours before a reading that already showed 68% of a week. The strip now states only the span it can prove.
- Phase 4: column height is linear against the busiest column with a visible floor, so a burst is never understated and a quiet column never reads as a gap.
- Phase 4: recency is carried by a gradient mask over the whole strip rather than per-column opacity — one declaration, a smooth ramp, and a floor each theme sets for itself.
- Phase 4: hovering the strip replaces the axis labels in place with that slice's clock time, its width and its token total. The readout is fixed-height so the card does not move under the cursor.
- Phase 4: added the menu bar strip mode, a 44×16 miniature of the same fold. It stays a template image below 85% like every other tray mode.
- Phase 4: `quotadeck debug <provider>` now prints the strip as text, so the fold can be checked against real logs rather than fixtures alone.
- Phase 4: snapshots now carry only the history the strip can draw instead of the full retention window — the panel was being sent up to 32 days of buckets every five seconds.
- Phase 4: added Vitest and wired it into CI. The panel's fold is a mirror of `core/src/horizon.rs` and is held to the same cases.

- Phase 3: added the Tauri v2 tray application — accessory activation policy, a transparent borderless panel positioned under the menu bar item, and click-away dismissal.
- Phase 3: the panel window is granted no filesystem capability at all; it receives folded snapshots over an event and nothing else.
- Phase 3: added the menu bar glyph, drawn as raw RGBA from the live reading. It stays a monochrome template image until usage passes 85%, and only then takes the critical colour.
- Phase 3: added the design system as CSS custom properties — the blueprint's dark and light palettes, Martian Mono for every number and Inter for text, both bundled with their SIL OFL licences.
- Phase 3: added the provider card with confidence badges, a level ramp that also carries pattern and text so it survives colour blindness, and empty states that give direction instead of apologising.
- Phase 3: added `ProviderEngine`, which keeps cursors between ticks. A timed full re-scan would have read hundreds of megabytes a minute; a quiet tick now reads zero bytes.
- Phase 3: the panel window sizes itself to its content rather than leaving empty surface below the cards.
- Phase 3: fixed an unreadable log folder being reported as "not installed" — found by running the app under `sandbox-exec`, which is the state every user starts in under the macOS App Sandbox.
- Phase 3: CI now builds and typechecks the panel, and enforces that no network entitlement and no frontend filesystem capability are ever added.

## 2026-07-25

- Phase 2: added the Codex provider — reads `~/.codex/sessions/**/rollout-*.jsonl`, classifies windows by reported duration, and reports measured limits per `limit_id`.
- Phase 2: counted Codex tokens from `last_token_usage` instead of differencing the session running total, after measuring that differencing over-reports by 17% when a resumed session carries its predecessor's total into a new file.
- Phase 2: deduped Codex re-emitted records on `(file + running total, turn total)`, which removed all 37 duplicates found in real logs without dropping the 5 distinct turns that shared a running total.
- Phase 2: split `cached_input_tokens` out of `input_tokens` and `reasoning_output_tokens` out of `output_tokens` so neither is counted twice.
- Phase 2: added `discovery` (segment-wildcard file finding, newest first) and `scan` (discovery plus incremental reading plus parsing) to core.
- Phase 2: `quotadeck debug <provider>` now prints windows, confidence, reset times and token totals from real logs.
- Phase 2: added six real and six synthetic Codex fixtures with a README separating measured shapes from defensive ones.

- Phase 1: added the Cargo workspace (`core`, `providers`, `app`) with no HTTP client and no listening socket.
- Phase 1: added the core data model — quota windows classified by reported duration, several independent limits per provider, and a confidence level that decays to stale after 30 minutes.
- Phase 1: added `EventIndex` with `(message.id, requestId)` dedup, cumulative-to-delta conversion per session, and retention-bounded pruning.
- Phase 1: added byte-offset incremental reading with a partial-line buffer, chunk limiting, and rotation detection by inode or file index.
- Phase 1: added bounded reverse-tail reading so an L1 refresh costs a few KB regardless of log size.
- Phase 1: added the non-recursive `notify` watcher with 750 ms per-path coalescing.
- Phase 1: added the redb store with batched writes (500 records / 60 s / shutdown).
- Phase 1: added the `Provider` trait, the provider registry, and the `quotadeck` debug CLI.
- Phase 1: added criterion benchmarks and CI enforcing format, clippy, tests, and the no-HTTP / no-credentials constraints.

- Phase 0: verified Codex `rate_limits` is non-null on real logs — Codex confirmed as an L1 MEASURED source.
- Phase 0: discovered and live-verified the Claude Code statusline `rate_limits` payload (`five_hour` + `seven_day`) — Claude Code upgraded to L1 with zero credentials.
- Phase 0: tested and rejected the OTLP telemetry path — no rate-limit metric, requires a listening socket, leaks PII.
- Phase 0: measured a 51% duplicate rate in Claude Code usage rows, confirming `(message.id, requestId)` dedup is mandatory.
- Phase 0: measured log volume (175 MB/day Codex, 0.49% quota-relevant) and restated the read budget as relative rather than a fixed MB/hour figure.
- Added `docs/DISCOVERY.md` with the full Phase 0 findings and `CLAUDE.md` with the project working rules.
