<div align="center">

<img src="docs/media/hero.svg" alt="Quota Deck — coding-agent quota windows in your menu bar" width="100%">

<p>
  <a href="https://github.com/kutluhangil/QuotaDeck/releases"><img alt="Download" src="https://img.shields.io/badge/download-signed%20DMG-e07a5a?style=for-the-badge&labelColor=0a0d14"></a>
  <img alt="Rust" src="https://img.shields.io/badge/Rust-Tauri%202-5ac8fa?style=for-the-badge&labelColor=0a0d14">
  <img alt="Privacy" src="https://img.shields.io/badge/network%20requests-zero-a3e635?style=for-the-badge&labelColor=0a0d14">
  <img alt="License" src="https://img.shields.io/badge/license-GPL--3.0-e07a5a?style=for-the-badge&labelColor=0a0d14">
</p>

<p><em>Coding agents stop you mid-task when a five-hour window or a weekly ceiling runs out,<br>
and the only place that number lives is a page you have to go and look at. Quota Deck puts it in your menu bar.</em></p>

<img src="docs/readme/v2-popover.png" alt="The Liquid Glass popover with quota windows for each tool" width="100%">

</div>

<img src="docs/media/divider.svg" width="100%" alt="">

## What it does

Quota Deck reads the session logs your tools already write to disk, folds them into the limits each tool enforces, and tells you how full every window is, when it resets and whether your current pace gets you there. **It never signs in, never reads a credential and never makes a network request.**

On macOS it also watches the Mac itself (CPU, GPU, SSD and SoC temperatures, fans, power, memory, disk, network, processes and a health score) and your Docker containers, in a Liquid Glass popover and a four-tab dashboard.

<table>
  <tr>
    <td width="50%"><img src="docs/readme/v2-overview.png" alt="The dashboard overview"></td>
    <td width="50%"><img src="docs/readme/v2-mac.png" alt="The Mac monitor tab"></td>
  </tr>
  <tr>
    <td align="center"><code>overview</code> · every tool, every window</td>
    <td align="center"><code>mac</code> · sensors, processes, Docker</td>
  </tr>
</table>

<img src="docs/media/divider.svg" width="100%" alt="">

## Tools it reads

| Tool | Where it reads | Limits |
|---|---|---|
| **Claude Code** | `~/.claude/projects` + the optional status line | Measured 5 h and 7 d windows via the status line |
| **Codex** | `~/.codex/sessions` | Measured: Codex writes its own rate limits |
| **Antigravity** | `~/.gemini/antigravity-cli/history.jsonl` and CLI logs | Measured "model exhausted, reopens at" |
| **Copilot CLI** | `~/.copilot/session-state` | Exact ceiling from the published plan allowance |
| **Gemini CLI** | `~/.gemini/tmp/*/chats` | Tokens and cost |
| **OpenCode · pi · Amp · Droid** | Their local session stores, read-only | Tokens and cost |

<img src="docs/media/divider.svg" width="100%" alt="">

## How it works

```mermaid
flowchart LR
  LOGS[Local session logs<br/>read-only] --> PR[providers crate<br/>one parser per tool]
  PR --> CORE[core crate<br/>windows · pace · cost]
  SYS[sysmon crate<br/>sensors · Docker] --> APP[Tauri 2 app]
  CORE --> APP
  APP --> MB[Menu bar · Liquid Glass popover]
  APP --> DB[Four-tab dashboard]
  CORE --> CLI[quotadeck CLI]
```

A Cargo workspace: `core` (windows, pace, pricing), `providers` (log parsers), `sysmon` (Mac sensors), `app` (Tauri shell), `cli`, and a React UI in `ui/`. Windows and Linux builds cover the quota side only.

<img src="docs/media/divider.svg" width="100%" alt="">

## Türkçe özet

**Quota Deck**, kodlama ajanlarının (Claude Code, Codex, Antigravity, Copilot CLI ve diğerleri) 5 saatlik ve haftalık kullanım limitlerini menü çubuğunda gösteren bir uygulama. Araçların diske zaten yazdığı oturum kayıtlarını okuyor; giriş yapmıyor, hiçbir anahtar okumuyor, ağa tek bir istek atmıyor. Hangi pencerenin ne kadar dolduğunu, ne zaman sıfırlanacağını ve bu hızla limite yetişip yetişmeyeceğini söylüyor. macOS'ta ayrıca Mac'in sıcaklık, fan, bellek ve disk durumunu ve Docker konteynerlerini izliyor.

<img src="docs/media/divider.svg" width="100%" alt="">

## Install and build

Download `QuotaDeck-<version>-macos-arm64.dmg` from [Releases](https://github.com/kutluhangil/QuotaDeck/releases). It is signed with a Developer ID and notarised. Apple silicon, macOS 13+; the Liquid Glass popover needs macOS 26.

```sh
npm --prefix ui install
npm --prefix ui exec tauri -- dev       # run with hot reload
npm --prefix ui exec tauri -- build     # bundle for this platform
cargo run -p quotadeck-cli -- status    # the command line
cargo test --workspace && npm --prefix ui test
```

<div align="center">
<br>
<a href="https://github.com/kutluhangil"><img src="docs/media/signature.svg" width="100%" alt="Built by kutluhangil"></a>
<sub>GPL-3.0 · privacy by construction: it reads files, and that is all it does.</sub>
</div>
