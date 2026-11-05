# Quota Deck v2 — Claude Design prompt (Liquid Glass edition)

Paste everything below the line into Claude Design.

---

## The brief in one paragraph

Design **Quota Deck**, a macOS 27 menu bar app that feels as if Apple shipped it as a first-party utility: one glance tells me how my Mac is doing (temperatures, memory, disk, fan, power), what my Docker containers are doing, and how much quota I have left on the three AI coding tools I use: **Claude Code, Codex and Antigravity**. It has two surfaces: a **Control Center–style glass popover** that drops from the menu bar, and a **full Dashboard window** for depth. Everything is local and read-only; the app never touches the network. I want something premium and original that nobody has made before, not another iStat clone.

## Design language: "Liquid Instruments"

Build on Apple's **Liquid Glass** (macOS 26/27), but give it a signature of its own.

**1. Glass is the chrome, not the content.** Follow Apple's two-layer rule:

- The *functional layer* is Liquid Glass. It covers the popover modules, the floating tab bar, the toolbar and the segmented controls. Glass has no color of its own: it refracts and blurs what is behind it, has a thin specular rim highlight on the top edge, and slightly lenses the background at its curved edges.
- The *content layer* is calm and opaque-ish. It holds the charts, tables and numbers on standard materials (`regular` / `thick`) with vibrant labels.
- Never stack glass on glass.

**2. The signature idea: liquid levels inside glass vessels.** Every "how full is it" value is shown as a **liquid level inside a glass capsule or pill**. The liquid has a soft meniscus, a subtle inner caustic highlight and a gentle settle animation when the value changes. This applies to CPU load, memory, disk, quota left and fan speed. It is the one visual idea that makes the app recognisable. Use it with restraint: one vessel per module, never decorative.

- **Quota capsules drain** as you use them. Remaining quota is the liquid; a hairline **pace marker** on the glass shows where you *should* be if usage were even.
- **Temperature colours the liquid**, never the chrome: cool graphite-blue below 60 °C, warm amber at 70–90 °C, ember red above 90 °C. The shift is a smooth gradient through temperature, not three hard steps. The number always carries the meaning too: colour is never the only signal.

**3. The second signature: the SoC Heat Field.** On the Mac page, show a stylised, abstract top-down **silicon die**: a rounded rectangle split into zones for **P-cores, E-cores, GPU, SoC fabric, and SSD (NAND)** off to the side. Each zone glows softly with its live temperature, like a thermal camera rendered in glass. It is not a photo of a chip; it is an elegant diagram of where the heat is right now. Hovering a zone shows its average, max and sensor count. No other monitor has this.

**4. Typography carries hierarchy.**

- SF Pro Display for hero numerals: big, light weight, tight tracking. SF Pro Text for labels. SF Mono only for bytes, PIDs and token counts.
- Tabular figures on every live number so digits never jitter.
- Numbers morph between values (digit roll, ~250 ms spring), they do not blink.

**5. Near-monochrome.**

- The UI is graphite/silver. Colour appears only in liquid fills, status chips and the single prominent action.
- Each AI tool gets one accent, used **only** inside its capsules and charts: Claude = warm terracotta, Codex = cool white/graphite with a faint mint edge, Antigravity = blue-violet.

**6. Motion is physics.** Springs that settle quickly, no bounce on data. Charts draw in once, then update in place. Honour Reduce Motion (static fills, no caustics) and Reduce Transparency (glass becomes a solid elevated graphite surface; the design must still look premium in that fallback).

**7. Charts in the Swift Charts idiom.**

- Thin 1.5 pt lines.
- Area fills that fade to transparent.
- Rounded-cap bars.
- No gridlines, only a faint baseline and two quiet axis labels.
- Hover or drag to scrub, with a lollipop marker and a floating glass value label.
- Sparklines have no axes at all.

**8. Dark first, light too.** Dark base `#000`-family with elevated steps around `#1c1c1e`, text near `#f5f5f7`. Also deliver light mode. Show the glass over the default macOS Tahoe wallpaper so the refraction is visible.

## Surface 1 — Menu bar item

A compact text item in SF Mono-like tabular figures: `48° · 76% · ✳ 67%`, meaning CPU temperature, memory used, and the tightest AI quota left (the glyph of whichever tool it is).

Design four states:

- Normal.
- Quota under 15%: amber dot.
- Quota under 5% or thermal "Serious": red dot.
- Compact mode: icon only.

## Surface 2 — The glass popover (Control Center style, ~360 × 640 pt)

Model it on macOS Control Center: **rounded glass modules of different sizes in a 4-column grid**, big round icon wells, and a full-width slider-like module at the bottom. A small header sits above the modules.

- **Header**: machine "Mac · Apple M6 · 32 GB", "up 1 d 23 h", and a **Health** pill: a small glass vessel with the score `92` and a word (Excellent / Good / Fair / Poor). Settings gear trailing.
- **Large module (2×2) — AI quota.** Three stacked liquid capsules (Claude, Codex, Antigravity), each with the tool glyph, "% left" for the tightest window, and "resets in 4 h 40 m". Antigravity has no percentage. Its capsule shows per-model state: "Flash: available", "Pro: exhausted · opens in 4 h 52 m".
- **Medium modules (2×1):**
  - CPU: % + temperature + liquid vessel.
  - Memory: % + pressure word.
  - Disk: free TB + SSD temperature.
  - Docker: "● 8 running · 3.1 GB".
- **Small round modules (1×1):**
  - Fan: RPM, ring-shaped liquid.
  - Power: watts.
  - Network: ↓/↑ mini wave.
  - Keep Awake: a toggle button with a filled active state.
- **Full-width module:** a 60-second **thermal ribbon**, a thin flowing band coloured by temperature with the 5-minute peak marked.
- **Footer:** a capsule button "Open Dashboard" (the one tinted prominent action), plus a quiet "Updated 2 s ago".

## Surface 3 — Dashboard window (~1180 × 760 pt, resizable)

The content scrolls edge-to-edge beneath a **floating glass toolbar** with a scroll-edge blur.

Toolbar, centred: a glass tab bar with **Overview · Mac · Docker · AI**. Trailing: a time-range segmented control (**1 h · 24 h · 7 d**) and a search field (it filters processes and containers).

### Overview

- **Hero row:** a large Health vessel with the score and one sentence ("All readings normal" / "Memory pressure is high"), next to the AI quota capsules for the three tools in a horizontal row.
- **Second row:** a compact SoC Heat Field, plus small cards for Memory, Disk, Fan, Power and Network.
- **Third row:** Docker summary and the top 5 processes.

### Mac

A bento grid of cards on material:

- **CPU:** % with an E-cluster / P-cluster split (two liquid columns), load average 1/5/15, a per-core mini bar strip, temperature (P-core avg/max, E-core avg), and a 60 s sparkline that expands into the selected range chart.
- **GPU:** %, temperature, watts.
- **Memory:** used / total, a pressure chip (Normal / Warning / Critical), compressed, swap used/total, and an area chart.
- **Memory temperature:** show **both states**. With a value ("Memory proximity 46 °C") on Macs that expose it, and a dignified empty state, "This Mac doesn't expose a memory sensor", on Macs that don't. Never a fake number.
- **Thermal:** the **full SoC Heat Field** as the hero of this page. Beside it, the macOS thermal state chip (Nominal / Fair / Serious / Critical), the 5-minute peak, and a 24 h temperature chart with CPU, GPU and SSD lines.
- **Storage:** internal SSD free/used vessel, read and write rates (dual sparkline), NAND temperature, a SMART chip "Verified"; external disks as compact rows.
- **Fan:** RPM, % of max, min/max range drawn as the vessel's scale, and "Managed by macOS". **Read-only.** Show a disabled, clearly secondary "Fan control: coming later" note, not live buttons.
- **Power:** system total watts big, with a stacked bar for CPU / GPU / ANE / DRAM.
- **Network:** ↓/↑ rates and a mirrored wave chart (down above, up below the baseline). No latency or ping.
- **Processes table:** name with app icon, memory, % CPU with an inline liquid bar, energy (W), PID, and a `…` menu. Sortable columns, the hover row raised slightly, and a count in the header "Processes (157)".

### Docker

- **Status header:** "● Running · Docker Desktop" or the empty states "Docker Desktop is not running" (with an "Open Docker" button) and "Docker is not installed".
- **Four stat tiles:** Containers (running/total), CPU (x of N vCPU), Memory (3.1 GB of 4.8 GB VM limit, as a vessel), Disk (images + volumes + build cache, as a stacked bar).
- **Container list grouped by Compose project:** collapsible glass section headers with the project's totals. Each row shows a state dot, name, image tag, health chip (healthy / unhealthy / none), CPU %, memory, uptime and restart count.

### AI

One page per tool through a secondary segmented control (**Claude · Codex · Antigravity**). Claude and Codex share one layout:

1. **Two big liquid capsules: Session window and Weekly window.** Each shows "% left", the reset countdown, the pace marker, and a sentence: "7% in reserve · lasts until reset" or "18% over pace · runs out in 2 d 3 h" (amber).
2. **Stat grid:** Today cost, Current window cost, Window tokens, 30-day cost, 30-day tokens, Top model.
3. **30-day daily cost chart:** rounded bars in the tool's accent; scrub to see the day.
4. **Recent windows:** date range, cost, tokens, and a thin capsule showing how full that window got.
5. **Confidence badges.** Every estimated number carries an outlined **"Estimated"** badge (tooltip: "Estimated from local logs at API rates"); measured numbers carry a subtle solid **"Measured"** badge. Make it visible but quiet. Trust is the product.

**Antigravity variant:** no percentages, because only exhaustion events are measured locally.

- A **per-model status list**: model name, state (Available / Exhausted), and "opens in 4 h 52 m" with a draining countdown capsule.
- An **activity** chart of conversations per day.
- A footnote: "Remaining quota isn't written to local files, so Quota Deck shows what it can measure."

**States for every tool:** Not installed (muted, with the tool glyph in outline), No data yet, Needs folder access (one "Grant access" button).

### Alerts & Settings (a sheet, not a page)

Rows with switches:

- CPU temperature above [80 °C].
- Free disk below [20 GB].
- Memory pressure critical.
- Any AI quota below [10%].
- Container turned unhealthy.

Also: a refresh rate segmented control (Live 1 s · Balanced 2 s · Saver 10 s), "Show in menu bar" choices, and the tool visibility toggles (Claude, Codex, Antigravity on; others off).

## Content to use in mocks (real readings from my Mac)

| Reading | Value |
|---|---|
| Machine | Apple M6, 32 GB, macOS 27.2 |
| CPU | 18 %, P-core 52.7 °C avg / 60.5 °C max, E-core 50.4 °C |
| GPU | 12 %, 49 °C |
| SSD (NAND) | 44 °C, SMART Verified |
| Memory | 76 %, pressure Normal, swap 5.1 of 6 GB |
| Fan | 999 RPM of 4900 max (20 %), Auto |
| Power | System 21.2 W |
| Claude | Session 100 % left (resets in 4 h 40 m); Weekly 67 % left (resets in 5 d 22 h); Today $10.23; 30-day $781.94 |
| Antigravity | One model exhausted, opens in 4 h 52 m |

## Deliverables

1. Menu bar item, 4 states.
2. The glass popover in dark and light mode, over the Tahoe wallpaper.
3. The Dashboard window: Overview, Mac, Docker, AI (Claude) and AI (Antigravity) in dark mode, plus the Mac page in light mode.
4. The Alerts & Settings sheet.
5. Empty and error states: Docker not running, tool not installed, memory sensor unavailable, Reduce Transparency fallback.
6. A component sheet: liquid capsule (horizontal, vertical, ring), pace marker, SoC Heat Field, temperature chip, pressure chip, confidence badges, glass tab bar, segmented control, chart styles (line, area, bars, mirrored wave, sparkline), process row, container row, health vessel.
7. Design tokens:
   - colours for dark and light, including the temperature gradient stops;
   - the type scale;
   - spacing (8 pt grid);
   - corner radii, concentric (outer radius = inner radius + padding);
   - glass parameters: blur, rim highlight, refraction strength, dispersion;
   - motion springs.

## Constraints

- It will be built in **React + Tailwind inside a Tauri web view**. Glass must be achievable with `backdrop-filter`, an SVG displacement filter for refraction, and a specular gradient rim, the way the open-source "Glisse" library recreates Liquid Glass on the web.
- Glass must degrade to an opaque elevated surface without losing its premium feel. Some builds cannot have a transparent window.
- Accessibility:
  - 4.5:1 text contrast in both modes;
  - hit targets at least 28 × 28 pt;
  - every chart has a text summary for VoiceOver;
  - colour is never the only signal.
- Restraint check before you finish: remove one element from every screen if you can. No gradients for decoration, no neon, no glow, no emoji, no skeuomorphic gauges or speedometers.
