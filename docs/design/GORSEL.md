# Quota Deck — Görsel prompt'ları (isteğe bağlı)

Bu görseller zorunlu değil. Uygulama şu an CSS ile çizilmiş boş durumlar ve animasyonlar kullanıyor; görseller gelirse onların yerine geçer.

## Nasıl üretilir

- Antigravity'de (Gemini görsel modeli) **her seferinde 1–2 görsel** üret. Paralel/toplu üretim görsel kotasını dakikalar içinde bitiriyor (429) ve ~5 saat bekletiyor.
- Hepsi **şeffaf arka planlı PNG**. Model şeffaflık vermezse düz `#FF00FF` arka planla üret, sonra arka planı sil.
- Bitince dosyaları `~/Downloads`'a bırakman yeterli. Ben `ui/src/assets/illustrations/` altına taşır, adlarını uydurur ve bağlarım.
- Her prompt İngilizce, çünkü model İngilizcede daha tutarlı.

Ortak stil cümlesi (her prompt'un sonuna eklendi):

> Apple product illustration style, macOS 27 Liquid Glass aesthetic, soft frosted glass and brushed aluminium materials, gentle studio lighting from the top left, subtle refraction and caustics, muted cool palette (slate blue #4E6E8E, silver #A3ADB8) with one restrained accent, isometric three-quarter view, crisp edges, no text, no logos, no people, transparent background, PNG, 2048×2048, centred with generous empty margin.

---

## 1. Docker kapalı — uyuyan konteyner yığını

**Dosya adı:** `docker-asleep.png` · **Nerede:** Docker sekmesi, Docker Desktop çalışmıyorken.

```
Three small shipping-container shaped glass blocks stacked loosely like sleeping cats, slightly tilted, their glass dimmed and frosted as if powered down, a tiny soft "z z" drifting upward made of three floating glass beads (not letters), faint cool blue inner glow nearly off, calm and quiet mood. Apple product illustration style, macOS 27 Liquid Glass aesthetic, soft frosted glass and brushed aluminium materials, gentle studio lighting from the top left, subtle refraction and caustics, muted cool palette (slate blue #4E6E8E, silver #A3ADB8) with one restrained accent, isometric three-quarter view, crisp edges, no text, no logos, no people, transparent background, PNG, 2048×2048, centred with generous empty margin.
```

## 2. Docker kurulu değil

**Dosya adı:** `docker-missing.png` · **Nerede:** Docker sekmesi, Docker bulunamadığında.

```
A single empty glass shipping-container outline standing on a thin aluminium plinth, the container drawn as a clear glass shell with nothing inside, a soft dashed outline of light around its base suggesting "a place for something", serene and patient. Apple product illustration style, macOS 27 Liquid Glass aesthetic, soft frosted glass and brushed aluminium materials, gentle studio lighting from the top left, subtle refraction and caustics, muted cool palette (slate blue #4E6E8E, silver #A3ADB8) with one restrained accent, isometric three-quarter view, crisp edges, no text, no logos, no people, transparent background, PNG, 2048×2048, centred with generous empty margin.
```

## 3. Araç kurulu değil (AI)

**Dosya adı:** `tool-missing.png` · **Nerede:** AI sekmesi, Claude/Codex/Antigravity bulunamadığında.

```
An empty liquid-glass capsule gauge lying horizontally, like a thermometer tube with no liquid inside, a faint diagonal hatch pattern etched in the glass, a small aluminium cap at one end, quietly waiting. Apple product illustration style, macOS 27 Liquid Glass aesthetic, soft frosted glass and brushed aluminium materials, gentle studio lighting from the top left, subtle refraction and caustics, muted cool palette (slate blue #4E6E8E, silver #A3ADB8) with one restrained accent, isometric three-quarter view, crisp edges, no text, no logos, no people, transparent background, PNG, 2048×2048, centred with generous empty margin.
```

## 4. Dashboard arka plan dokusu

**Dosya adı:** `backdrop-dark.png` ve `backdrop-light.png` · **Nerede:** popover'ın camın arkasındaki arka plan (şu an CSS gradyanı).

Koyu:
```
Abstract wallpaper of soft overlapping light fields: deep navy base, a cobalt glow top-left, a violet glow top-right, a warm copper glow bottom-right, a teal glow bottom-left, two very faint flowing translucent ribbons crossing horizontally, extremely smooth gradients, no noise, no objects, no text, macOS 27 wallpaper style, 2880×1800 PNG.
```

Açık:
```
Abstract wallpaper of soft overlapping pastel light fields: pale sky-blue base, a light periwinkle glow top-right, a peach glow bottom-right, a mint glow bottom-left, two very faint flowing translucent white ribbons crossing horizontally, extremely smooth gradients, no noise, no objects, no text, macOS 27 wallpaper style, 2880×1800 PNG.
```

## 5. Uygulama ikonu varyasyonları

**Dosya adı:** `icon-a.png`, `icon-b.png`, `icon-c.png` · 1024×1024, macOS squircle içinde.

```
macOS 27 app icon in the Liquid Glass style, a rounded-square (squircle) glass tile with a soft silver rim, inside it a single vertical liquid-glass capsule gauge three-quarters full of slate-blue liquid with a bright meniscus highlight, a thin white pace tick across the capsule, subtle depth and refraction, gentle top-left light, no text, no letters, 1024×1024 PNG with transparent corners.
```

```
macOS 27 app icon in the Liquid Glass style, a squircle glass tile containing three short horizontal liquid-glass bars stacked vertically at different fill levels (copper, silver, violet), each with a glossy meniscus, frosted background behind them, no text, 1024×1024 PNG with transparent corners.
```

```
macOS 27 app icon in the Liquid Glass style, a squircle tile shaped like a tiny frosted-glass chip die seen from above, four zones glowing softly in cool blue with one zone warm amber, a thin aluminium frame, no text, 1024×1024 PNG with transparent corners.
```

---

## Bağlantı noktaları (üretildi ve bağlandı)

| Görsel | Kod | Durum |
|---|---|---|
| `docker-asleep.png` | `ui/src/liquid/DockerTab.tsx`, `OverviewTab.tsx`, `Popover.tsx` | Bağlandı (`ui/src/assets/illustrations/docker-asleep.png`) |
| `docker-missing.png` | `ui/src/liquid/DockerTab.tsx`, `OverviewTab.tsx`, `Popover.tsx` (`installed=false`) | Bağlandı (`ui/src/assets/illustrations/docker-missing.png`) |
| `tool-missing.png` | `ui/src/liquid/AiTab.tsx` → `PercentTool`, kurulu değil durumu | Bağlandı (`ui/src/assets/illustrations/tool-missing.png`) |
| `backdrop-*.png` | `ui/src/styles/themes/liquid.css` → `.backdrop` ve `.dash::before` | Bağlandı (`ui/src/assets/illustrations/backdrop-dark.png`, `backdrop-light.png`) |
| `icon-*.png` | `ui/src/assets/illustrations/icon-{a,b,c}.png` (`app/icons/` için adaylar) | Üretildi (Kutluhan birini seçtiğinde `app/icons/` güncellenir) |
