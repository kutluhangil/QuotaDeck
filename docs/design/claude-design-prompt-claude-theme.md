# Quota Deck — Claude Design prompt: "Claude" theme

Paste everything below the line into Claude Design, in the **same project** as "Quota Deck v2 — Liquid Instruments", so both themes share one file.

---

## What this is

Quota Deck already has a design called **Liquid Instruments** (Apple Liquid Glass). Now design its second theme, **"Claude"**: the same app, the same screens, the same information, re-imagined as if **Anthropic's own design team had built it**.

The theme must feel like Claude: ivory paper, warm ink, terracotta clay, a literary serif, hand-inked engraving illustrations, calm honest copy. It should be quiet, intelligent and warm, never techy, neon or gamer-ish. It should feel like a beautifully typeset field notebook for your Mac and your AI tools.

**Important for engineering:** themes are swapped at runtime from Settings.

- Keep the **same screens, the same component anatomy and the same information hierarchy** as the Liquid Instruments artboards (popover modules, dashboard pages, cards, tables).
- What changes is tokens (colour, type, radius, shadow), textures, ornaments, illustration slots, chart styling and motion.
- Name every component exactly as in Liquid Instruments so both themes map 1:1 onto one React component tree.
- Theme-only decorations (ornaments, illustrations, paper texture) must be optional layers that can be removed without breaking layout.

## The product (so you know every feature)

Quota Deck is a macOS menu bar app for one machine: **Mac mini M6, 32 GB, macOS 27.2**. It is local and read-only: no account, no network. It shows three things:

1. **The Mac:**
   - CPU (E/P clusters, per-core, load average), GPU, memory (pressure, swap, compressed);
   - temperatures (P-cores, E-cores, GPU, SoC, SSD/NAND; this Mac has **no RAM sensor**, so an honest "no sensor" state);
   - fan (999 RPM of 4900 max, with Auto / Cool / Max control after a one-time admin approval);
   - power (system watts + CPU/GPU/ANE/DRAM split);
   - storage (free space, read/write speed, SMART "Verified", external disks);
   - network (down/up only, no ping);
   - top processes (name, memory, % CPU, energy, PID);
   - a 0–100 health score.
2. **Docker:** running/stopped, containers grouped by Compose project with CPU, memory, health and restarts; totals for containers, CPU, memory (of the VM limit) and disk (images, volumes, build cache). When Docker Desktop is off, a calm "Docker is asleep" state with a gentle animation.
3. **AI quota for exactly three tools:**
   - **Claude Code and Codex:** session window (5 h) and weekly window, "% left", reset countdowns, a pace marker (where you'd be at even usage), "in reserve / over pace" sentences, today and window cost, tokens, 30-day cost, top model, a recent-windows history, a usage heatmap, breakdowns by model / project / agent, and a session list.
   - **Antigravity:** no percentage exists locally, so a per-model list: "Available" or "Exhausted, reopens in 4 h 52 m". Plus daily activity. An optional "Live quota via agy" setting shows real percentages but uses the network, and must say so.
   - Every estimated number carries an **"Estimated"** badge and every measured one a quiet **"Measured"** mark.

Use the **real product logos** (Claude spark, OpenAI blossom for Codex, Antigravity mark) at small sizes, never recoloured into something off-brand.

## Visual language

### Palette (exact values from the Claude-style site I already use)

| Role | Light | Dark |
|---|---|---|
| Background (paper) | `#F9F9F7` | `#1F1E1D` |
| Surface | `#FCFCFB` | `#262624` |
| Raised (cards) | `#FFFFFF` | `#30302E` |
| Sunk (wells, inputs) | `#F1F0EC` | `#1A1918` |
| Ink (primary text) | `#141413` | `#FAF9F5` |
| Ink 2 (secondary) | `#52514E` | `#C2C0B6` |
| Ink 3 (tertiary) | `#87867F` | `#8F8D85` |
| Line (hairline) | `#E1E1DF` | `#3E3E38` |
| Line 2 (faint) | `#ECEBE7` | `#33332F` |
| Clay (the accent) | `#D97757` | `#D97757` |
| Clay ink (accent text) | `#B5573A` | `#E8957A` |
| Peach band | `#EBC9B7` | `#3B2A22` |
| Dot texture | `rgba(20,20,19,.07)` | `rgba(250,249,245,.06)` |
| Link | `#0099CC` | `#5FB8DE` |
| Shadow | `0 1px 2px rgba(20,20,19,.04), 0 8px 24px -12px rgba(20,20,19,.12)` | `0 1px 2px rgba(0,0,0,.3), 0 12px 32px -16px rgba(0,0,0,.6)` |

Secondary hues, used sparingly for categories and charts only:

| Hue | Value |
|---|---|
| Blue | `#5A8FC0` |
| Olive | `#788C5D` |
| Lavender | `#9A7BB5` |
| Mustard | `#C9A227` |
| Teal | `#4F9D9A` |
| Warm grey | `#87867F` |

- **Tool accents:** Claude = clay `#D97757`; Codex = ink with teal `#4F9D9A`; Antigravity = lavender `#9A7BB5`.
- **Temperature scale, warm and earthy, never neon:** below 60 °C sage `#8A9A7B`; 60–70 °C mustard `#C9A227`; 70–90 °C clay `#D97757`; above 90 °C rust `#A8432A`. Always paired with the number and a word (Calm / Warm / Hot / Critical).
- **Status words:** Normal = olive, Warning = mustard, Critical = rust. Never pure red or green.

### Typography

- **Display and headings:** a literary serif, Anthropic Serif with **Source Serif 4** as the shipping fallback. Weight 400–500, optical sizing on, tracking −0.012em. Hero numbers (health score, "% left", temperatures) are set in the serif: big, calm, like a book's chapter numeral.
- **UI text:** Inter (variable), 13–15 px, comfortable line-height 1.5–1.65.
- **Data:** JetBrains Mono for bytes, PIDs, token counts and timestamps. Tabular figures everywhere live numbers update.
- **Hierarchy comes from serif vs sans and size.** Few boxes; generous margins; centred section headings with an ornament rule.

### Surfaces, texture, ornament

- **Paper:** the window background is ivory with a fixed **16 px dot grid** (1 px dots) and a soft radial vignette that fades the dots toward the edges, like the dot texture on Claude's site.
- **Cards** are paper cards:
  - radius 16–20 px;
  - hairline border in Line 2;
  - Raised at about 55% opacity with a faint backdrop blur over the dots;
  - on hover they lift 2 px and gain the soft shadow.
  - No heavy shadows, no glass refraction, no gradients for decoration.
- **Ornament rule:** centred section titles sit between two fading hairlines with a small node in the middle, like an engraved divider. For the biggest sections use a tiny engraved compass-rose ornament whose north needle is the Claude spark.
- **Capsule navigation:** a pill-shaped tab bar on Surface, with a raised white indicator that **slides** between tabs (0.45 s, `cubic-bezier(0.16,1,0.3,1)`).
- **Primary button:** an ink-filled pill (`#141413` on light, ivory on dark) with paper-coloured text, 48 px tall on hero CTAs and 32 px in toolbars. It presses down to 0.97 scale.
- **Secondary button:** a hairline pill on translucent Raised.
- **Icons:** thin-line (Lucide style, 1.75 px stroke), ink-coloured, never filled.

### The signatures (what makes this theme unforgettable)

1. **Ink-wash gauges.** Every "how full" value (quota left, CPU, memory, disk, fan) is a horizontal or vertical bar drawn like a hand-inked engraving:
   - a hairline outline;
   - the filled part is a flat clay or category-colour wash;
   - its leading edge is finished with fine **cross-hatching** that fades out, like a pen stroke trailing off.
   - The **pace marker is a tiny clay push-pin** stuck into the bar, echoing my site's illustration of cards pinned with a thread.
2. **The Thermal Atlas.** The SoC is drawn as an **engraved cartographer's map**:
   - the die outline is an old map border;
   - P-cores, E-cores, GPU, SoC fabric and NAND are "regions";
   - **isotherm contour lines** (like a topographic map) show where the heat is, with a clay-to-rust wash inside hotter contours;
   - a small compass rose sits in the corner;
   - region labels are set in small-caps serif.
   - Hover shows a paper tooltip card with average, max and sensor count.
3. **The Health Seal.** The 0–100 health score sits inside an engraved **wax-seal / medallion** ring (a circular ornament with a hand-drawn rim), with the score in serif and one word below (Excellent / Good / Fair / Poor), plus one plain sentence: "All readings are calm." or "Memory is under pressure."
4. **The Ledger.** AI cost and token tables look like a fine accountant's ledger: serif numerals, ruled hairlines, a right-aligned total line with a double rule, and dates in mono.
5. **The Timeline Spine.** Recent windows, alerts and history use a vertical dotted spine on the left with clay nodes, like a journal timeline.

### Charts ("ink on paper")

- Lines are 1.5 px **ink** (Ink 2), with the current series in clay. Area fills are a 12–18% wash of the line colour; no glossy gradients.
- Bars have rounded caps, in a flat wash with a hairline outline.
- No gridlines; one baseline hairline; two quiet axis labels in Ink 3.
- Scrubbing shows a dotted vertical rule and a small paper card with the value.
- The heatmap is a calendar of small rounded squares from Sunk to clay, with a legend "quiet → busy".
- The mirrored network wave has down above the baseline and up below it, in ink and Ink 3.
- The thermal ribbon (popover) is a thin hand-inked band coloured along the warm temperature scale.

### Motion

- One orchestrated rise when a page opens: children fade up 14 px in sequence (0.9 s, `cubic-bezier(0.16,1,0.3,1)`, 80 ms stagger).
- Page switches crossfade (out 0.22 s, in 0.32 s with a 6 px rise).
- Gauges fill once with that easing; numbers change with a gentle digit roll.
- The Docker-asleep illustration breathes slowly (a 6 s ease-in-out drift).
- Honour Reduce Motion: no rises, no drifts, instant fills.

### Illustrations (slots; I will generate them separately)

Hand-inked engravings in the Claude-brand style: confident black ink linework with fine hatching, flat clay `#D97757` and peach `#EBC9B7` fills, warm grey accents, a transparent background, and an ivory-ink dark variant. No text, faces or logos. Reserve these slots and draw tasteful placeholders in this style:

- Docker asleep (a small stack of shipping crates with a sleeping cat curled on top).
- Tool not installed (an empty pegboard hook with a tag).
- Needs folder access (a closed notebook tied with string).
- No data yet (a blank index card leaning on a small plant).
- The Thermal Atlas compass rose.
- The Health Seal rim.
- The ornament divider.
- The About-window frontispiece (the Mac mini drawn as an engraved object on a desk with three small cards pinned above it).

### Voice

Claude's voice is plain, calm and honest. Examples:

- "Nothing is guessed."
- "Remaining quota isn't written to local files, so this shows what can be measured."
- "Docker is asleep. Start Docker Desktop and the containers will appear here."
- "This Mac doesn't expose a memory temperature."
- "7% in reserve. At this pace it lasts until the reset."

No exclamation marks, no hype, no emoji.

## Screens to design (every one, light and dark)

### 1. Menu bar item

Monochrome, matching macOS: `48° · 76% · [Claude spark] 67%`. Design these states:

- Normal.
- Quota under 15%: a small mustard dot.
- Quota under 5% or thermal Critical: a rust dot.
- Compact: an icon only. A tiny engraved capsule whose fill shows the tightest quota.
- Antigravity tightest: shows `4:52` until reopen.
- Pressed (popover open).

### 2. Popover (360 × 640, attached to the menu bar)

An ivory paper sheet with the dot texture, rounded 18 px, with a soft shadow. Same module grid as Liquid Instruments, but as paper cards:

- **Header:** "Mac mini · M6 · 32 GB", "macOS 27.2 · up 1 d 23 h" in Ink 3, the Health Seal (small) and a settings icon.
- **AI quota card (2×2):** Claude, Codex and Antigravity rows with logo, serif "% left", an ink-wash bar with a push-pin pace marker, and "Weekly · resets in 5 d 22 h". The Antigravity row shows per-model mini tags (Flash: available · Pro: reopens in 4 h 52 m).
- **CPU, Memory, Disk, Docker cards (2×1):** serif hero number, one quiet caption, a vertical ink-wash gauge.
- **Fan, Power, Network, Keep Awake (1×1):**
  - Fan shows RPM and a round engraved dial.
  - Keep Awake is a toggle card: filled ink when on, hairline when off.
- **Thermal ribbon (full width):** last 60 s, with the "5-min peak 60.5°" label.
- **Footer:** "Updated 2 s ago" and an ink pill button "Open Dashboard".

### 3. Dashboard window (1180 × 760, resizable)

- **Top bar:** traffic lights, the Quota Deck wordmark in serif, and the centred capsule navigation **Overview · Mac · Docker · AI**. On the right: a range control (1 h · 24 h · 7 d), a search field (filters processes and containers) and a settings icon.
- Every page starts with a centred serif page title and a one-line Ink-2 summary, then the content.

**3a. Overview**

- Hero: the large Health Seal, with three AI quota gauges beside it (Claude, Codex, Antigravity).
- An ornament divider, then: a compact Thermal Atlas, plus small cards for Memory, Storage, Fan, Power and Network.
- An ornament divider, then: the Docker summary (or "asleep") and the five busiest processes.

**3b. Mac**

- **CPU:** a serif %; two vertical ink-wash gauges for the E-cluster and P-cluster; a per-core strip; load 1/5/15 in mono; temperatures (P avg/max, E avg); a sparkline that expands into the range chart.
- **GPU:** %, °C, W.
- **Memory:** used/total, a pressure word chip, compressed, swap, and an area chart.
- **Memory temperature:** the honest empty state ("This Mac doesn't expose a memory temperature") with a small illustration slot. Also show the alternative design with a value, for completeness.
- **The Thermal Atlas, full size (the hero of the page):**
  - the macOS thermal state chip (Nominal / Fair / Serious / Critical);
  - the 5-min peak;
  - a 24 h chart with CPU, GPU and SSD lines;
  - a sensor ledger listing every sensor family (Tp, Te, Tg, Ts, TH0) with count, average and max.
- **Storage:** an internal SSD gauge, read/write dual sparkline, NAND temperature, the SMART "Verified" chip, and external disks as ledger rows.
- **Fan:**
  - RPM in serif, % of max, and the min/max range drawn as an engraved dial scale.
  - A segmented control: **Auto · Cool · Max**.
  - Show three states: managed by macOS (Auto); **"Unchanged — needs approval"** in mustard after the admin prompt was declined; and Cool active, with "Returns to Auto when Quota Deck quits."
- **Power:** system watts in serif, with a stacked ink-wash bar for CPU / GPU / ANE / DRAM.
- **Network:** ↓/↑ rates and the mirrored wave. A footnote: "No ping. Quota Deck makes no network requests."
- **Processes ledger:** app icon, name, memory, % CPU with an inline ink-wash bar, energy W, PID, and a `…` menu (Reveal in Finder, Copy PID). Sortable column headers in small caps; the hover row on a peach band; a count "Processes · 157".

**3c. Docker**

- **Running:** "Docker Desktop · running". Four ledger tiles: Containers 8/11, CPU x of N vCPU, Memory 3.1 of 4.8 GB as a gauge, Disk as a stacked bar (images / volumes / build cache). Below, containers grouped by Compose project, with collapsible serif group headers and project totals. Each row shows a state dot, name, image:tag in mono, a health chip (healthy olive / unhealthy rust / none), CPU, memory, uptime and restarts.
- **Asleep:** the breathing illustration, "Docker is asleep." in serif, one Ink-2 line, and an ink pill "Open Docker Desktop".
- **Not installed:** an illustration slot and a quiet line.

**3d. AI**

A secondary capsule control: **Claude · Codex · Antigravity**, each tab with its real logo.

- **Claude and Codex (shared layout):**
  1. Two large ink-wash gauges, **Session (5 h)** and **Weekly**: serif "% left", the reset countdown, the push-pin pace marker, and a sentence (reserve in olive, over pace in mustard).
  2. **Ledger stats:** today, current window, window tokens, 30-day cost, 30-day tokens, top model. Each value carries an "Estimated" (hairline pill) or "Measured" (small filled dot + word) badge.
  3. **30-day cost bars** in the tool accent, with a range switch Tokens / Equivalent cost and custom dates (From / To).
  4. **Daily activity heatmap** (last month).
  5. **Recent windows** on the Timeline Spine: date range, cost, tokens, and how full that window got.
  6. **Breakdowns:** by model, by project and by agent, as ledger lists with thin bars. Long paths are shortened in the middle.
  7. **Sessions list:** start, duration, model, tokens, cost.
  8. **Export:** "Copy CSV" and "Copy JSON" as hairline pills.
- **Antigravity:**
  - Model status ledger: model, state, reopens-in countdown gauge.
  - A daily activity chart (conversations and prompts per day).
  - A note card: "Remaining quota isn't written to local files, so Quota Deck shows what it can measure."
  - The **Live quota via agy** switch with the label "Uses the network through the agy command".
  - When live is on, show percentages per model with an "agy · live" badge.

### 4. Settings (a full page in the dashboard, plus a compact version reachable from the popover)

A left column of section names in serif, with the content on the right. Every control is real and labelled:

- **Appearance:**
  - **Design theme** cards with thumbnails, "Liquid" and "Claude" (selected), plus a disabled "More themes coming" card.
  - Mode: Match system / Light / Dark.
  - Reduce texture (hides the dot paper) and Reduce motion toggles.
- **Menu bar:**
  - Content: CPU temperature, Memory, Tightest quota (checkboxes).
  - Style: Glyph / Percentage / Horizon.
  - Keyboard shortcut ⌘⌥Q, on/off.
- **Refresh:** Live 1 s · Balanced 2 s · Saver 10 s, with an explanation of the energy cost.
- **AI tools:**
  - Claude, Codex and Antigravity on, with reorder handles.
  - "Other tools" collapsed: Amp, Copilot, Droid, Gemini, OpenCode and Pi Agent, all off.
  - Per tool: plan (Pro / Max / Team / Not set: "No estimate is shown. Nothing is guessed."), warning thresholds (chips like 50 / 75 / 90%), separate accounts, and additional log folders.
- **Antigravity:** Live quota via agy (network label), and the agy path.
- **Claude Code status line:** connect / disconnect, a before/after preview of the status line, "Your existing status line keeps running."
- **Mac & sensors:**
  - Temperature unit °C / °F.
  - **Fan helper:** status (Not installed / Installed / Needs approval), "Remove fan helper", and the note "Fans return to Auto when Quota Deck quits or the Mac sleeps."
  - Keep Awake default.
- **Docker:** show Docker on/off, the docker command path, and the refresh interval.
- **Alerts:** rows with switches and number fields:
  - CPU above 80 °C;
  - free disk below 20 GB;
  - memory pressure critical;
  - any AI quota below 10%;
  - container became unhealthy.
  - "Quiet" for an hour or until tomorrow.
- **Data:**
  - History kept on this device (7 / 30 / 90 days).
  - Folder access (choose / revoke; "Read-only session logs").
  - Export.
  - Sample deck (Show a sample / Show my machine).
- **Language:** Match system, English, Türkçe, Deutsch, Español. "Every language names itself."
- **Startup:** open at login.
- **About:** the frontispiece illustration, version, "Local. Read-only. No account. No network.", acknowledgements and licences.

### 5. Flows and dialogs

- **Fan approval:**
  1. Click Cool.
  2. A paper sheet explains in plain words why administrator approval is needed and what the helper can and cannot do: "It can only set fan speed. Nothing else."
  3. A button "Continue" leads to the native macOS admin prompt (show it as context).
  4. Show both outcomes: approved (fan spins up, a toast "Cool mode on") and declined (the mustard "Unchanged — needs approval" card).
- **First launch:** a three-step paper onboarding: welcome (frontispiece), choose folder access, pick the menu bar content. Skippable.
- **Notifications:** macOS banners in Claude voice, for example "Claude weekly quota is at 10%. It resets in 2 d 4 h." and "CPU is at 92°. Quota Deck has not changed your fans."
- **Toasts and inline errors:** e.g. "Couldn't read Docker. Is Docker Desktop running?"

### 6. States sheet

Loading (paper skeletons with a shimmer of the dot texture), empty, error, not installed, needs access, Docker asleep, RAM sensor unavailable, Reduce Texture on, high contrast, and a narrow popover with long Turkish strings.

### 7. Component sheet

- Ink-wash gauge (horizontal, vertical, dial) with push-pin pace marker.
- Health Seal.
- Thermal Atlas (full and compact).
- Ornament divider.
- Capsule nav with sliding indicator.
- Segmented control.
- Ink pill and hairline pill buttons.
- Chips: status, temperature and badges (Estimated / Measured / agy live).
- Ledger table row.
- Timeline spine.
- Paper card.
- Toggle switch (olive when on), number field.
- Tooltip card.
- Toast.
- Every chart type.

### 8. Tokens sheet

- Colours (light and dark), the temperature scale and status colours.
- Type scale: serif display 52 / 38 / 32 / 22 / 19, sans 15 / 14 / 13 / 11, mono 12 / 11.
- Spacing on a 4/8 grid.
- Radii: 6 / 10 / 16 / 20 / pill.
- Shadows.
- Dot texture spec.
- Motion curves and durations.
- Theme-switch mapping: Liquid token → Claude token, one table.

## Constraints

- It will be built in React with plain CSS variables inside a Tauri web view. Every effect must be CSS or SVG:
  - hatching as SVG patterns;
  - isotherms as SVG paths;
  - the dot paper as `radial-gradient`.
- Fonts: Source Serif 4, Inter and JetBrains Mono, all bundled locally. No web font requests at runtime.
- Accessibility:
  - 4.5:1 text contrast in both modes (check clay text: use Clay ink `#B5573A` on light);
  - hit targets at least 28 × 28 pt;
  - colour never the only signal;
  - every chart has a text summary;
  - every illustration is decorative (`alt=""`) or described.
- Restraint: one clay-accented element per card at most. If a screen feels busy, remove, don't shrink. No glow, no glassmorphism, no gradients for decoration, no emoji.
