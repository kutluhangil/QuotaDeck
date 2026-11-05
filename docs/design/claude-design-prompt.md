# Quota Deck v2 — Claude Design prompt

Design a macOS menu bar app called **Quota Deck**: one dark, native-feeling popover that shows (1) the health of my Mac mini and (2) the quota/cost of the three AI coding tools I use — Claude Code, Codex and Antigravity — plus (3) my Docker containers. Everything is local and read-only; the app never touches the network.

## Platform and frame
- macOS 26 (Tahoe) menu bar popover, attached to a status item. Width **380 px**, height up to **640 px**, inner content scrolls; header and tab bar stay pinned.
- Dark mode first (also deliver a light variant of the System tab). Translucent vibrancy background, 12 px corner radius, cards on a slightly lighter surface with 1 px hairline borders, 10–12 px card radius, 12 px gutters.
- Typography: SF Pro Text / SF Pro Rounded for big numerals, SF Mono for byte/token counts. Tabular figures everywhere numbers update live.
- Density: information-dense like iStat Menus / Stats, but calm. No gradients-for-decoration, no glassy glow, no emoji.

## Menu bar item (design it too)
A compact status item, monospace, ~140 px: `◔ 18%  42°  ✳ 67%` → CPU load, CPU temperature, and the most constrained AI quota (% left). Show a variant where a quota is under 15% (amber) and under 5% (red).

## Header (pinned)
- Left: machine name "Mac mini (2024) · Apple M4 · 24 GB", subtitle "macOS 26.2 · up 1 d 23 h".
- Right: a single **health score** pill (e.g. `92 Good`) and a settings gear.
- Below: icon + label tab bar with 6 tabs: **Overview · Mac · Docker · Claude · Codex · Antigravity**. If 6 do not fit at 380 px, use icon-only tabs with the active one showing its label.

## Tab 1 — Overview (the default)
A one-glance summary, top to bottom:
1. 2×2 grid of mini cards: CPU (% + temp), Memory (% + pressure word), Disk (free TB + bar), Fan (RPM).
2. "AI quotas" block: one row per tool (Claude, Codex, Antigravity) with tool glyph, a thin progress bar for the tightest window, "% left" and "resets in 4 h 40 m". Tools that are not installed show a muted "Not installed" row, never a fake value.
3. Docker strip: "● Running · 8 containers · 3.1 GB RAM" (or "Docker not running", muted).

## Tab 2 — Mac (system sensors)
Cards in a 2-column grid, each with a title row (icon + name left, chip right), one big number, one small sparkline or bar, one caption line:
- **CPU** — load %, chip with temperature `42 °C`, per-cluster mini bars (E-cores / P-cores), sparkline of last 60 s, caption "10 cores · idle".
- **GPU** — load %, temp chip, caption "0.0 W".
- **Memory** — used %, chip with pressure ("Normal" green / "Warning" amber / "Critical" red), bar, caption "14.7 GB / 24 GB · swap 118 MB".
- **Memory temperature** — design this card with a real value AND an "unavailable on this Mac" muted state (Apple Silicon may not expose it).
- **SSD** — free space big number "1.7 TB", used bar, temp chip, caption "345 GB used · 17% · read 863 KB/s · write 35 MB/s".
- **External disks** — compact list rows (name, used/total bar, read/write).
- **Fan** — RPM big number, chip "Auto", caption "Managed by macOS". Read-only: no Cool/Max buttons.
- **Power** — total package watts with CPU/GPU split (small).
- **Network** — down/up KB/s with a dual sparkline (local counters only).
- **Top processes** — table (name, memory, % CPU), 5 rows, toggle "CPU | Memory".
Temperature chips use a 3-step scale: neutral < 70 °C, amber 70–90 °C, red > 90 °C.

## Tab 3 — Docker
- Summary row: status dot + "Running", 4 stat tiles: Containers, CPU (x / N vCPU), RAM (3.1 GB / 4.8 GB VM), Disk (44 GB images + volumes).
- Container table: name, state dot, CPU %, RAM; grouped by compose project with collapsible headers; "+14 more" footer.
- Empty states: "Docker Desktop not running" and "Docker not installed".

## Tabs 4–6 — Claude, Codex, Antigravity (one shared layout, tool accent color)
Accent: Claude = warm terracotta, Codex = neutral graphite/white, Antigravity = cool blue-violet. Accent is used only on bars and the active tab, never on text blocks.
1. Identity row: tool name, plan chip (e.g. "Pro"), "Updated just now".
2. **Session window** card: "Session 100% left", "Resets in 4 h 40 m", progress bar with a pace marker (where you *should* be if usage were even), caption "7% in reserve · lasts until reset".
3. **Weekly window** card: same structure; caption can be a warning "18% over pace · runs out in 2 d 3 h".
4. Stats grid (2×3): Today cost, Current window cost, Latest tokens, Window tokens, 30-day cost, 30-day tokens.
5. 30-day daily cost bar chart (thin bars, max label top-right, no gridlines).
6. "Recent windows" list: date range + cost + tokens, 4 rows.
7. Footer caption: "Top model: …" and a **confidence badge**: "Measured" (solid) vs "Estimated" (outlined, with a tooltip "Estimated from local logs at API rates"). Every estimated number must carry this badge — design it so it is visible but not loud.
8. States: Not installed, No data yet, Logs unreadable (permission needed — with a single "Grant access" button).

## Bottom bar (pinned, all tabs)
Small text buttons: Refresh ⌘R · Settings ⌘, · Quit ⌘Q, plus "Updated 3 s ago".

## Deliverables
- All 6 tabs in dark mode at 380 × 640, plus the Mac tab in light mode.
- Menu bar item in 3 states (normal, amber, red).
- A component sheet: stat card, temp chip, pressure chip, progress bar with pace marker, sparkline, process/container row, confidence badge, empty state.
- Design tokens (colors, type scale, spacing, radii) as a short table so it can be implemented in React + Tailwind inside a Tauri webview.
- Accessibility: 4.5:1 contrast for text, color is never the only signal (chips also carry a word), numbers have VoiceOver-friendly labels.
