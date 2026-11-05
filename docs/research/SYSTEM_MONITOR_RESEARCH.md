# Quota Deck v2 — Sistem izleme, Docker ve AI kotası araştırması

Tarih: 2026-11-04 · Hazırlayan: Claude (Kutluhan ile birlikte inceleme için)
Durum: **Araştırma raporu.** Kod yazılmadı; sonda (probe) yalnız scratchpad'de çalıştı.

---

## 0. Kısa özet

- Mole'un "Status" ekranındaki her şey (sıcaklık, fan, SSD sıcaklığı, güç, GPU, bellek basıncı, süreçler) **bu Mac'te root olmadan okunabiliyor**. Kendi yazdığım salt-okunur C sondasıyla doğruladım (§3).
- Hedef makine tek: **Mac mini M6** (`Mac18,5 · Apple M6 · 32 GB · macOS 27.2`). Başka Mac desteklenmiyor.
- **RAM sıcaklığı M6'da yok.** M4'te `Tm0p/Tm1p/Tm2p` ("Memory Proximity") var (Stats tablosu); M6'da hiç `Tm*` anahtarı yok. Arayüz "bu Mac'te yok" durumunu göstermeli.
- **SSD sıcaklığı var:** HID'de `NAND CH0 temp` ve SMC'de `TH0x/TH0a/TH0b` (~44 °C).
- **Fan okunuyor:** `F0Ac` 999 RPM, `F0Mn` 1000, `F0Mx` 4900, `F0md` 0 (Auto). Mole'un "Speed 20%" değeri = 999/4900.
- **Fan kontrolü** (Auto/Cool/Max) her rakipte root yetkili bir yardımcı süreç (privileged helper) istiyor. App Store'da yapılamaz.
- **Docker** şu an kapalı (soket yok). Okuma yolu: Docker Engine API (Unix soketi) ya da `docker` CLI. İkisi de yalnız sandbox dışı sürümde çalışır.
- **Antigravity**'nin yerel izi var: `~/.gemini/antigravity/log/*.log` içinde `QUOTA_EXHAUSTED` + `quotaResetTimeStamp`, konuşmalar SQLite'ta. Yüzde olarak "kalan kota" ise yalnız ağ üzerinden geliyor (CodexBar böyle yapıyor).
- **Lisans:** Mole CLI **GPL-3.0**; kod kopyalanamaz, yalnız fikir alınır. Mole Mac uygulaması kapalı kaynak. Stats, macmon, mactop ve CodexBar **MIT**; kod okunabilir ve uyarlanabilir.

---

## 1. İncelenen uygulamalar

| Uygulama | Tür | Lisans / fiyat | Öne çıkan | Bizim için ders |
|---|---|---|---|---|
| **Mole** (mole.fit + `tw93/mole`) | Temizlik + uygulama yönetimi + disk analizi + Status + menü çubuğu | CLI GPL-3.0; Mac app kapalı kaynak, tek seferlik ödeme | Status: Health skoru, CPU/GPU/Memory/Thermal/Free space/Network/Fan kartları, süreç tablosu (MEM, %CPU, PWR, PID), 60 sn sparkline, fan Auto/Cool/Max | Kart düzeni, sağlık skoru, "5 dk tepe" sıcaklık, menü çubuğu metni `TMP 48°C MEM 76%` |
| **Stats** (`exelban/stats`) | Menü çubuğu modülleri | MIT, ücretsiz | CPU, GPU, RAM, Disk (+SMART), Sensors, Fan, Net, Battery, Bluetooth | Sensör anahtar tabloları, NVMe SMART okuma, GPU % (`IOAccelerator`), bellek basıncı sysctl'i. Uyarısı: en pahalı modüller Sensors ve Bluetooth |
| **macmon** (`vladkens/macmon`) | Rust CLI + **kütüphane crate** | MIT | sudo'suz CPU/GPU/ANE güç, sıcaklık, RAM/swap, fan, E/P küme frekansı | **Bizim stack'e en uygun referans.** IOReport + SMC + HID, deps: `core-foundation`, `libc`, `serde` |
| **mactop** (`metaspartan/mactop`) | Go TUI | MIT | Apple Silicon güç/sıcaklık | macmon ile aynı yol; ek bilgi yok |
| **iStat Menus 7** | Ücretli menü çubuğu | Ticari | Sensörler, sıcaklığa/bataryaya göre fan rampası, hava durumu, geçmiş | Geçmiş grafikleri ve kural tabanlı uyarılar beklentisi |
| **TG Pro** | Sıcaklık + fan | Ticari, $10 tek sefer | Çekirdek başı sıcaklık, SMART, sıcaklıkla tetiklenen Auto Boost, uyarılar, CSV log, tanı raporu; M5'e kadar destek | Uyarı kuralları, CSV dışa aktarım |
| **Macs Fan Control** | Fan | Ücretsiz, Pro ücretli | Sabit RPM veya sensöre bağlı fan, preset; çıkışta Auto'ya döner | "Çıkarken Auto'ya dön" kuralı zorunlu |
| **Sensei** | İzleme + temizlik | $29/yıl | SMART, pil sağlığı, disk benchmark, geçmiş grafikleri | Kapsam dışı (temizlik istemiyoruz) |
| **CurveFan / ChillMac / FanBar / ExoMacFan** | Açık kaynak fan kontrolü | Çoğu MIT | Okuma yetkisiz, yazma root LaunchDaemon/XPC yardımcısında | Yardımcı süreç tasarımı; bazı makinelerde `thermalmonitord` yazmayı geri alıyor |
| **CodexBar** (`steipete/CodexBar`) | AI kota menü çubuğu | MIT | 60+ sağlayıcı; Claude/Codex/Antigravity oturum ve haftalık yüzdeler | Veriyi ağdan çekiyor (OAuth API, `agy` yerel HTTPS sunucusu). QuotaDeck'in "ağ yok" kuralıyla çelişir |
| **Gantry / BeszelBar / Deskmon / Walvis** | Docker menü/uygulama | Açık kaynak | Konteyner listesi, CPU/RAM | Yerel Docker için hazır, hafif bir menü çubuğu çözümü yok; boşluk var |

### Mole Status ekranının tam envanteri (Image #4, #9)

| Kart | İçerik | Bizde kaynak |
|---|---|---|
| HEALTH | skor + kelime ("100 Excellent"), çip/RAM/macOS, uptime, "Current readings normal" | Kendi formülümüz (§4) |
| CPU | %, sıcaklık çipi, çekirdek başı mini çubuklar, "idle · 12 cores · Load avg 1,7" | `host_processor_info`, `getloadavg`, SMC `Tp*/Te*` |
| GPU | %, sıcaklık, "idle · 0.1W" | `IOAccelerator` → `PerformanceStatistics`, SMC `Tg*`, IOReport GPU enerji |
| MEMORY | %, "Pressure normal", alan grafiği, "24,15 GB · 5 GB swap" | `host_statistics64`, `kern.memorystatus_vm_pressure_level`, `vm.swapusage` |
| THERMAL | CPU° + GPU°, ilerleme çubuğu, "M6 · 5-min peak 111°C", "Normal" çipi | SMC ortalama/maks + 5 dk halka tampon; `ProcessInfo.thermalState` |
| FREE SPACE | "1,7 TB", çubuk, "345 GB used · 17%" | `statfs` |
| NETWORK | KB/s, iniş/çıkış dalga grafiği, "Wi-Fi", "Low Lag" | `getifaddrs` sayaçları. **"Low Lag" ping istiyor: yasak** |
| FAN | RPM, "Speed 20%", Auto/Cool/Max, "Managed by macOS" | SMC `F0Ac/F0Mx/F0md`; kontrol ayrı karar (§6) |
| Süreç tablosu | NAME (144), MEM, %CPU (çubuk), PWR, PID, arama, `…` menüsü | `libproc` (`proc_listpids`, `proc_pid_rusage` → enerji) |
| Menü çubuğu menüsü (Image #10) | Open, Settings, Clean, …, Keep Screen On, Fan Cooling, Eject, Run Doctor | Bize uyanlar: Open Dashboard, Keep Awake, Eject |

---

## 2. Verinin kaynağı: teknik yöntemler

| Metrik | API | Yetki | Sandbox (App Store) | Not |
|---|---|---|---|---|
| CPU % (toplam, çekirdek, E/P) | `host_processor_info` (Mach) | Yok | Büyük olasılıkla çalışır | İki örnek arası fark |
| Load avg, uptime | `getloadavg`, `sysctl kern.boottime` | Yok | Çalışır | |
| RAM kullanımı, sıkıştırılmış, swap | `host_statistics64`, `sysctl vm.swapusage` | Yok | Çalışır | |
| Bellek basıncı | `sysctl kern.memorystatus_vm_pressure_level` (1/2/4) | Yok | Test edilecek | Bu Mac: 1 = normal |
| Termal durum | `NSProcessInfo.thermalState` | Yok | **Çalışır** (resmî API) | Nominal/Fair/Serious/Critical |
| CPU/GPU/SSD/SoC sıcaklıkları | SMC (`AppleSMC` → `IOConnectCallStructMethod`, selector 2) | Yok (okuma) | Muhtemelen **engelli**; `iokit-user-client-class` istisnası gerekir | §3'te doğrulandı |
| Sıcaklıklar (alternatif) | `IOHIDEventSystemClient` (özel API), usage page `0xff00`/5 | Yok | Test edilecek | 43 servis; isimler "PMU tdie*", "NAND CH0 temp" |
| Güç (CPU/GPU/ANE/DRAM W) | IOReport `Energy Model` (`libIOReport.dylib`, özel API) | Yok | Test edilecek | macmon'un yolu; örnekleme süresi gerekir (≥ 200 ms) |
| Sistem toplam gücü | SMC `PSTR` | Yok | SMC ile aynı | Bu Mac: 21,2 W |
| Fan RPM/min/max/mod | SMC `FNum`, `F0Ac`, `F0Mn`, `F0Mx`, `F0md` | Yok (okuma) | SMC ile aynı | |
| Fan **kontrolü** | SMC yazma (`F0md`=1, `F0Tg`; M1–M4'te `Ftst` kilidi) | **root yardımcı** | **İmkânsız** | §6 |
| GPU % | IORegistry `IOAccelerator` → `PerformanceStatistics["Device Utilization %"]` | Yok | Test edilecek | Bu Mac: 12 % |
| Disk boş/dolu | `statfs` | Yok | Kendi konteyneri dışında kısıtlı | |
| Disk okuma/yazma B/s | IORegistry `IOBlockStorageDriver` → `Statistics` | Yok | Test edilecek | |
| SMART durumu + NVMe sıcaklığı | `IONVMeSMARTUserClient` plug-in (Stats böyle yapıyor) | Yok | Muhtemelen engelli | `system_profiler SPNVMeDataType` → "Verified" |
| Ağ hızı | `getifaddrs` → `if_data` sayaçları | Yok | Çalışır | Paket göndermez, ağ isteği değil |
| Wi-Fi adı | CoreWLAN | **Konum izni** | — | Gereksiz, atla |
| Gecikme ("Low Lag") | ping/ICMP | — | — | **Ağ isteği: yasak** |
| Süreç listesi, enerji | `proc_listpids`, `proc_pidinfo`, `proc_pid_rusage` (`ri_billed_energy`) | Yok (kendi kullanıcısı) | Başka süreçler büyük olasılıkla görünmez | |
| Docker | Engine API: `GET /containers/json`, `/containers/{id}/stats?stream=false&one-shot=true`, `/system/df`, `/info` (Unix soketi `~/.docker/run/docker.sock`) **veya** `docker ps/stats/system df --format json` | Yok | **Çalışmaz** | §5 |

**Sonuç:** Sensör, güç, fan, Docker ve süreç verisinin tamamı **sandbox dışı (doğrudan dağıtım, notarized) sürümde** güvenle çalışır. App Store sürümünde büyük olasılıkla yalnız CPU, RAM, termal durum ve ağ kalır. Bunu Faz 0'da gerçek bir sandbox derlemesiyle ölçmek gerekiyor.

---

## 3. Bu Mac'te doğrulama (M6, macOS 27.2, root yok)

Sonda: `scratchpad/probe/sensors.c` (IOKit + CoreFoundation, salt okunur, yaklaşık 130 satır).

```
HID temperature services: 43   (PMU tdie0..19, PMU2 tdie1..10, tdev*, tcal, "NAND CH0 temp" 44.0 C)
SMC keys: 2066, 125 sıcaklık anahtarı
```

| SMC ailesi | Adet | Ort. °C | Maks °C | Anlamı (prefix kuralı, macmon/Stats) |
|---|---|---|---|---|
| `Tp*` | 21 | 52,7 | 60,5 | CPU performans kümesi |
| `Te*` | 9 | 50,4 | 51,1 | CPU verimlilik kümesi |
| `Tg*` | 18 | 49,4 | 49,5 | GPU |
| `Ts*` | 9 | 48,8 | 50,0 | SoC |
| `TH0x / TH0a / TH0b` | 3 | 44,3 | 44,8 | NAND / SSD |
| `TMVR` | 1 | 44,5 | — | Bellek voltaj regülatörü olabilir. **RAM sıcaklığı değil**, kesin değil |
| `TCMb` | 1 | 66,2 | — | Bilinmiyor |
| `Ta*`, `TR*`, `TV*`, `TP*`, `TW0P` | … | 38–68 | — | Ortam, regülatör, kablosuz; panelde gerek yok |
| `Tm*` | **0** | — | — | **RAM sıcaklığı yok** (M4'te `Tm0p..Tm2p` var) |

| Fan/güç | Değer |
|---|---|
| `FNum` | 1 fan |
| `F0Ac` | 999 RPM (`F0Mn` 1000, `F0Mx` 4900, `F0md` 0 = otomatik) |
| `PSTR` | 21,2 W sistem toplamı |
| GPU (`IOAccelerator`) | 12 % |
| Bellek basıncı | 1 (normal), swap 5,1 / 6 GB |
| SSD | APPLE SSD AP0256Z, SMART "Verified" |

**Önemli ders:** Stats sensör anahtarlarını çip başına tabloyla tutuyor ve her yeni çipte geride kalıyor (README'si bunu kendisi söylüyor). M6 bu tabloda yok. Bizim yöntemimiz **prefix ailesine göre dinamik** olmalı: `Tp/Te` → CPU, `Tg` → GPU, `TH0` → SSD, `Tm` → RAM (varsa). Her aile için ortalama ve maksimum alınır, gerçek dışı değerler (<1 veya >150 °C) atılır. macmon da böyle yapıyor; bu yöntem M6'da doğrudan çalıştı.

---

## 4. Ne eklenmeli? Öneri listesi

Öncelik: **P0** = ilk sürüm, **P1** = sonra, **P2** = belki.

### 4.1 Mac (sistem)

| Öz. | Öncelik | Açıklama |
|---|---|---|
| CPU kartı | P0 | %, E/P küme çubukları, load avg, sıcaklık çipi (Tp/Te ort.), 60 sn sparkline |
| GPU kartı | P0 | %, sıcaklık (Tg ort.), W (IOReport) |
| Bellek kartı | P0 | %, basınç çipi (Normal/Warning/Critical), kullanılan/toplam, swap, sıkıştırılmış |
| Termal kartı | P0 | CPU° + GPU°, `thermalState` çipi, "5 dk tepe" (en yüksek sensör) |
| Disk kartı | P0 | Boş alan, çubuk, okuma/yazma B/s, SSD sıcaklığı (TH0x/NAND), SMART durumu; harici diskler liste olarak |
| Fan kartı | P0 | RPM, % (Ac/Mx), "Managed by macOS"; **salt okunur** |
| RAM sıcaklığı | P0 | M6 da `Tm*` yok → "Bu Mac.te sensör yok". Uydurma değer gösterilmez |
| Güç kartı | P1 | Sistem toplamı (PSTR), CPU/GPU/ANE/DRAM ayrımı (IOReport) |
| Ağ kartı | P0 | İniş/çıkış B/s, dalga grafiği. Ping yok |
| Süreç tablosu | P0 | Ad, bellek, %CPU, enerji (PWR), PID, arama; ilk 10 |
| Sağlık skoru | P0 | Mole'un ağırlık fikri (CPU 30, bellek 25, disk 20, termal 15, IO 10; SMART "failing" ise tavan 44; eşikler 85/65/45). **Kod kendimizin**, GPL'den kopya yok |
| Geçmiş (24 s) | P1 | 1 dk'lık özetler mevcut `redb` deposuna; dashboard'da grafik |
| Uyarılar | P1 | Sıcaklık > X, disk < Y GB, bellek basıncı kritik, kota < %10. `tauri-plugin-notification` zaten var |
| Menü çubuğu metni | P0 | `48° · 76% · ✳ 67%` (CPU sıcaklığı · bellek · en dar kota). Tauri `tray.set_title` |
| Keep Awake | P2 | `IOPMAssertionCreateWithName`; sandbox'ta da çalışır |
| Diski çıkar (Eject) | P2 | DiskArbitration |
| Fan kontrolü | P2 | Ayrı karar (§6) |

### 4.2 Docker

| Öz. | Öncelik | Açıklama |
|---|---|---|
| Durum | P0 | Çalışıyor / Docker Desktop kapalı / kurulu değil |
| Özet | P0 | Konteyner sayısı (çalışan/toplam), toplam CPU, toplam RAM / VM limiti, disk (imaj + volume + build cache) |
| Konteyner tablosu | P0 | Ad, durum noktası, CPU %, RAM; compose projesine göre gruplu, "+14 daha" |
| Sağlık | P1 | `healthcheck` durumu (healthy/unhealthy), yeniden başlatma sayısı |
| Eylemler | P2 | Başlat/durdur. **Yazma işlemi** olduğu için ayrı karar |

### 4.3 AI — yalnız Claude, Codex, Antigravity

| Öz. | Öncelik | Açıklama |
|---|---|---|
| Diğer 6 sağlayıcıyı gizle | P0 | Amp, Copilot, Droid, Gemini, OpenCode, PiAgent varsayılan olarak kapalı (kod silinmez, ayarlardan açılır) |
| Claude / Codex | P0 | Mevcut veri: oturum/haftalık pencere, tempo, maliyet, 30 gün. Tasarıma uygun yeni kartlar |
| Antigravity sağlayıcısı | P0 | Yeni dosya `providers/src/antigravity.rs` + fixture + `providers/mod.rs` kaydı (CLAUDE.md kuralı) |
| Antigravity "kota bitti" | P0 | `~/.gemini/antigravity/log/cli-*.log` içindeki `QUOTA_EXHAUSTED` + `quotaResetTimeStamp` + model → "Model X dolu, 4 sa 52 dk sonra açılır" (**ölçülmüş**) |
| Antigravity etkinlik | P1 | `conversation_summaries.db` (salt okunur SQLite: konuşma sayısı, son etkinlik, adım sayısı), `antigravity-cli/history.jsonl` (istem sayısı/gün) |
| Antigravity token | P2 | `conversations/*.db` → `gen_metadata` protobuf blob'ları. Model adı okunuyor (`gemini-3.8-flash-high`); token alanları şemasız, tersine mühendislik gerekiyor |
| Antigravity % kalan | Karar | Yalnız ağdan geliyor (§6, karar 2) |

---

## 5. Mimari öneri

```
core/        (değişmez: log motoru)
providers/   + antigravity.rs
sysmon/      YENİ crate — macOS'a özel okuyucular, her biri tek dosya:
  cpu.rs  mem.rs  disk.rs  net.rs  proc.rs  thermal.rs (SMC+HID)  power.rs (IOReport)
  gpu.rs  fan.rs  docker.rs  health.rs
app/         sysmon'u zamanlayıcıyla örnekler, panele/dashboard'a olay yollar
ui/          Panel (menü çubuğu) + Dashboard penceresi (zaten var: DASHBOARD_WINDOW)
```

- **Örnekleme hızı:** Dashboard açıkken 1 sn, yalnız panel açıkken 2 sn, ikisi de kapalıyken 10 sn (yalnız menü çubuğu metni). Sensörler ve IOReport pahalı; Stats aynı uyarıyı yapıyor.
- **Bağımlılıklar** (CLAUDE.md: önce boyut ve bellek etkisini söyle):
  - `macmon` crate (MIT; `core-foundation` + `libc` + `serde`; bunlar ağaçta zaten var). Binary'ye yaklaşık 100–200 KB ekler, ağ kodu yok. **Seçenek A:** dependency olarak eklemek. **Seçenek B:** yalnız gereken SMC/HID/IOReport parçalarını (~600 satır) MIT atfıyla `sysmon/`'a taşımak. Önerim B: tek bakımcılı bir crate'in API'sine bağlı kalmayız.
  - `sysinfo` crate (CPU/RAM/disk/ağ/süreç, çapraz platform). Yaklaşık 300–500 KB ekler. Windows/Linux sürümleri de basit metrikleri alır. Alternatif: doğrudan `libc` (~300 satır, yalnız macOS).
- **Dashboard penceresi:** Mole tarzı geniş pencere (yaklaşık 1100×720), üstte sekmeler **Overview · Mac · Docker · AI**. Menü çubuğu paneli hızlı bakış için kalır, "Open Dashboard" ile tam ekran açılır.
- **Güvenilirlik kuralları korunur:** okunamayan sensör `Unavailable` olur; tahmin edilen değer "Estimated" rozetiyle gösterilir; tek bir sensör hatası tüm kartı düşürmez (Mole'un "bir toplayıcı patlasa da gerisi gelir" ilkesi).

---

## 6. Kutluhan'ın kararı gereken noktalar

1. **Docker erişimi.** Unix soketine elle yazılmış küçük bir HTTP isteği **ya da** `docker` CLI alt süreci. Soket daha hızlı, CLI daha basit. İkisi de sandbox dışı. *Önerim: CLI.* "HTTP istemcisi yok" kuralına dokunmaz; Docker kapalıysa hemen hata verir.
2. **Antigravity % kalan.** (a) Yalnız yerel log: "kota bitti, X'te açılır" ve etkinlik bilgisi gelir, yüzde gelmez. (b) `agy -p /usage --output-format json` alt süreci: gerçek yüzdeler gelir ama `agy` Google'a bağlanır ve OAuth açabilir; CodexBar bunu yapıyor. *Önerim: (a).* "Ağ yok" sözü korunur.
3. **Fan kontrolü.** (a) Salt okunur. (b) Mole gibi Auto/Cool/Max: root XPC yardımcı süreç (SMAppService daemon) gerekir, uygulama çıkarken Auto'ya dönmesi zorunlu, App Store'da imkânsız, M1–M4'te `Ftst` kilidi gerekir. *Önerim: şimdilik (a).*
4. **App Store sürümü.** Sistem sekmesi orada kısıtlı ya da tamamen yok. Sistem izleme **yalnız doğrudan dağıtım** sürümünde mi olsun? *Önerim: evet.* App Store sürümü bugünkü gibi yalnız AI kotası kalsın.
5. **Diğer 6 sağlayıcı.** Varsayılan olarak gizlensin mi, yoksa kod tamamen mi kaldırılsın? *Önerim: gizle.*
6. **Pencere.** Mole gibi geniş bir dashboard + küçük menü çubuğu paneli. Claude Design tasarımı hangi oranda? Tasarım dosyası gelince eşleştireceğim.

---

## 7. Faz önerisi (kararlardan sonra kesinleşir)

| Faz | İçerik | Bitti sayılır |
|---|---|---|
| **0 — Sonda** | `sysmon` iskeleti; SMC/HID/IOReport okuma; **sandbox derlemesinde** hangisinin çalıştığını ölç; CPU/RAM maliyetini ölç | Ölçüm tablosu, kırmızı benchmark yok |
| **1 — Temel metrikler** | CPU, RAM, basınç, disk, ağ, süreçler, sağlık skoru; fixture testleri | `cargo clippy -D warnings` temiz |
| **2 — Sensörler** | Sıcaklıklar (prefix aileleri), SSD/NAND, fan (okuma), güç, termal durum, 5 dk tepe | Mac mini M6 da doğru değer |
| **3 — Docker** | Seçilen yöntemle durum, özet, konteyner tablosu | Docker açık/kapalı/kurulu değil durumları |
| **4 — Antigravity** | Sağlayıcı + fixture (anonim) + log ayrıştırma; diğer sağlayıcıları gizle | Gerçek logla doğrulama |
| **5 — Arayüz** | Claude Design tasarımı: panel + dashboard + menü çubuğu metni | Ekranda elle test |
| **6 — Geçmiş ve uyarılar** | 24 sa geçmiş, bildirim kuralları | — |

---

## 8. Kaynaklar

- Mole: https://mole.fit/ · https://github.com/tw93/mole (GPL-3.0) · fan karşılaştırması: https://mole.fit/blog/mac-fan-control-apps
- Stats: https://github.com/exelban/stats (MIT)
- macmon: https://github.com/vladkens/macmon (MIT)
- mactop: https://github.com/metaspartan/mactop (MIT)
- iStat Menus: https://bjango.com/mac/istatmenus/
- TG Pro: https://www.tunabellysoftware.com/tgpro/
- Macs Fan Control: https://crystalidea.com/macs-fan-control
- CurveFan: https://github.com/BeastOrange/CurveFan
- CodexBar: https://github.com/steipete/CodexBar (MIT) · Antigravity notları: `docs/antigravity.md`
- Karşılaştırma: https://cindori.com/reviews/best-mac-performance-monitor-apps
- AI kota izleyicileri: https://sessionwatcher.com/guides/best-claude-code-usage-trackers
- Docker Engine API: https://docs.docker.com/reference/api/engine/
- Docker menü uygulamaları: https://github.com/hurryingauto3/container-mb · https://github.com/Willow-Walker/deskmon
