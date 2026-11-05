# Quota Deck v2 — Uygulama planı

Tarih: 2026-11-04 · Kaynaklar: `docs/research/SYSTEM_MONITOR_RESEARCH.md`, `docs/design/liquid-instruments/` (Claude Design çıktısı), `docs/design/claude-design-prompt-v2.md`

Her faz tek başına bitirilir, doğrulanır ve Kutluhan'a raporlanır. **Commit/push yalnız Kutluhan isteyince.**

---

> **2026-11-05 güncellemesi (Kutluhan):** App Store'dan vazgeçildi. Dağıtım yalnız GitHub'dan, açık kaynak (GPL-3.0), Developer ID ile imzalı ve Apple onaylı DMG (`scripts/release-macos.sh`). Mağaza dosyaları `stores/` altında park edildi. Bu yüzden K4 artık "tek sürüm" demek; F5'teki cam sapması kaldırıldı (gerçek Liquid Glass açık), F8 geliştirme kurulum betiğiyle bu Mac'te denenebilir.

## Kararlar (Kutluhan, 2026-11-04)

| # | Konu | Karar |
|---|---|---|
| K1 | Docker | Docker çalışırken okunur. Kapalıysa "Docker kapalı" durumu ve küçük bir bekleme animasyonu gösterilir. Yöntem: `docker` CLI alt süreci (Claude'un önerisi, Kutluhan yöntemi bıraktı). |
| K2 | Antigravity kotası | **İkisi de, ayardan seçilir.** Varsayılan yerel log; ayarda "agy ile canlı kota" açılabilir. |
| K3 | Fan | **Auto / Cool / Max kontrolü** (Mole gibi). Yardımcı süreç kurulumda değil, kullanıcı ilk kez Cool/Max'e bastığında kurulur. macOS yönetici onayı bir kez çıkar. Reddedilirse kart "Unchanged, needs approval" der. |
| K4 | Dağıtım | Sistem izleme **yalnız doğrudan (notarized DMG) sürümde.** App Store sürümü bugünkü gibi yalnız AI kotası. |
| K5 | Diğer 6 sağlayıcı | **Gizle.** Kod kalır, varsayılan kapalı; Ayarlar'dan açılır. |
| K6 | Tıklama | Menü çubuğu → **cam popover**; "Open Dashboard" → büyük pencere. |
| K7 | Logolar | **Gerçek logolar** (Claude, OpenAI/Codex, Antigravity). Ticari yayından önce yeniden değerlendirilecek. |
| K8 | Tema | Ayarlarda **tasarım teması** seçimi: şimdilik "Liquid" (Apple). Mevcut Açık/Koyu/Sistem seçimi ayrı kalır. İleride başka temalar eklenecek. |
| K9 | Görseller | İsteğe bağlı: Apple tarzı, şeffaf arka planlı görseller için prompt dosyası (`docs/design/GORSEL.md`). Kutluhan Antigravity/Gemini ile üretir. |

---

## Faz 0 — Kurallar, iskelet, ölçüm

**Amaç:** Yeni yeteneklerin kurallara girmesi, App Store sürümünün etkilenmemesi, sondanın Rust'a taşınması.

1. **`CLAUDE.md` kırmızı çizgilerini güncelle** (Kutluhan onayıyla). Doğrudan sürüm için açık istisnalar:
   - **Docker:** `docker` CLI alt süreci, yalnız okuma komutları (`ps`, `stats --no-stream`, `system df`, `info`).
   - **agy:** alt süreci, yalnız ayar açıkken. Ağ isteğini `agy` yapar, Quota Deck yapmaz; bu ayar açıkça "ağ kullanır" diye etiketlenir.
   - **Fan yardımcısı:** SMC'ye yalnız fan anahtarlarını (`F*md`, `F*Tg`, `Ftst`) yazar, başka hiçbir şey yazmaz.
   - **Özel API'ler** (IOReport, IOHIDEventSystemClient, AppleSMC) yalnız doğrudan sürüm binary'sinde bulunur.
2. **Cargo özelliği `system-monitor`** (varsayılan açık). `scripts/appstore.sh` bunu kapatarak derler. Böylece App Store binary'sinde özel API sembolü hiç olmaz; App Review'un özel API taraması temiz kalır. `scripts/check-appstore-config.mjs` bunu da doğrular.
3. **Yeni crate `sysmon/`** (workspace üyesi, yalnız macOS hedefinde derlenir, diğer platformlarda boş).
4. **Sondayı Rust'a taşı:** `sysmon/src/smc.rs` (okuma), `sysmon/src/hid.rs`. Kaynaklar: `scratchpad/probe/sensors.c` ve macmon'un MIT kodu; atıf `licenses/` altına.
5. **Bağımlılık kararı** (CLAUDE.md: önce etkiyi yaz):
   - `core-foundation` + `libc`: ağaçta zaten var, ek boyut ≈ 0.
   - `sysinfo` **eklenmez**. CPU/RAM/ağ/süreç doğrudan `libc`/Mach ile okunur (~400 satır, binary'ye etkisi ≈ 0).
6. **Ölçüm:** bir örnekleme turunun CPU süresi ve RSS'i (1 sn aralıkla, 60 sn). Hedef: tur başına 5 ms'nin altında CPU, en fazla +8 MB RSS. Kırmızıysa dur, düzelt.

**Bitti sayılır:** `cargo clippy --all-targets -- -D warnings` temiz; `sysmon` testleri geçiyor; App Store derlemesinde `nm` çıktısında `IOReport` / `IOHIDEventSystemClient` yok.

> **Durum (2026-11-04): bitti.**
> - Kırmızı çizgi istisnaları `CLAUDE.md` + `AGENTS.md`'de.
> - `system-monitor` özelliği eklendi; `appstore.sh` `--no-default-features` ile derliyor ve özel API sembol taraması yapıyor; `check-appstore-config` + 2 yeni negatif test.
> - `sysmon` crate'i: `smc.rs` (okuma, `Probe` önbelleği), `hid.rs`, `examples/probe.rs`.
> - Ölçüm (Mac mini M6, release), tur başına:
>   - SMC (150 anahtar): **~1 ms CPU, 22 ms**;
>   - HID (43 servis): 2,6 ms CPU, 44 ms; yalnız yedek olarak kullanılacak;
>   - tepe RSS: 6,9–8,2 MB.
> - Doğrulama: `cargo clippy --workspace` (varsayılan ve `--no-default-features`) temiz; `cargo test --workspace` 525/525.

---

## Faz 1 — Temel metrikler

`sysmon/src/`:

- `cpu.rs`: toplam, çekirdek başı, E/P küme yüzdeleri (`host_processor_info` farkı); `getloadavg`; uptime.
- `mem.rs`: kullanılan, sıkıştırılmış, swap; basınç (`kern.memorystatus_vm_pressure_level` → Normal/Warning/Critical).
- `disk.rs`:
  - iç ve harici birimler (`statfs`, `getfsstat`);
  - okuma/yazma B/s (`IOBlockStorageDriver` → `Statistics`);
  - SMART durumu.
- `net.rs`: `getifaddrs` sayaçlarından iniş/çıkış B/s. Ping yok.
- `proc.rs`: `proc_listpids` + `proc_pidinfo` + `proc_pid_rusage` → ad, bellek, %CPU, enerji (W), PID. İlk 50 süreç tutulur.
- `health.rs`: kendi formülümüz (ağırlıklar CPU 30, bellek 25, disk 20, termal 15, IO 10; SMART failing ise tavan 44; eşikler 85/65/45). Testlerle sabitlenir.

**Uygulama tarafı:**

- `app/src/system.rs` örnekleyiciyi yönetir.
  - Dashboard açıkken 1 sn, yalnız popover açıkken 2 sn, ikisi kapalıyken 10 sn (yalnız menü çubuğu metni).
  - Olay adı: `sys://state`. Komut: `system_state`.
- Bir toplayıcı hata verirse diğerleri gelir; o alan `Unavailable` olur.

**Bitti sayılır:** her modül için birim testi (sabit girdi → beklenen çıktı); gerçek Mac'te değerler Activity Monitor ile tutarlı.

> **Durum (2026-11-04): bitti.**
> - `sysmon/src/`: `cpu.rs`, `mem.rs`, `disk.rs`, `net.rs`, `proc.rs`, `health.rs` + ortak `iokit.rs`; hepsini `Sampler` tek turda okur (`Snapshot`).
> - Çekirdek kümeleri IODeviceTree'deki `cluster-type`'tan okunur, adı `hw.perflevelN.name`'den gelir. M6'da: Efficiency 6, Super 2, Performance 4.
> - Süreç enerjisi `rusage_info_v6.ri_energy_nj`'den (libc yalnız v4'ü tanıdığı için yapı elle tanımlı).
> - Ağ 64 bit sayaçlardan (`NET_RT_IFLIST2`); loopback ve VPN tünelleri sayılmaz.
> - `app/src/system.rs`: `sys://state` olayı + `system_state` komutu; tempo 1 sn / 2 sn / 10 sn. Gizliyken süreç listesi okunmaz. App Store derlemesinde iş parçacığı hiç başlamaz.
> - **SMART F2'ye kaydı:** NVMe SMART, sıcaklık/aşınma ile aynı eklenti arayüzünden okunuyor. Sağlık skoru şimdilik `smart_failing = false` ile hesaplanıyor.
> - Ölçüm (release): tam tur (süreçler dahil) **~1,1 ms CPU**, tepe RSS 8,9 MB.
> - Bellek, `top` ile tutarlı: wired 3,9 GB, compressed 9,8 GB.
> - Doğrulama: clippy iki modda temiz, `cargo fmt --check` temiz, `cargo test --workspace` 545/545.

---

## Faz 2 — Sensörler

- `thermal.rs`: SMC'yi prefix ailesine göre sınıflandır.
  - `Tp*`/`Te*` → CPU (P/E ayrı), `Tg*` → GPU, `Ts*` → SoC, `TH0*` → SSD/NAND, `Tm*` → RAM (varsa).
  - Ortalama/maks hesaplanır; 1 °C'nin altı ve 150 °C'nin üstü atılır.
  - SMC açılmazsa HID'e düşülür (`PMU tdie*`, `NAND CH0 temp`).
- 5 dakikalık tepe: en yüksek sensörün halka tamponu.
- `thermalState`: `NSProcessInfo.thermalState` (objc2, ağaçta zaten var).
- `fan.rs` (okuma): `FNum`, `F{n}Ac/Mn/Mx/md/Tg` → RPM, % (Ac/Mx), mod.
- `power.rs`:
  - sistem toplamı `PSTR`;
  - IOReport `Energy Model` ile CPU/GPU/ANE/DRAM W (macmon yöntemi, 500 ms örnek).
- `gpu.rs`: `IOAccelerator` → `PerformanceStatistics["Device Utilization %"]`.
- RAM sıcaklığı: `Tm*` yoksa `Unavailable("no sensor on this Mac")`. Uydurma değer yok.
- `smart.rs` (F1'den taşındı): `IONVMeSMARTUserClient` → kritik uyarı, aşınma %, NAND sıcaklığı; sağlık skorundaki `smart_failing` buradan beslenir.

**Bitti sayılır:**

- Anahtar tablosu fixture'ı ile testler: bu Mac'in (Mac mini M6) gerçek anahtar kümesi.
- Bu Mac'te değerler sondayla aynı.

> **Durum (2026-11-04): bitti.**
> - `sysmon/src/`: `thermal.rs`, `fan.rs`, `power.rs`, `gpu.rs`, `smart.rs`; hepsi `Snapshot`'a girdi (`thermal`, `fans`, `power`, `gpuUsage`, `smart`).
> - Sıcaklık: prefix aileleri (`Tp`/`Te`/`Tg`/`Ts`/`TH0`/`Tm`), 1–150 °C dışı atılır, SMC açılmazsa HID. 5 dk tepe halka tamponla. `thermalState` `NSProcessInfo`'dan.
> - Fixture: `sysmon/tests/fixtures/mac-mini-m6/smc-temperature-keys.txt` (148 anahtar adı). Test aile sayılarını doğrular: Tp 21, Te 9, Tg 18, Ts 9, TH0 3, Tm 0.
> - SMART: `NVMeSMARTLib` eklentisi, dakikada bir okunur; kritik uyarı, aşınma %, sıcaklık. Sağlık skoru artık `cpu_celsius` ve `smart_failing` ile hesaplanıyor.
> - **Güç, M6'da kısmi:** `PSTR` (sistem toplamı) ve IOReport `GPU Energy` var. IOReport "Energy Model" grubunda M6'da CPU/ANE/DRAM kanalı yok. `PMP › Energy` altındaki EACC0/PACC0 basit sayaç değil, anlamsız değer dönüyor. Bu alanlar `None` kalır, arayüz "bu Mac'te yok" der.
> - F1 hatası düzeltildi: `Cf::own` boş işaretçide nesneyi kurup bırakıyordu (`then_some` hevesli), `CFRelease(NULL)` ile süreç çöküyordu. Regresyon testi eklendi.
> - Ölçüm (release, tek `Sampler`): tur başına **~2 ms CPU**, ~9 ms duvar; kurulum bir kez ~30 ms; tepe RSS 10,7 MB.
> - App Store sembol taraması artık anlamlı: özellik açıkken 11 özel sembol, kapalıyken 0.
> - Doğrulama: clippy iki modda temiz, `cargo fmt --check` temiz, `cargo test --workspace` 557/557, Node 8/8.

---

## Faz 3 — Docker (CLI)

- `sysmon/src/docker.rs`:
  - Kurulu mu: `docker` PATH'te mi ve `/Applications/Docker.app` var mı.
  - Çalışıyor mu: `docker info --format '{{json .}}'`, 3 sn zaman aşımı.
  - Veriler:
    - `docker ps -a --format '{{json .}}'`: ad, imaj, durum, compose projesi (label), sağlık, yeniden başlatma sayısı;
    - `docker stats --no-stream --format '{{json .}}'`: CPU %, bellek;
    - `docker system df --format '{{json .}}'`: imaj, volume, build cache.
- Durumlar: `NotInstalled`, `NotRunning`, `Running(Summary)`.
- Örnekleme yalnız Docker sekmesi açıkken veya popover açıkken (CLI pahalı). En sık 5 sn.
- Bozuk bir JSON satırı tüm listeyi düşürmez (`Ok(None)` kuralı).

**Bitti sayılır:** fixture'lı testler (gerçek `docker` çıktısı, anonimleştirilmiş); Docker açık, kapalı ve kurulu değil durumlarında elle test.


> **Durum (2026-11-05): bitti.**
> - `sysmon/src/docker.rs` + ortak `process.rs` (zaman aşımı + çıktı sınırı). CLI, Finder'dan açılan uygulamanın çıplak `PATH`'i yüzünden bilinen yollarda aranır.
> - `ps` JSON'unda `HealthStatus` alanı var; yeniden başlatma sayısı yalnız `inspect` ile geliyor, izinli alt komutlarda olmadığı için **eklenmedi**.
> - Hiçbir konteyner çalışmıyorsa `stats` atlanır. Tur ~1 sn (çoğu `docker info`), ayrı iş parçacığında.
> - Tempo: ekranda bir şey açıkken 5 sn; kapalıyken yalnız "sağlıksız konteyner" uyarısı için 60 sn.
> - Fixture: `sysmon/tests/fixtures/docker/` (sunucudaki gerçek çıktının biçimi, anonim). Bozuk satır yalnız kendini düşürür.
> - Elle: bu Mac'te Docker 29.8.2 açık, konteyner yok → doğru okundu. Kapalı / kurulu değil durumları elle denenmedi (Docker'ı kapatmadım).

---

## Faz 4 — Antigravity ve sağlayıcı görünürlüğü

- `providers/src/antigravity.rs` (CLAUDE.md: tek dosya, fixture testi, `providers::all()` kaydı):
  - **Log (varsayılan, ölçülmüş):** `~/.gemini/antigravity/log/cli-*.log` ters kuyruk okuması (tail). `QUOTA_EXHAUSTED` + `model` + `quotaResetTimeStamp` → model başına "dolu, X'te açılır".
  - **Etkinlik:**
    - `conversation_summaries.db` salt okunur: konuşma sayısı ve son etkinlik;
    - `antigravity-cli/history.jsonl`: günlük istem sayısı.
  - **Canlı (ayar, varsayılan kapalı):** `agy -p /usage --output-format json`.
    - Ortam: `AGY_CLI_DISABLE_AUTO_UPDATE=true`, özel geçici dizin.
    - Tarayıcı açmasını engelleyen `BROWSER` koruması (CodexBar yöntemi).
    - 90 sn sınır, 1 MiB çıktı sınırı, en sık 5 dakikada bir.
  - Kimlik ve token dosyaları (`google_accounts.json` vb.) **asla** açılmaz, listelenmez.
- Fixture: `tests/fixtures/antigravity/` (anonimleştirilmiş log parçası + küçük SQLite).
- **Varsayılan görünürlük:** Amp, Copilot, Droid, Gemini, OpenCode ve PiAgent ilk açılışta `disabled_providers` içinde. Mevcut kullanıcı ayarları ezilmez (yalnız ayar dosyası yoksa).


> **Durum (2026-11-05): bitti.**
> - `providers/src/antigravity.rs`: `history.jsonl` satır satır (istem başına bir istek; istem metni hiç okunmaz). Bu Mac'te 37 istem okundu.
> - "Model dolu": son 3 `cli-*.log`'un son 256 KB'ı tersten okunur, `quotaResetTimeStamp` üstündeki `model` ile eşlenir (çok satırlı JSON bloğu satır ayrıştırıcıya uymadığı için).
> - Canlı kota: `agy -p /usage --output-format json` **token harcamıyor** (çıktıda 0 token, bir kez denendi). Haftalık iki grup verir (Gemini; Claude+GPT). Yalnız doğrudan sürümde, ayar açıkken, en sık 5 dakikada bir.
> - `conversation_summaries.db` okunmadı: istem geçmişi aynı bilgiyi veriyor, ek SQLite okuyucusuna gerek kalmadı.
> - Yeni kurulumda 6 araç gizli (`HIDDEN_BY_DEFAULT`); var olan ayar dosyası olduğu gibi kalır.

---

## Faz 5 — Tasarım sistemi ve tema altyapısı

- `docs/design/liquid-instruments/qd.css` → `ui/src/styles/themes/liquid.css`. Değişkenler `.qd.dark/.qd.light` yerine `[data-theme="liquid"][data-appearance="dark|light"]` altına taşınır.
- **Tema ayarı:** `Settings.design_theme: "liquid"` (şimdilik tek değer). Ayarlar'da "Tasarım" grubu. Yeni tema = yeni CSS dosyası + listeye bir satır.
- **Logolar:** `ui/src/assets/logos/{claude,codex,antigravity}.svg` (resmî basın kitlerinden). Tasarımdaki `mark` bileşeni logo SVG'si alır.
- **Cam efekti:**
  - Doğrudan sürümde `macOSPrivateApi: true`, `transparent: true`, pencere vibrancy (`NSVisualEffectView`, Tauri `windowEffects`).
  - App Store sürümünde opak yedek görünüm (`.rt` kuralları).
- Yazı tipi: sistem yığını (SF). Inter yalnız yedek; zaten `@fontsource-variable/inter` var.
- Erişilebilirlik: Reduce Motion ve Reduce Transparency medya sorguları.


> **Durum (2026-11-05): bitti.** (Aşağıdaki sapma GitHub kararıyla kalktı: popover penceresi saydam, `liquidGlassRegular` + `popover` malzemesi, `macOSPrivateApi` açık; Tauri CLI 2.12.4'e yükseltildi.)
> - `ui/src/styles/themes/liquid.css` (tasarımdaki `qd.css`, `[data-design="liquid"]` altında). Eski stiller `.legacy` kapsamına alındı.
> - Ayarlar: `designTheme` (şimdilik yalnız `liquid`).
> - Logolar `ui/src/assets/logos/` (kaynaklar README'de), maske olarak çizilir.
> - **Sapma:** gerçek pencere saydamlığı (`macOSPrivateApi` + vibrancy) yapılmadı. F0'daki App Store koruması temel yapılandırmada bunu yasaklıyor; Tauri bu ayarı Cargo özelliğiyle birebir eşleşmeye zorladığından App Store kontrol derlemesini de bozuyordu. Cam efekti pencere içindeki renkli arka planın üzerinde; Reduce Transparency'de opak.

---

## Faz 6 — Menü çubuğu ve popover

- Menü çubuğu başlığı: `48° · 76% ·` + en dar kotanın logosu + `67%`. Antigravity en darsa `4:52`. Durumlar: normal / amber (kota < 15) / kırmızı (kota < 5 veya termal Serious) / kompakt (yalnız ikon). Tray `set_title` + ikon.
- Popover (`Main.dc.html` → React):
  - başlık + sağlık hapı;
  - AI kotası 2×2 modül;
  - CPU, Bellek, Disk, Docker 2×1 modüller;
  - Fan, Güç, Ağ, Awake yuvarlak modüller;
  - termal şerit;
  - "Open Dashboard".
- **Keep Awake:** `IOPMAssertionCreateWithName`, çıkışta bırakılır.
- Panel boyutu 360×640 (`PANEL_WIDTH`, `MAX_PANEL_HEIGHT` güncellenir).


> **Durum (2026-11-05): bitti.**
> - Menü çubuğu: `48° · 76%` önek olarak mevcut kota simgesi/yüzdesinin önüne eklenir (`tray::set_system_text`). Amber/kırmızı nokta ve kompakt mod tasarımı yapılmadı; mevcut tray modları kaldı.
> - Popover: `ui/src/liquid/Popover.tsx` — tasarımdaki tüm modüller; App Store sürümünde yalnız AI modülü.
> - Keep Awake: `app/src/awake.rs`, `PreventUserIdleSystemSleep` (genel API).
> - Panel 360 px, en fazla 640 px.

---

## Faz 7 — Dashboard penceresi

Boyut 1180×760, yeniden boyutlanabilir (`DASHBOARD_WIDTH/HEIGHT`). Üstte yüzen cam araç çubuğu: Overview · Mac · Docker · AI, zaman aralığı (1 sa · 24 sa · 7 g), arama.

- **Overview:** sağlık + 3 AI kapsülü, kompakt SoC Heat Field, küçük kartlar, Docker özeti, ilk 5 süreç.
- **Mac:**
  - CPU, GPU, Bellek, RAM sıcaklığı (iki durum);
  - **SoC Heat Field** (sayfanın kahramanı) + termal durum + 24 sa grafik;
  - Depolama, Fan (kontrollü), Güç, Ağ, süreç tablosu (sıralanabilir, aranabilir).
- **Docker:** durum başlığı; **kapalı/kurulu değil durumunda boş ekran + animasyon** (K1); 4 istatistik kutusu; Compose gruplu konteyner listesi.
- **AI:** Claude / Codex / Antigravity segmentleri. Claude/Codex: oturum ve haftalık kapsüller, istatistikler, 30 gün grafik, son pencereler, Estimated/Measured rozetleri. Antigravity: model durum listesi, etkinlik grafiği, canlı kota ayarı.
- **Ayarlar sayfası (sheet):**
  - uyarı kuralları;
  - yenileme hızı (Live 1 sn / Balanced 2 sn / Saver 10 sn);
  - menü çubuğu içeriği;
  - araç görünürlüğü;
  - tasarım teması;
  - Antigravity canlı kota anahtarı.
- Grafikler: SVG, kütüphane yok (tasarımdaki `.chart` stilleri). Gerekirse sonra değerlendirilir.


> **Durum (2026-11-05): bitti.**
> - `ui/src/liquid/Board.tsx` + `OverviewTab`, `MacTab`, `DockerTab`, `AiTab`. 1180×760.
> - AI sekmesinin altında eski geçmiş/dışa aktarma ekranı (`UsageHistory.tsx`, eski `Dashboard.tsx`) duruyor; ısı haritası, kırılımlar, oturumlar ve JSON/CSV dışa aktarma kaybolmadı.
> - Yenileme hızı ayarı (Live 1 sn / Balanced 2 sn / Saver 10 sn), menü çubuğu içerik seçimi, Antigravity 30 günlük istem grafiği ve tasarım teması seçici (`Liquid` / `Claude`) tamamlandı.
> - Kullanılmayan eski bileşenler silindi (Overview, ProviderDetail, ProviderTabs, QuietTools, PaceBadge, HorizonStrip, horizon.ts).

---

## Faz 8 — Fan kontrolü

- **Yardımcı süreç:** küçük bir root LaunchDaemon (`com.kutluhangil.quotadeck.fanhelper`).
  - `SMAppService.daemon(plistName:)` ile **ilk Cool/Max tıklamasında** kaydedilir; macOS onay penceresi bir kez çıkar.
  - Uygulama ile XPC (mach service) üzerinden konuşur; **eş imza kontrolü** yapılır (yalnız bizim imzalı uygulamamız bağlanabilir).
  - Yalnız 3 komut kabul eder: `set_mode(fan, auto|cool|max)`, `restore_auto()`, `status()`. Rastgele SMC yazımı yok.
- **Modlar:** Auto = `F{n}md=0`. Cool ≈ maksimumun %60'ı. Max = `F{n}Mx`. M6'da `Ftst` kilidi yok; mod doğrudan yazılır. **Bu Mac'te dikkatle denenir.**
- **Güvenlik:**
  - Uygulama çıkınca, uykuya geçince/uyanınca ya da XPC bağlantısı kopunca yardımcı otomatik Auto'ya döner.
  - Kaldırma yolu: Ayarlar → "Fan yardımcısını kaldır" (`unregister`).
- **Reddedilirse:** kart amber "Unchanged, needs approval"; tekrar basınca onay yeniden istenir.
- Dil: yardımcı Swift ile yazılır (~250 satır; XPC Rust'ta zahmetli). Uygulama tarafı `objc2` ile `NSXPCConnection`.


> **Durum (2026-11-05): bitti ve arayüzde aktif.**
> - `app/fanhelper/main.swift` (daemon + client + register + selftest), launchd plist, `scripts/build-fanhelper.sh`, `app/tauri.fan.conf.json`.
> - XPC eş imza şartı takım kimliğine sabitlenir (`TEAM_ID`); şart kurulamazsa yardımcı başlamaz. Kira: uygulama 30 sn'de bir yeniler, 90 sn'de gelmezse Auto. Uyku ve SIGTERM'de Auto. Uygulama çıkışında Auto.
> - `selftest` SMC yapı düzeninin 80 bayt olduğunu ve alan konumlarını doğrular. Swift'in C'den farklı dolgu yapması ilk derlemede yakalandı ve düzeltildi. Gerçek SMC'den salt okuma: 1 fan, 1000–4900 rpm.
> - Uygulama: `app/src/fan_control.rs` (`set_fan_mode`, `remove_fan_helper`). Arayüzde `FAN_CONTROL_READY = true` ve Ayarlar'da "Fan yardımcısını kaldır" düğmesi eklendi.
> - Kalan: imzalı derlemeyle bu Mac'te deneme (`docs/USER_ACTIONS.md` §7).

---

## Faz 9 — Geçmiş ve uyarılar

- 1 dakikalık özetler mevcut `redb` deposuna (`system_history` tablosu); 7 gün saklama. Toplu yazım kuralına uyulur (60 sn).
- Bildirimler (`tauri-plugin-notification` zaten var):
  - CPU sıcaklığı > eşik;
  - boş disk < eşik;
  - bellek basıncı kritik;
  - kota < eşik;
  - konteyner unhealthy.
  - Aynı uyarı 30 dakikada bir kez.


> **Durum (2026-11-05): bitti.**
> - `app/src/system_history.rs`: ayrı redb dosyası (`system-history.redb`; ana depo okuma döngüsünün iş parçacığına ait, redb bir dosyayı süreç başına bir kez açar). Dakikada bir kayıt, 7 gün, en fazla 360 nokta.
> - `app/src/system_alerts.rs`: CPU > 90 °C, boş disk < 20 GB, bellek kritik, sağlıksız konteyner; 30 dk tekrar engeli. Ayarlardan değişir. Kota uyarıları zaten `alerts.rs`'de.

---

## Faz 10 — Görseller (isteğe bağlı)

`docs/design/GORSEL.md`: Apple tarzı, **şeffaf PNG** prompt'ları.

- Docker kapalı boş durumu: uyuyan konteyner yığını.
- "Araç kurulu değil" boş durumu.
- Dashboard arka plan dokusu (soyut cam ve ışık).
- Uygulama ikonu varyasyonları.

Kutluhan Antigravity'de üretir (ders: küçük partiler, 429'da bekle). Dosyalar `ui/src/assets/illustrations/` altına taşınır.


> **Durum (2026-11-05): bitti** — `docs/design/GORSEL.md` içindeki 8 görsel üretildi, `ui/src/assets/illustrations/` altına yerleştirildi ve `DockerTab.tsx`, `OverviewTab.tsx`, `Popover.tsx`, `AiTab.tsx`, `liquid.css` içine bağlandı.

---

## Faz 11 — Doğrulama ve teslim

- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm --prefix ui run typecheck && npm --prefix ui test`.
- Perf benchmark (Faz 0 hedefleri) yeşil.
- App Store derlemesi: `scripts/appstore.sh` + `check-appstore-config` + özel API sembol taraması.
- **Elle test listesi (Kutluhan):**
  - popover ve dashboard, açık/koyu tema;
  - Docker açık/kapalı;
  - fan onayı ve Auto'ya dönüş;
  - VoiceOver.
- `docs/USER_ACTIONS.md` güncellenir (notarization, logo kullanımı notu).


> **Durum (2026-11-05): otomatik kısım bitti.**
> - clippy iki modda temiz, `cargo fmt --check` temiz, Rust testleri ve arayüz testleri yeşil, `npm run build` temiz, `check-appstore-config` + Node testleri yeşil.
> - Uygulama geliştirme modunda 20 sn hatasız çalıştı. Arayüz tarayıcıda örnek veriyle gözden geçirildi.
> - Elle test listesi: `docs/USER_ACTIONS.md` §7.

---

## Sıra ve bağımlılıklar

```
F0 → F1 → F2 ─┐
      F3 ─────┤
      F4 ─────┼→ F5 → F6 → F7 → F8 → F9 → F11
              │                    F10 (bağımsız)
```

F3 ve F4, F1'den sonra paralel yürüyebilir. Arayüz (F5–F7) veri katmanı hazır olunca başlar; demo verisiyle önce de başlanabilir.
