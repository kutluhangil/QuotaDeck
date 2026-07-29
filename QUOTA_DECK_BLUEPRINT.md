# QUOTA DECK — AI Kullanım Limiti Takip Uygulaması

> **Blueprint v1.0** · Hazırlanma: 25 Temmuz 2026
> **Hedef:** macOS menü çubuğu + Windows tray uygulaması. Mac App Store (ücretli) ve Microsoft Store dağıtımı.
> **Stack:** Tauri v2 (Rust core + React 19/TypeScript UI)
> **Yürütme:** Claude Code, faz faz. Her faz sonunda kullanıcı manuel commit atar.

---

## 0. ÖNCE OKU — Bu Projeyi Şekillendiren 3 Karar

Bu üç karar mimarinin tamamını belirliyor. Değiştirirsen alttaki her şey değişir.

### Karar 1 — SIFIR KİMLİK BİLGİSİ (zorunlu, tartışmaya kapalı)

**Uygulama hiçbir AI sağlayıcısının OAuth token'ını, session cookie'sini veya auth dosyasını OKUMAZ ve hiçbir sağlayıcı API'sine istek ATMAZ.**

Sebep — Anthropic'in Ocak/Şubat 2026'daki politika uygulaması:

- 9 Ocak 2026'da Anthropic sunucu tarafında bir önlem devreye aldı: abonelik OAuth token'ları resmî Claude Code CLI dışında çalışmıyor. Üçüncü parti araçlar `"This credential is only authorized for use with Claude Code and cannot be used for other API requests"` hatası aldı. OpenCode, Cline, Roo Code kırıldı.
- Şubat 2026'da Anthropic resmî olarak netleştirdi: Free/Pro/Max planlarının OAuth token'ları **yalnızca** Claude Code ve Claude.ai ile kullanılabilir; başka herhangi bir ürün, araç veya hizmetle kullanımı **yetkisizdir ve tüketici hizmet şartlarının ihlalidir.**
- Tüketici ToS Bölüm 3, hizmetlere "bot, script veya başka yollarla otomatik/insan-dışı erişimi" açıkça yasaklıyor; tek istisna Anthropic API Key ile erişim.
- OpenCode, "anthropic legal requests" gerekçesiyle Claude Pro/Max hesap anahtarı desteğini kaldırdı.
- Aynı örüntü Google tarafında da işledi: OpenClaw üzerinden Gemini'ye erişen ücretli plan aboneleri askıya alındı.

**Bunun senin için anlamı:** Keychain'den Claude Code OAuth token'ı okuyup usage API'sine soran mimari (TokenEater'ın yaptığı, muhtemelen CUStats'ın da yaptığı) **ücretli bir App Store ürünü için kabul edilemez risktir.** Üç ayrı ateş hattı:

1. Anthropic'ten takedown / yasal talep
2. Apple 5.2.1 (Legal - IP) kaldırması — üçüncü parti ToS ihlali eden ürünler
3. **En kötüsü:** Senin müşterilerinin AI hesapları askıya alınır. Ücretli ürün sattığın insanlara bunu yapamazsın.

**Alternatif — ve iyi haber:** Yerel log dosyalarını okumak bu kapsamda değil. O dosyalar kullanıcının kendi diskinde, kendi verisi. API'ye istek yok, kimlik bilgisi yok, ToS ihlali yok. Codex zaten gerçek `rate_limits` verisini loga yazıyor — kimlik bilgisine hiç gerek yok.

**Sonuç mimarisi:** %100 yerel, %100 offline, sıfır ağ trafiği. Bu bir kısıt değil — **ürünün en güçlü pazarlama argümanı.** "No account. No login. No network. Ever."

### Karar 2 — İki Katmanlı Veri Modeli

Her sağlayıcı için iki farklı bilgi seviyesi var. Karıştırma:

| Seviye | Ne | Kaynak | Kesinlik |
|---|---|---|---|
| **L1 — MEASURED** | Sağlayıcının kendi bildirdiği kalan limit % ve reset zamanı | Logda `rate_limits` benzeri alan | Kesin |
| **L2 — DERIVED** | Token/maliyet toplamları + kayan pencere yeniden inşası | JSONL token sayımları | Tahmin |

UI her satırda hangi seviyede olduğunu **görsel olarak** belirtir. Tahmini veriyi kesinmiş gibi göstermek, bu kategorideki uygulamaların 1 numaralı güven kaybı sebebi (CUStats yorumu: *"The clock for claude 2x hours was wrong and therefore useless"*).

### Karar 3 — Performans Bütçesi Bir Özelliktir

CUStats sürüm notundan alıntı (v1.8.0): *"Disk I/O reduced from ~1.8 GB/hour to ~22 MB/hour"* — batched writes ve debounced timer'larla düzeltmişler. Kullanıcı yorumu ise net: *"consumes an obscene amount of battery, and the menu-bar GUI is very glitchy."*

Bu bizim **1 numaralı rekabet avantajımız.** Sert bütçe:

```
Bellek (RSS)        < 60 MB   (Tauri + WebView gerçeği; native Swift 20 MB olurdu)
CPU (idle)          < 0.1%
Disk okuma          < 5 MB/saat  (incremental, byte-offset)
Disk yazma          < 2 MB/saat  (batched, 60 sn debounce)
Soğuk başlangıç     < 1.5 sn
İlk tam tarama      < 3 sn / 500 dosya
Ağ                  0 byte. Her zaman.
```

Bu sayılar README'ye ve store açıklamasına yazılır. Ölçülür. CI'da regresyon testi vardır.

---

## 1. Rakip Analizi — CUStats

Ana referans. App Store'da $9.99, tek seferlik satın alma, 3,1 MB, macOS 13+.

**Güçlü yanları:**
- Native Swift → 3,1 MB binary, hızlı
- Pace prediction (haftalık gidişat tahmini) — gerçekten faydalı
- Çoklu hesap (10'a kadar), staggered refresh
- GitHub-tarzı aylık heatmap
- Demo mode — satın almadan denenebiliyor
- "Data Not Collected" App Store gizlilik rozeti

**Zayıf yanları — bizim açığımız:**

| Zayıflık | Kanıt | Bizim yanıtımız |
|---|---|---|
| Sadece 2 sağlayıcı | Sürüm geçmişi: Codex desteği v1.6.0'da (Oca 2026) geldi. Pazarlama "multi-provider" diyor ama gerçek Claude + Codex | **15+ sağlayıcı** |
| Batarya/disk | v1.8.0: 1,8 GB/saat → 22 MB/saat düzeltmesi; kullanıcı yorumu batarya şikayeti | Sert perf bütçesi + CI regresyon testi |
| Kimlik bilgisi bağımlılığı | "An AI provider account with API access" gerektiriyor; `~/.codex/auth.json` okuyor | **Sıfır kimlik bilgisi** |
| Yanlış geri sayım | Yorum: *"The clock for claude 2x hours was wrong"* | L1/L2 ayrımı + belirsizlik göstergesi |
| Sadece İngilizce | App Store dil listesi: EN | TR + EN (sonra DE/ES) |
| Sadece macOS | — | macOS + Windows |
| Sandbox sürtünmesi | v1.3.0: *"user will be ask to grant the permission to access"* | Aynı kısıt bizde de var — ama tek seferlik, iyi tasarlanmış onboarding |
| Marka uyumluluk sorunu | v1.8.0: *"Fix App Store metadata compliance — restricted third-party and Apple terms removed"* | Baştan marka-temiz isimlendirme (bkz. §9) |

**Tasarım:** Site tema rengi `#D97359` — sıcak kil/terracotta. Bu, Anthropic'in kendi arayüz vurgu rengine (`#D97757`) neredeyse birebir aynı. Yani CUStats görsel kimliğini takip ettiği ürünün markasından ödünç almış. Biz bunu yapmayacağız (§7).

---

## 2. Mimari

```
┌──────────────────────────────────────────────────────────────┐
│  React 19 + TypeScript  (WebView)                            │
│  ─ Tray popover panel (380×520)                              │
│  ─ Dashboard window (960×640)                                │
│  ─ Onboarding / izin akışı                                   │
│  ─ Zustand store · TanStack Virtual · CSS custom properties  │
└───────────────────────────▲──────────────────────────────────┘
                            │  tauri::Event (push) + invoke (pull)
                            │  Rust ASLA UI'a ham JSONL göndermez
┌───────────────────────────┴──────────────────────────────────┐
│  Rust core                                                   │
│                                                              │
│  ┌────────────┐  ┌──────────────┐  ┌─────────────────────┐  │
│  │ Discovery  │→ │ Watcher      │→ │ Incremental Parser  │  │
│  │ (yol tara) │  │ (notify+     │  │ (byte-offset, mmap) │  │
│  │            │  │  debounce)   │  │                     │  │
│  └────────────┘  └──────────────┘  └──────────┬──────────┘  │
│                                                │              │
│  ┌─────────────────────────────────────────────▼──────────┐  │
│  │ Provider trait impls (15+)                             │  │
│  │ ClaudeCode · Codex · Kimi · Gemini · Copilot · Qwen …  │  │
│  └─────────────────────────────────────────────┬──────────┘  │
│                                                │              │
│  ┌──────────────┐  ┌───────────────┐  ┌────────▼─────────┐  │
│  │ Window Engine│  │ Pace Forecast │  │ Store (redb)     │  │
│  │ (kayan 5s/7g)│  │ (lineer+EWMA) │  │ batched, 60s     │  │
│  └──────────────┘  └───────────────┘  └──────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ Platform katmanı (cfg(target_os))                      │  │
│  │ macOS: security-scoped bookmarks · NSStatusItem        │  │
│  │ Windows: doğrudan FS · Shell_NotifyIcon                │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  AĞ KATMANI YOK. reqwest bağımlılığı YOK. Bu kasıtlı.        │
└──────────────────────────────────────────────────────────────┘
```

**Neden Tauri v2 ve neden native Swift değil:**

Native SwiftUI `MenuBarExtra` daha küçük (3 MB vs ~12 MB) ve daha az bellek yer (~20 MB vs ~60 MB). Ama Windows'u tamamen kaybedersin ve iki ayrı kod tabanı bakarsın. Tek geliştiricisin. Tauri v2 hem Mac App Store'a hem Microsoft Store'a resmî olarak dağıtılabiliyor. Karar: **Tauri.**

Bunun bedeli dürüstçe: CUStats 3,1 MB, bizimki ~12-15 MB olacak ve ~40 MB daha fazla RAM yiyecek. Bunu perf bütçesiyle telafi ediyoruz.

**Kritik Tauri notları:**
- Frontend'e `tauri-plugin-fs` **verilmez**. Tüm dosya erişimi Rust'ta, capability whitelist ile.
- Tray popover için `tauri-plugin-positioner` + `NSPanel` davranışı (macOS'ta focus çalmamalı).
- `tauri-plugin-autostart` login item için.
- `tauri-plugin-notification` eşik uyarıları için.
- Windows'ta WebView2 `offlineInstaller` modu **zorunlu** (Microsoft Store şartı).

---

## 3. Sağlayıcı Matrisi

Her sağlayıcı bir `Provider` trait implementasyonu. Yeni sağlayıcı eklemek = tek dosya.

### 3.1 Tier A — Gerçek limit verisi (L1 MEASURED)

#### Codex (OpenAI) — **en sağlam kaynak, burada başla**

```
Yol   macOS/Linux  ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl
      Windows      %USERPROFILE%\.codex\sessions\YYYY\MM\DD\rollout-*.jsonl
```

Aranan kayıt:

```json
{
  "timestamp": "2026-07-25T07:27:21.415Z",
  "type": "event_msg",
  "payload": {
    "type": "token_count",
    "info": {
      "total_token_usage": {
        "input_tokens": 5200, "cached_input_tokens": 2048,
        "output_tokens": 14, "reasoning_output_tokens": 0,
        "total_tokens": 5214
      },
      "last_token_usage": { "...": "..." },
      "model_context_window": 258400
    },
    "rate_limits": {
      "primary":   { "used_percent": 0.0,  "window_minutes": 299,   "resets_in_seconds": 17940 },
      "secondary": { "used_percent": 22.0, "window_minutes": 10079, "resets_in_seconds": 351406 }
    }
  }
}
```

- `primary` = 5 saatlik pencere (`window_minutes: 299`)
- `secondary` = haftalık pencere (`window_minutes: 10079` ≈ 7 gün)
- Bazı sürümlerde `plan_type` ve `resets_at` alanları da geliyor

**⚠️ BİLİNEN TUZAK — mutlaka ele al:** `rate_limits` sıklıkla `null` geliyor.
- `codex exec` modunda **her zaman** null (openai/codex#14728). Sunucu exec oturumlarına `x-codex-*` header'ı göndermiyor.
- Bazı CLI sürümlerinde rollout dosyalarında hep null (openai/codex#14880, v0.114.0).
- VS Code / app-server modunda ise dolu geliyor.

**Strateji:** Tüm oturum dosyalarını tarayıp `rate_limits != null` olan **en yeni** kaydı bul. Yaşını hesapla. 30 dakikadan eskiyse UI'da "bayat" rozeti göster. Hiç yoksa L2'ye düş.

#### Kimi (Moonshot)

```
Yol   ~/.kimi/sessions/<group-id>/<session-id>/wire.jsonl
      ~/.kimi-code/sessions/<workspace-id>/<session-id>/agents/<agent-id>/wire.jsonl
Env   KIMI_DATA_DIR (tek dizin veya virgülle ayrılmış liste)
```

Token alanları:

| Alan | kimi-cli | kimi-code |
|---|---|---|
| Input | `token_usage.input_other` | `usage.inputOther` |
| Output | `token_usage.output` | `usage.output` |
| Cache read | `token_usage.input_cache_read` | `usage.inputCacheRead` |
| Cache create | `token_usage.input_cache_creation` | `usage.inputCacheCreation` |

**Kurallar:**
- Yalnızca token kullanımı sıfır olmayan `StatusUpdate` mesajları sayılır.
- kimi-code logları `usage.record` satırları yazar. **Sadece turn-scoped kayıtlar sayılır** — session-scoped kayıtlar kümülatif toplamdır, ikisini toplarsan iki kat sayarsın.
- Kimi'nin 5s / haftalık / 7g kotaları var (Kimi Code platformu, `api.kimi.com/coding/v1`). CLI'da `/usage` ile görülüyor.
- ⚠️ Kimi'nin **iki ayrı kota sistemi** var: Kimi Code platform kotası ve Kimi Open Platform kotası. Bunlar birbirinden bağımsız ve eşleşmiyor. UI'da hangisini gösterdiğimizi açıkça etiketle.
- Fiyatlama: `kimi-for-coding` model adı korunur; 2026-04-20T15:28:10Z öncesi Moonshot K2.5, sonrası K2.6 fiyatlandırması.

#### Antigravity (Google) — **v2'ye ertele**

Artık IDE içinde native kota ekranı var: `Settings > Advanced Settings > Models` — model bucket'ları için gerçek zamanlı geri sayım.

Programatik erişim resmî değil. Mevcut açık kaynak çözümler önce yerel IDE bağlantısını deniyor (IDE'nin açık olması şart), başarısız olursa Google Cloud Code API'sine düşüyor.

Yerel veri klasörü:
```
macOS    ~/Library/Application Support/Antigravity/
Windows  %APPDATA%\Antigravity
Linux    ~/.config/Antigravity
```
Sürümler arası konum değişiyor.

**Karar: v1'e KOYMA.** Ticari üründe tersine mühendislik + Google ToS riski + kırılganlık. v2'de "deneysel" bayrağıyla, kapalı varsayılanla.

### 3.2 Tier B — Token/maliyet verisi (L2 DERIVED)

Hepsi yerel JSONL. Kimlik bilgisi gerekmez.

| Sağlayıcı | Kök dizin | Not |
|---|---|---|
| **Claude Code** | `~/.claude/projects/**/*.jsonl` | Mesaj başına `usage` objesi: `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`. `message.id` + `requestId` ile dedup **şart** |
| **OpenCode** | `~/.local/share/opencode/` | |
| **Amp** | Sourcegraph Amp veri dizini | |
| **Droid** | Factory Droid | |
| **Codebuff** | | |
| **Hermes Agent** | | |
| **pi-agent** | | |
| **Goose** | Block Goose | |
| **Kilo** | | |
| **Qwen Code** | `~/.qwen/` | |
| **GitHub Copilot CLI** | | |
| **Gemini CLI** | `~/.gemini/` | |
| **OpenClaw** | | ⚠️ Anthropic tarafından bloklandı; Gemini erişimi de askıya alındı. Yalnızca geçmiş veri gösterimi. |

> **Uygulama notu:** Bu listenin kanonik ve güncel kaynağı `ccusage` projesinin Data Sources dokümantasyonudur (`ccusage.com/guide/`). Her sağlayıcı için tam yol ve şema orada. Faz 0'da her birini kendi makinende doğrula — kurulu olmayanları `unavailable` olarak işaretle, sahte veri gösterme.

### 3.3 Claude Code için özel not — L1'e yükseltme yolları

Claude Code'un kalan limitini **kimlik bilgisi kullanmadan** almanın iki meşru yolu olabilir. Faz 0'da ikisini de doğrula:

1. **Statusline hook** — Claude Code, `settings.json` içindeki `statusLine.command` ile bir script'e her turn'de JSON gönderir. Kullanıcı bizim küçük shim script'imizi buraya kurarsa, Claude Code **bize kendi isteğiyle** veri iter. Kullanıcının kendi konfigürasyonu, resmî mekanizma, ToS temiz.
2. **OpenTelemetry export** — `CLAUDE_CODE_ENABLE_TELEMETRY=1` ile OTLP metrik çıkışı. Yerel bir OTLP receiver (localhost, 127.0.0.1) dinleriz. Resmî, dokümante mekanizma.

İkisi de opt-in kurulum gerektirir ama ikisi de tamamen meşru ve L1 kalitesinde veri verir. **Bu, CUStats'ın yapamadığı/yapmadığı şeydir ve ürünün teknik farkıdır.** Faz 0 çıktısı: hangisi çalışıyor, hangi alanları veriyor.

---

## 4. Veri Modeli

```rust
// ── Çekirdek tipler ───────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ProviderId {
    ClaudeCode, Codex, Kimi, GeminiCli, CopilotCli, Qwen,
    OpenCode, Amp, Droid, Codebuff, Hermes, PiAgent, Goose,
    Kilo, OpenClaw, Antigravity,
}

#[derive(Clone, Serialize)]
pub enum Confidence {
    /// Sağlayıcı kendi bildirdi. Kesin.
    Measured { reported_at: DateTime<Utc> },
    /// Yerel token sayımından türetildi. Tahmin.
    Derived  { basis: DerivationBasis },
    /// Veri var ama bayat.
    Stale    { last_seen: DateTime<Utc>, age: Duration },
    /// Araç kurulu değil veya log yok.
    Unavailable { reason: UnavailableReason },
}

#[derive(Clone, Serialize)]
pub struct QuotaWindow {
    pub label: String,           // "5 saatlik" | "haftalık"
    pub duration: Duration,
    pub used_percent: Option<f32>,
    pub resets_at: Option<DateTime<Utc>>,
    pub confidence: Confidence,
}

#[derive(Clone, Serialize)]
pub struct TokenRollup {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
    pub estimated_cost_usd: Option<f64>,   // LiteLLM fiyat tablosu, gömülü
}

#[derive(Clone, Serialize)]
pub struct ProviderSnapshot {
    pub id: ProviderId,
    pub display_name: String,
    pub installed: bool,
    pub windows: Vec<QuotaWindow>,         // genelde 2: 5s + haftalık
    pub today: TokenRollup,
    pub series: Vec<Bucket>,               // Horizon şeridi için, 5 dk kova
    pub pace: Option<PaceForecast>,
    pub last_activity: Option<DateTime<Utc>>,
}

// ── Provider trait ────────────────────────────────────────────

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn display_name(&self) -> &'static str;

    /// Bu makinede araç kurulu mu? Kök dizinleri döndür.
    /// Env override'ları burada ele alınır (örn. KIMI_DATA_DIR).
    fn discover_roots(&self) -> Vec<PathBuf>;

    /// Hangi dosyalar izlenecek (glob).
    fn watch_globs(&self) -> Vec<&'static str>;

    /// TEK bir JSONL satırını ayrıştır. Saf fonksiyon, I/O yok, panic yok.
    /// Ayrıştırılamayan satır → Ok(None). Asla Err ile akışı durdurma.
    fn parse_line(&self, line: &str) -> Result<Option<ParsedEvent>>;

    /// Ayrıştırılmış olaylardan snapshot üret.
    fn build_snapshot(&self, events: &EventIndex) -> ProviderSnapshot;

    /// Bu sağlayıcı L1 verebiliyor mu?
    fn supports_measured(&self) -> bool { false }
}
```

**Dedup kuralı (Claude Code için kritik):** Aynı mesaj birden fazla JSONL dosyasında görünebilir (resume, fork, sidechain). `(message.id, requestId)` çiftini `HashSet`'te tut. Görülmüşse atla. Bu yapılmazsa maliyet 2-3 kat şişer.

---

## 5. Parser Motoru — Performansın Kalbi

Bu bölüm rakibin batarya sorununu yaşamamamızın tek sebebi. Kısayol yok.

### 5.1 Incremental okuma

```rust
struct FileCursor {
    path: PathBuf,
    byte_offset: u64,      // bir sonraki okumanın başlayacağı yer
    inode: u64,            // dosya döndü mü tespiti (Windows: file_index)
    size_at_last_read: u64,
    partial_line: String,  // satır ortasında bittiyse buffer
}
```

**Akış:**
1. Watcher olayı gelir → 750 ms debounce (aynı dosyaya gelen olaylar tek tetiğe katlanır)
2. `metadata()` ile boyut oku. `size < size_at_last_read` ise **dosya rotate edilmiş** → cursor sıfırla, baştan oku
3. `seek(byte_offset)` → sondan itibaren oku
4. `partial_line` ile birleştir, `\n`'e böl, son eksik parçayı sakla
5. Her satır → `provider.parse_line()`
6. `byte_offset` güncelle

**Asla yapma:** dosyayı baştan okuma, `read_to_string`, her tick'te tüm dizini `glob`'lama.

### 5.2 İlk tarama (cold start)

```
1. discover_roots()  → hangi araçlar kurulu
2. Her kök için dosya listesi + mtime
3. mtime DESC sırala
4. İlk 50 dosyayı tam ayrıştır → UI'a hemen gönder (progressive render)
5. Kalanı rayon ile paralel, arka planda
6. Her cursor'ı redb'ye yaz
7. Sonraki açılışlarda 4. adım anında gelir (cursor'lar hazır)
```

Kullanıcı 300 ms içinde ilk veriyi görür. Tam tarama arka planda biter.

### 5.3 Yazma stratejisi

```rust
// YANLIŞ — CUStats'ın 1,8 GB/saat hatası
on_every_event(|e| { db.write(e); });

// DOĞRU
struct BatchedStore {
    pending: Vec<Delta>,
    last_flush: Instant,
}
// Koşul: pending.len() > 500 VEYA elapsed > 60s VEYA app_quitting
```

Ek: `redb` tek dosya, WAL yok, mmap tabanlı. SQLite'a göre bu iş yükünde belirgin daha az disk yazar.

### 5.4 Watcher disiplini

- `notify` crate, recursive **kapalı** — sadece bilinen alt dizinleri izle
- Codex `sessions/YYYY/MM/DD/` üretiyor → sadece **bugünün** ve **dünün** klasörünü izle, gece yarısı rotasyonunu zamanlayıcıyla yap
- macOS'ta FSEvents coalescing zaten var; üstüne bizim debounce
- İzlenen dosya sayısı üst sınırı: 200. Aşılırsa en yeni 200'ü izle, kalanı 5 dakikalık poll'e düşür
- Uygulama arka plandayken (popover kapalı) tick aralığı 5 sn → 60 sn'ye düşer

### 5.5 CI perf regresyon testi

`benches/` altında criterion benchmark. GitHub Actions'ta her PR'da çalışır:

```
fixture: 500 dosya, ~180 MB toplam JSONL
assert: cold_parse   < 3000 ms
assert: warm_tick    < 20 ms
assert: peak_rss     < 60 MB
assert: bytes_read_per_hour_simulated < 5 MB
```

Bu testler kırmızıysa merge yok. Perf bir özellik, sonradan düzeltilecek bir bug değil.

---

## 6. Pencere Motoru ve Tahmin

### 6.1 Kayan pencere yeniden inşası (L2 için)

Sabit kova değil, gerçek kayan pencere:

```
now = T
5s penceresi  = [T-5h, T] aralığındaki tüm token'lar
7g penceresi  = [T-7d, T] aralığındaki tüm token'lar
```

Olayları 5 dakikalık kovalarda tut (`Vec<Bucket>`, ring buffer). Pencere toplamı = ilgili kovaların toplamı. Her tick'te sadece kenar kovaları ekle/çıkar — O(1).

**Belirsizlik:** Plan limitleri (Pro/Max 5x/Max 20x için token tavanı) resmî olarak yayınlanmıyor. Kullanıcı ayarlardan planını seçer, biz topluluk kalibrasyonlu bir tavan kullanırız ve **her zaman "tahmini" etiketiyle** göstereriz. Yanlış kesinlik = güven kaybı.

### 6.2 Pace forecast

```
burn_rate   = son 60 dakikadaki tüketim / 60
naive_eta   = kalan_kota / burn_rate
```

Ama saf lineer tahmin geceleri saçmalar. İki katman:

1. **Kısa vade (5s penceresi):** EWMA, α=0.3, 15 dakikalık pencere → "Bu hızla 47 dakika sonra dolar"
2. **Uzun vade (haftalık):** Kullanıcının son 4 haftasının gün-içi profiline göre ağırlıklandır → "Hafta sonunda ~%118" + risk rozeti (sağlıklı / riskli / aşımda)

Risk eşikleri: `<%90 sağlıklı`, `%90-100 riskli`, `>%100 aşımda`.

### 6.3 Kaçak ajan tespiti (differentiator)

Takılmış bir agent kotayı sessizce yakar. Sinyal: aynı araç çağrısının tekrarı + token sayısının azalmaması + uzun süre. Basit heuristik:

```
son 10 dakikada:
  distinct_tool_calls / total_tool_calls < 0.25
  VE  token_delta_trend >= 0
  VE  süre > 8 dk
→ "Codex 12 dakikadır aynı döngüde. %8 kota harcandı." bildirimi
```

Bu, CUStats'ta olmayan gerçek bir değer. v1.1'e koy.

---

## 7. Tasarım Sistemi

> Brief: CUStats'tan daha iyi olacak. CUStats `#D97359` terracotta kullanıyor — bu Anthropic'in kendi arayüz vurgu rengiyle (`#D97757`) neredeyse aynı ve şu an AI ile üretilmiş arayüzlerin klişesi. **Bu palete girmiyoruz.** Krem+serif, siyah+asit yeşili ve broadsheet düzeni de aynı sebeple eleniyor.

### 7.1 Konsept: "The Horizon"

**Temel içgörü:** AI kotası bir pil değil, bir **gelgittir.** 5 saat önceki kullanımın sessizce üstünden düşer. İlerleme çubuğu bunu anlatamaz — sadece "ne kadar doldu"yu söyler, "ne zaman boşalacağını" söyleyemez.

**İmza öğe:** Her sağlayıcı için tek bir yatay şerit. Sağ kenar = **şimdi**. Sol kenar = **pencere sınırı**. Kullanım blokları sağdan girer, zamanla sola kayar ve sol kenardan düşer (kota geri gelir). Alt kenarda ince bir "geri dönen kapasite" hayalet katmanı — birazdan serbest kalacak kotayı önceden gösterir.

```
┌──────────────────────────────────────────────────────────┐
│  CODEX                          5s ▓▓▓▓▓▓▓░░░  68%       │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │ ░░░▒▒▒▓▓▓░░▒▒▒▒▓▓▓▓▓▓░░░░░░▒▒▓▓█████████│         │  │
│  │ ·····································░░░│         │  │  ← hayalet: geri dönecek
│  └────────────────────────────────────────────────────┘  │
│   -5s          -3s          -1s              şimdi ┘     │
│                                                          │
│  17:42'de %23 serbest kalıyor · haftalık %31             │
└──────────────────────────────────────────────────────────┘
```

Bu şerit hem tarihçe hem tahmin, hem de kayan pencere modelini kullanıcıya **öğretir.** Cesaretimizi buraya harcıyoruz; geri kalan her şey sessiz ve disiplinli.

### 7.2 Renk

Menü çubuğu paneli rastgele duvar kağıdının üstünde durur — kendi kapalı yüzeyini kurmalı.

```css
/* Dark (varsayılan) */
--surface-abyss:  #0A0E17;   /* panel zemini */
--surface-hull:   #131A26;   /* kart */
--surface-raised: #1A2331;   /* hover / aktif */
--rule:           #232E40;   /* hairline, 1px */

--ink-primary:    #E6EBF4;
--ink-secondary:  #8A99B0;
--ink-tertiary:   #55637A;

/* Seviye rampası — yalnız durum için, dekorasyon için asla */
--level-ample:    #45C4A0;   /* soğuk yeşil, <%60 */
--level-tight:    #E3A83C;   /* pirinç, %60-85 */
--level-critical: #FF5E5B;   /* mercan, >%85 */

/* Yardımcı */
--ghost:          #2B3A52;   /* geri dönen kapasite katmanı */
--measured:       #6BA8FF;   /* L1 rozeti — mavi = kesin */
```

```css
/* Light */
--surface-abyss:  #F2F4F8;
--surface-hull:   #FFFFFF;
--surface-raised: #E9EDF3;
--rule:           #DCE2EB;
--ink-primary:    #10151F;
--ink-secondary:  #55637A;
--ink-tertiary:   #8A99B0;
/* seviye renkleri aynı, kontrast için %8 koyulaştır */
```

**Kural:** Seviye rengi yalnızca gerçek doluluk göstergelerinde kullanılır. Buton, başlık, ikon, çerçeve — asla. Kırmızı gördüğünde kullanıcı tek şey anlamalı: kota bitiyor.

### 7.3 Tipografi

| Rol | Yüz | Neden |
|---|---|---|
| Rakam / veri | **Martian Mono** (SIL OFL) | Geniş, teknik, mükemmel tabular figür. Yüzde tıkırdarken **zıplamaz** — bu estetik değil, işlevsel bir gereklilik |
| Arayüz / gövde | **Inter Variable** | Nötr iş atı, küçük boyutta okunur |

Ölçek (menü paneli, 380px):

```
display   28px / 32  Martian Mono  500  -0.02em   (büyük yüzde)
metric    15px / 20  Martian Mono  400  -0.01em   (ikincil sayı)
label     11px / 14  Inter         600  +0.08em   UPPERCASE (sağlayıcı adı)
body      13px / 18  Inter         400   0        (açıklama)
caption   11px / 14  Inter         400  +0.02em   (zaman damgası)
```

Sağlayıcı adları küçük, harf aralıklı, büyük harf — bir aletin panelindeki etiket gibi. Marka logosu **kullanılmaz** (§9 marka riski).

### 7.4 Düzen — Tray paneli (380 × 520)

```
┌────────────────────────────────────────┐
│  ⌁ QUOTA DECK              ⚙  ⤢        │  48px başlık
├────────────────────────────────────────┤
│  CODEX                    ✦ ölçüldü    │
│  68%                          5 saatlik│
│  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░                │
│  [Horizon şeridi ————————————]         │  ← imza
│  17:42'de sıfırlanır · hafta %31       │
├────────────────────────────────────────┤
│  CLAUDE CODE              ≈ tahmini    │
│  ~41%                         5 saatlik│
│  ▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░               │
│  [Horizon şeridi ————————————]         │
│  ~19:10 · $4.82 bugün                  │
├────────────────────────────────────────┤
│  KIMI                     ✦ ölçüldü    │
│  ...                                   │
├────────────────────────────────────────┤
│  3 araç sessiz  ▾                      │  katlanmış: kullanılmayanlar
├────────────────────────────────────────┤
│  Tümü · Geçmiş · Ayarlar               │  40px alt bar
└────────────────────────────────────────┘
```

**Güven rozetleri** — küçük, sessiz, ama her zaman görünür:
- `✦ ölçüldü` — mavi nokta, L1
- `≈ tahmini` — içi boş halka, L2
- `◷ 2s önce` — bayat veri
- `○ sessiz` — araç kurulu ama kullanım yok

### 7.5 Menü çubuğu ikonu

Üç mod, ayarlardan seçilir:

1. **Glif** (varsayılan) — 16×16, en kritik sağlayıcının doluluğunu gösteren dikey bar dolgusu. Sayı yok, dikkat dağıtmaz.
2. **Kompakt** — `68%` tek sayı, en kritik sağlayıcı
3. **Şerit** — mini Horizon, 44px genişlik

Renk: `<%85` monokrom (`--ink-secondary`). `>%85` sadece o zaman `--level-critical`. Menü çubuğunda sürekli yanan renk = kullanıcının kapatma sebebi.

### 7.6 Boş ve hata durumları

Yönlendirici ol, özür dileme:

- Hiç araç bulunamadı → *"Bu Mac'te desteklenen bir AI aracı bulunamadı. Claude Code, Codex veya Kimi kurduktan sonra Quota Deck otomatik algılar."* + [Desteklenen araçlar]
- Klasör izni yok → *"Codex oturum kayıtlarına erişim gerekiyor. Erişim tek seferlik verilir ve veri cihazından çıkmaz."* + [Klasörü seç]
- `rate_limits` null → *"Codex bu oturumda limit bilgisi yazmadı. Token sayımından tahmin gösteriliyor."* Panik yok, sessiz düşüş.

---

## 8. Mağaza Dağıtımı

### 8.1 Mac App Store — sandbox gerçeği

**En önemli teknik kısıt bu. Baştan doğru kur, sonra düzeltmek pahalı.**

App Store'a giren her uygulama sandbox içinde çalışmak **zorunda.** Sandbox'ta:

- `~/.claude`, `~/.codex`, `~/.kimi` **doğrudan okunamaz.**
- `FileManager.urls(for:in:)` gerçek ana dizini değil, container'ı döndürür (`~/Library/Containers/<bundle-id>/Data/...`). Gerçek ana dizin için `getpwuid` gerekir.
- `com.apple.security.temporary-exception.files.home-relative-path.read-write` gibi geçici istisnalar App Review'da **reddedilme sebebidir.**

**Tek meşru yol:** Kullanıcı `NSOpenPanel` ile klasörü **kendisi seçer** (bu, macOS için açık yetkilendirme sayılır), biz **security-scoped bookmark** oluşturup saklarız.

```
Onboarding akışı (tek seferlik):
1. Açıklama ekranı: neden erişim gerekiyor, veri nereye gidiyor (hiçbir yere)
2. NSOpenPanel → kullanıcı ana dizinini seçer (tek seçim, tüm araçları kapsar)
3. url.bookmarkData(options: .withSecurityScope) → redb'ye kaydet
4. Her açılışta: URL(resolvingBookmarkData:) → startAccessingSecurityScopedResource()
5. Uygulama kapanırken: stopAccessingSecurityScopedResource()
```

**⚠️ Kritik uyarı:** `stop...` çağrılmazsa kernel kaynağı sızar. Yeterince sızarsa uygulama sandbox'a yeni konum ekleme yeteneğini **tamamen kaybeder** (yeniden başlatana kadar). Rust tarafında `Drop` impl ile garanti altına al.

**⚠️ İkinci uyarı:** `~/.claude` gizli klasör. Kullanıcıya "Finder'da `Cmd+Shift+.` ile gizli dosyaları göster" demek kötü UX. Bunun yerine **ana dizini** seçtir (tek panel, tek karar), alt yolları biz çözeriz.

**Entitlements.plist:**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.app-sandbox</key><true/>
  <key>com.apple.security.files.user-selected.read-only</key><true/>
  <key>com.apple.security.files.bookmarks.app-scope</key><true/>
  <key>com.apple.application-identifier</key>
  <string>$TEAM_ID.$IDENTIFIER</string>
  <key>com.apple.developer.team-identifier</key>
  <string>$TEAM_ID</string>
  <!-- AĞ ENTITLEMENT'I YOK. Bilerek. Review'da artı puan. -->
</dict>
</plist>
```

`com.apple.security.network.client` **eklenmiyor** — çünkü hiç ağ isteği yok. Bu, gizlilik iddiamızın kod imzasıyla kanıtlanması demek.

**Keychain'e neden dokunmuyoruz:** Başka bir uygulamanın keychain kaydını okumak sistem parolası soran bir dialog tetikler — aynı keychain access group tanımlansa bile. Sandbox'lı bir App Store uygulamasında bu hem UX felaketi hem review riski. Karar 1 zaten bunu yasaklıyor; bu ikinci sebep.

**Tauri build zinciri:**

```bash
# Ayrı config: sadece App Store build'i için
# src-tauri/tauri.appstore.conf.json
{
  "bundle": {
    "macOS": {
      "entitlements": "./Entitlements.plist",
      "files": { "embedded.provisionprofile": "./MacAppStore.provisionprofile" }
    }
  }
}

npm run tauri bundle -- --bundles app \
  --target universal-apple-darwin \
  --config src-tauri/tauri.appstore.conf.json

codesign --sign "Apple Distribution: <AD>" \
  --entitlements src-tauri/Entitlements.plist --deep --force \
  "target/universal-apple-darwin/release/bundle/macos/Quota Deck.app"

xcrun productbuild --sign "3rd Party Mac Developer Installer: <AD>" \
  --component "Quota Deck.app" /Applications "QuotaDeck.pkg"

xcrun altool --upload-app --type macos --file QuotaDeck.pkg \
  --apiKey "$APPLE_API_KEY" --apiIssuer "$APPLE_API_ISSUER"
```

Gerekenler: Apple Distribution sertifikası, 3rd Party Mac Developer Installer sertifikası, Mac App Store provisioning profile (`embedded.provisionprofile`), `.pkg` formatı (`.app` doğrudan yüklenmez).

**Bilinen Tauri tuzağı:** `Asset validation failed (90296) App sandbox not enabled` hatası — entitlements dosyası bundle'a doğru gömülmemiş demektir. `codesign -d --entitlements - <app>` ile doğrula.

### 8.2 Microsoft Store

Tauri'nin şu an ürettiği format EXE ve MSI. İki yol var:

**Yol A — EXE/MSI (Tauri'nin resmî yolu):**
- Partner Center'da `New Product > EXE or MSI app` ile kayıt
- Store yalnızca yüklenmemiş uygulamaya **link verir**; installer senin sunucunda barınır
- Installer **offline**, **kod imzalı** ve **sessiz kurulum** destekli olmalı
- NSIS `-setup.exe` sessiz kurulumu `/S` (büyük S) ile yapar → Partner Center'a bu parametreyi gir
- MSI kullanırsan `/quiet`
- **Zorunlu:** `webviewInstallMode: { "type": "offlineInstaller" }` — Store şartı
- Publisher adı ürün adıyla aynı olamaz

```json
// src-tauri/tauri.msstore.conf.json
{ "bundle": { "windows": { "webviewInstallMode": { "type": "offlineInstaller" } } } }
```

**Yol B — MSIX (daha temiz, önerilen):**
- `@choochmeque/tauri-windows-bundle` ile MSIX/MSIXBUNDLE üret
- Veya Microsoft'un resmî `winapp` CLI'ı (Tauri rehberi mevcut)
- x64 + arm64 için ayrı paket, sonra `.msixbundle`
- `runFullTrust` otomatik ekleniyor (Tauri için zorunlu) → **dosya erişimi kısıtlanmıyor**, macOS sandbox derdi Windows'ta yok
- **Store MSIX'i senin yerine imzalıyor** — kod imzalama sertifikası satın almana gerek yok
- Startup task, toast bildirim, tray gibi özellikler manifest'ten geliyor

**Karar: Yol B (MSIX).** Sertifika maliyeti yok, kurulum deneyimi daha iyi, güncellemeler Store üzerinden.

### 8.3 Marka ve isimlendirme — reddedilmemek için

Apple'ın kuralı net: başka bir şirketin ürün adını arama trafiği için başlığına koymak, App Review'ın açıkça uyardığı metadata tuzağı. Ürün adı **senin kendi adın** olmalı. Marka adları yalnızca **açıklama metninde**, referans amaçlı ("… ile uyumlu", "… için") kullanılabilir; başlıkta, ikonda veya alt başlıkta asla.

CUStats bunu zor yoldan öğrenmiş — v1.8.0 sürüm notu: *"Fix App Store metadata compliance — restricted third-party and Apple terms removed."*

| | ❌ Kullanma | ✅ Kullan |
|---|---|---|
| Uygulama adı | "Claude Usage Tracker" | **"Quota Deck"** |
| Alt başlık | "Claude & Codex limits" | "AI kodlama kotalarını izleyin" |
| İkon | Sağlayıcı logoları | Kendi Horizon glifin |
| Açıklama | "Track Claude Pro limits" | "Claude Code, Codex, Kimi ve 12 aracın yerel oturum kayıtlarıyla çalışır" |
| Ekran görüntüsü | Sağlayıcı logoları | Sadece metin etiketleri |
| Anahtar kelimeler | Marka doldurma | Kategorik terimler |

**Uygulama içinde de logo yok.** Sağlayıcıları harf aralıklı büyük harf metinle göster (§7.3). Bu hem yasal olarak temiz hem de tasarım dilimizle tutarlı.

**İsim adayları:** Quota Deck · Ceiling · Windowpane · Runway · Headroom · Tidemark
Bu blueprint boyunca **Quota Deck** kullanılıyor. Değiştirirsen tek yerden değiştir (`APP_NAME` sabiti).

### 8.4 Fiyatlandırma ve IAP

- Tek seferlik satın alma (CUStats $9.99). Abonelik yok — ürünün ruhuna aykırı.
- Sunucun yok, hesap yok → tüm dijital satış **Apple IAP / Microsoft IAP** üzerinden olmak zorunda. Harici satın alma linki = red.
- **Demo mode zorunlu.** CUStats'ta var ve doğru karar: kullanıcı satın almadan önce arayüzü görmeli. Sahte ama gerçekçi veriyle.
- Fiyat önerisi: $7.99–$12.99 bandı. Daha çok sağlayıcı + iki platform → CUStats'ın üstünde konumlanabilirsin.

---

## 9. Faz Planı

Her faz bağımsız çalışabilir bir çıktı üretir. Faz sonunda **sen** commit atarsın.

### Faz 0 — Keşif ve Doğrulama (kod yok, 1 gün)

Bu faz atlanamaz. Bütün planın varsayımlarını kendi makinende doğrular.

- [ ] `~/.claude/projects/` içinde JSONL var mı? Bir satır örneği çıkar, `usage` alanlarını doğrula
- [ ] `~/.codex/sessions/` içinde en yeni `rollout-*.jsonl` bul. `rate_limits` **dolu mu, null mu?** (Bu tek bulgu Codex'in L1 mi L2 mi olacağını belirler)
- [ ] Kurulu diğer araçları listele: `ls ~/.kimi ~/.gemini ~/.qwen ~/.local/share/opencode` vb.
- [ ] Her bulunan araç için: 1 örnek satır + dosya sayısı + toplam boyut → `docs/DISCOVERY.md`
- [ ] Claude Code `settings.json` içinde `statusLine` mekanizmasını test et — bize ne gönderiyor?
- [ ] `CLAUDE_CODE_ENABLE_TELEMETRY=1` OTLP çıkışını test et — hangi metrikler geliyor?
- [ ] `ccusage` kur, `npx ccusage@latest daily` çalıştır, çıktısını referans doğruluk temeli olarak kaydet

**Çıktı:** `docs/DISCOVERY.md` — gerçek makinedeki gerçek durum. Sonraki her faz bunu referans alır.

### Faz 1 — Rust Çekirdek İskeleti

- [ ] Cargo workspace: `core/`, `providers/`, `app/`
- [ ] `Provider` trait + `ProviderId` + `ProviderSnapshot` tipleri
- [ ] `FileCursor` + incremental reader + partial-line buffer
- [ ] Dosya rotasyonu tespiti (inode / file_index)
- [ ] `notify` watcher + 750 ms debounce
- [ ] `redb` store + batched writer (500 kayıt / 60 sn)
- [ ] Birim testler: fixture JSONL ile parse, dedup, rotasyon
- [ ] `benches/` criterion iskeleti

### Faz 2 — Codex Sağlayıcısı (ilk uçtan uca)

- [ ] `discover_roots()` — macOS/Windows yol çözümü
- [ ] `rollout-*.jsonl` parser, `token_count` + `rate_limits` çıkarımı
- [ ] `rate_limits: null` fallback zinciri (en yeni non-null kaydı bul → yaşı hesapla → bayatsa L2)
- [ ] `primary`/`secondary` → `QuotaWindow` eşlemesi
- [ ] `Confidence` seviyesi doğru atanıyor mu — test
- [ ] CLI doğrulama komutu: `cargo run -- debug codex` → terminalde tablo

### Faz 3 — Tray + Panel (ilk görsel çıktı)

- [ ] Tauri v2 kurulumu, capability whitelist (fs plugin frontend'e **verilmez**)
- [ ] Tray ikonu + üç mod (glif / kompakt / şerit)
- [ ] `tauri-plugin-positioner` ile popover konumlandırma
- [ ] macOS: `NSPanel` davranışı — focus çalmama, dışarı tıklayınca kapanma
- [ ] Tasarım token'ları CSS custom property olarak (`tokens.css`)
- [ ] Martian Mono + Inter gömülü (lisans dosyaları `licenses/` altına)
- [ ] Codex kartı render — henüz Horizon yok, basit bar
- [ ] Dark + light tema

### Faz 4 — İmza Öğe: Horizon Şeridi

- [ ] `Bucket` serisi Rust'tan gelir (5 dk çözünürlük, ring buffer)
- [ ] Canvas veya SVG render — 60 fps değil, **1 fps yeter** (bilinçli karar, batarya)
- [ ] Kayan pencere animasyonu: bloklar sola akar
- [ ] "Geri dönen kapasite" hayalet katmanı
- [ ] `prefers-reduced-motion` → animasyon kapalı, statik gösterim
- [ ] Hover: o kovadaki token/maliyet tooltip'i

### Faz 5 — Claude Code Sağlayıcısı

- [ ] `~/.claude/projects/**/*.jsonl` parser
- [ ] `(message.id, requestId)` dedup — **bu adım atlanırsa maliyet 2-3× şişer**
- [ ] Kayan pencere motoru (5s + 7g)
- [ ] Plan seçimi UI'ı (Pro / Max 5× / Max 20×) + tahmini tavan
- [ ] LiteLLM fiyat tablosu gömülü (JSON, build-time)
- [ ] "Tahmini" güven rozeti her yerde görünür
- [ ] Faz 0'da çalışan bulunduysa: statusline veya OTLP ile L1'e yükseltme

### Faz 6 — Sağlayıcı Genişletme

Her biri ayrı dosya, ayrı test, ayrı commit. Faz 0 (`docs/DISCOVERY.md` §1) bu makinede yalnızca
Copilot CLI ve Hermes'i buldu; geri kalanı kurulu değil. Tahmin edilen şemayla parser yazılmaz,
o yüzden bunlar gerçek fixture edinilene kadar açık kalıyor (§10).

- [ ] Kimi (`~/.kimi`, `~/.kimi-code`, `KIMI_DATA_DIR`; turn-scoped kayıt filtresi) — makinede yok
- [ ] Gemini CLI — makinede yok (`~/.gemini` var ama oturum logu tutmuyor)
- [x] GitHub Copilot CLI — kredi sayacı, takvim ayı penceresi, `quota_exceeded` ölçümü (§11)
- [ ] Qwen Code — makinede yok
- [ ] OpenCode — makinede yok
- [ ] Amp — makinede yok
- [ ] Droid — makinede yok
- [ ] Goose — makinede yok
- [ ] Codebuff, pi-agent, Kilo — makinede yok. Hermes kurulu ama `~/.hermes/logs` hiç token kaydı tutmuyor, ayrıştıracak bir şey yok.
- [x] "Sessiz araçlar" katlanır bölümü — `ui/src/components/QuietTools.tsx`

### Faz 7 — Tahmin, Geçmiş, Bildirimler

- [x] Pace forecast (EWMA kısa vade + profil ağırlıklı uzun vade) — `core/src/pace.rs`
- [x] Risk rozetleri (sağlıklı / riskli / aşımda) — `ui/src/components/PaceBadge.tsx`
- [x] Dashboard penceresi: gün / hafta / ay — `ui/src/Dashboard.tsx`, pencere `open_dashboard` ile açılır
- [x] Aktivite ısı haritası — `ui/src/components/Heatmap.tsx`, yerel takvim günü, nötr mürekkep rampası
- [x] Eşik bildirimleri (%70 / %85 / %95) — sağlayıcı başına ayarlanabilir — `app/src/alerts.rs`
- [x] Bildirim susturma (1 saat / bugün) — `Settings.muted_until`

### Faz 8 — Yerelleştirme ve Erişilebilirlik

- [x] i18n altyapısı, TR + EN
- [x] Tüm stringler dışarı alınmış, hardcode yok
- [x] Klavye navigasyonu, görünür focus halkası
- [x] VoiceOver / Narrator etiketleri
- [x] Renk körlüğü: seviye yalnızca renkle değil, **desen + metin** ile de ifade edilir
- [x] `prefers-reduced-motion` tüm animasyonlarda

### Faz 9 — macOS Sandbox ve App Store

- [ ] `Entitlements.plist` (§8.1)
- [ ] `NSOpenPanel` onboarding akışı
- [ ] Security-scoped bookmark kaydet / çöz / start / **stop** (Drop impl ile garanti)
- [ ] Sandbox içinde tam regresyon testi — `sandbox-exec` ile lokal doğrulama
- [ ] Demo mode
- [ ] Apple sertifikaları + provisioning profile
- [ ] `.pkg` üretimi ve Transporter yüklemesi
- [ ] App Store metadata — marka temizliği kontrolü (§8.3)
- [ ] Gizlilik beyanı: "Data Not Collected"

### Faz 10 — Windows ve Microsoft Store

- [ ] Windows yol çözümü (`%USERPROFILE%`, `%APPDATA%`)
- [ ] `Shell_NotifyIcon` tray + popover konumlandırma
- [ ] `webviewInstallMode: offlineInstaller`
- [ ] MSIX paketleme (x64 + arm64 → `.msixbundle`)
- [ ] Startup task manifest
- [ ] Partner Center kaydı ve gönderim

### Faz 11 — v1.1 (yayın sonrası)

- [ ] Kaçak ajan tespiti (§6.3)
- [ ] Antigravity sağlayıcısı — deneysel bayrak, varsayılan kapalı
- [ ] Çoklu hesap / çoklu config kökü
- [ ] Menü çubuğu widget'ları
- [ ] DE / ES yerelleştirme

---

## 10. CLAUDE.md İçeriği

Proje kökünde `CLAUDE.md` olarak oluştur:

```markdown
# Quota Deck — Claude Code Çalışma Kuralları

## Git — İSTİSNASIZ
- `git commit` ÇALIŞTIRMA. Commit'leri Kutluhan manuel atar.
- `git push` ÇALIŞTIRMA.
- Branch AÇMA (`git branch`, `git checkout -b` yok). Mevcut branch'te kal.
- Commit mesajlarına `Co-Authored-By: Claude` veya herhangi bir AI atıfı EKLEME.
  GitHub Contributors listesinde yalnızca Kutluhan görünmeli.
- Değişiklikleri yap, `git status` ve `git diff` ile özetle, dur.

## Mimari kırmızı çizgiler
- AĞ İSTEĞİ YOK. `reqwest`, `hyper`, `ureq` veya benzeri HTTP istemcisi
  bağımlılık olarak EKLENMEZ. Bu ToS ve App Store uyumluluğu için zorunlu.
- Keychain / Credential Manager OKUNMAZ.
- Sağlayıcı auth dosyaları (`auth.json`, `credentials.json`, `.credentials`)
  AÇILMAZ, listelenmez, varlığı bile kontrol edilmez.
- Yalnızca oturum/telemetri log dosyaları okunur. Salt okunur. Asla yazılmaz.
- Frontend'e `tauri-plugin-fs` verilmez. Dosya erişimi sadece Rust'ta.

## Performans kuralları
- JSONL dosyaları ASLA baştan okunmaz. Her zaman byte-offset cursor.
- `read_to_string` ile log dosyası okuma YOK.
- Her olayda disk yazma YOK. Batched: 500 kayıt veya 60 saniye.
- Watcher recursive DEĞİL. Sadece bilinen alt dizinler.
- Yeni bağımlılık eklemeden önce bellek/binary etkisini belirt.

## Kod
- Rust: `clippy -- -D warnings` temiz olmalı.
- `unwrap()` / `expect()` üretim yolunda yok. Parser'da panic = uygulama ölür.
- Ayrıştırılamayan satır `Ok(None)` döner, `Err` değil. Bozuk tek satır
  tüm dosyayı düşürmemeli.
- TS: `strict: true`, `any` yok.
- Yeni sağlayıcı = tek dosya + fixture testi + `providers/mod.rs` kaydı.

## Test
- Her sağlayıcı için `tests/fixtures/<provider>/` altında gerçek örnek JSONL.
- Fixture'lardaki tüm kişisel veri anonimleştirilmiş olmalı.
- Perf benchmark'ları kırmızıysa devam etme, önce düzelt.
```

---

## 11. Riskler ve Açık Kararlar

### Yüksek risk

| Risk | Etki | Azaltma |
|---|---|---|
| **Log formatı değişikliği** | Sağlayıcı bir sürümde şemayı değiştirir, veri durur | Parser'lar defensive: bilinmeyen alan → yoksay, eksik alan → `None`. Her sağlayıcı için "son başarılı ayrıştırma" zaman damgası tut, 7 günden eskiyse UI'da uyar. Otomatik güncelleme kanalı hazır olsun. |
| **Codex `rate_limits: null`** | Ana L1 kaynağı çalışmaz | Faz 0'da doğrula. Null ise Codex de L2'ye düşer ve ürünün L1 iddiası zayıflar. **Bu durumda ürün konumlandırmasını "maliyet ve kullanım takibi" olarak revize et.** |
| **Sandbox reddi** | App Store'a giremezsin | Faz 9'u erken prototiple. Sandbox'ı Faz 3'te bile test et, sona bırakma. |
| **Anthropic/OpenAI politika değişikliği** | Yerel log okuma da yasaklanabilir | Düşük olasılık (kullanıcının kendi diski) ama izle. ToS değişikliklerini takip et. |

### Orta risk

| Risk | Azaltma |
|---|---|
| Tauri bundle boyutu (~15 MB vs CUStats 3,1 MB) | Kabul et. Perf bütçesiyle telafi et, store açıklamasında ölçülen RAM/CPU rakamlarını yayınla. |
| Marka reddi (5.2.1 / 4.1) | §8.3'ü harfiyen uygula. Gönderimden önce tüm metadata'yı marka taraması yap. |
| Kullanıcı sandbox iznini vermezse | Onboarding'de değeri net anlat. İzin verilmezse demo mode'da kal, kilitleme. |
| Plan limitleri bilinmiyor | Her zaman "tahmini" etiketi. Kullanıcı kendi tavanını manuel girebilsin. |

### Karar bekleyen sorular

1. **Uygulama adı** — Quota Deck mi, başka bir aday mı? Store'da isim rezervasyonu erken yapılmalı.
2. **Fiyat** — CUStats $9.99. Altında mı, üstünde mi, eşit mi?
3. **iOS companion?** CUStats'ın "CUStats Go" diye ayrı bir iOS uygulaması var. Ama bizim mimarimiz %100 yerel — iOS'ta okunacak log yok. **Öneri: yapma.** Senkron eklemek Karar 1'i bozar.
4. **Açık kaynak mı?** Parser katmanını MIT açık kaynak yapıp uygulamayı ücretli tutmak güven inşa eder ve topluluk sağlayıcı katkısı getirir. Ama kopyalanma riski. Karar senin.
5. **Faz 0 sonucu Codex null çıkarsa** ürün konumlandırması değişir — o noktada dur ve yeniden değerlendir.

---

## 12. Kaynak Referansları

Faz 0 ve her yeni sağlayıcı eklerken başvurulacak kanonik kaynaklar:

- **ccusage Data Sources** — `ccusage.com/guide/` · her sağlayıcının güncel yolu, şeması ve token eşlemesi. Bu blueprint'teki sağlayıcı listesinin kaynağı.
- **Tauri App Store** — `v2.tauri.app/distribute/app-store/`
- **Tauri Microsoft Store** — `v2.tauri.app/distribute/microsoft-store/`
- **Tauri Windows Installer** — `v2.tauri.app/distribute/windows-installer/`
- **winapp CLI + Tauri (MSIX)** — `learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/guides/tauri`
- **App Store Review Guidelines** — `developer.apple.com/app-store/review/guidelines/` (özellikle 4.1, 5.2.1, 5.2.5)
- **Apple Security-Scoped Bookmarks** — App Sandbox dokümantasyonu
- **Claude Code legal & compliance** — `code.claude.com/docs/en/legal-and-compliance`
- **Codex `rate_limits` sorunları** — `github.com/openai/codex/issues/14728`, `/14880`

---

## Ek A — Kabul Kriterleri (v1.0 yayın öncesi)

- [ ] Faz 0'da tespit edilen tüm kurulu araçlar doğru algılanıyor
- [ ] En az 1 sağlayıcıda gerçek L1 verisi gösteriliyor
- [ ] Perf bütçesinin tamamı CI'da yeşil
- [ ] Sandbox içinde 72 saat kesintisiz çalışma testi geçti
- [ ] Bellek 72 saat sonunda başlangıç değerinin %20'sinden fazla artmamış (sızıntı yok)
- [ ] Kod tabanında hiçbir HTTP istemcisi bağımlılığı yok (`cargo tree | grep -E 'reqwest|hyper|ureq'` boş)
- [ ] Hiçbir auth/credential dosyası referansı yok (`rg -i 'auth\.json|credentials|keychain'` boş)
- [ ] TR ve EN yerelleştirmesi tam
- [ ] Demo mode satın alma olmadan tüm arayüzü gösteriyor
- [ ] Marka taraması: uygulama adı, alt başlık, ikon ve ekran görüntülerinde üçüncü parti marka yok
- [ ] Light ve dark tema, `prefers-reduced-motion` ve VoiceOver test edildi
