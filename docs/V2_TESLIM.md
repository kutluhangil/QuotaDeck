# Quota Deck 2.0 — Teslim raporu (2026-11-05)

Dağıtım: **yalnız GitHub**, açık kaynak, **GPL-3.0**. Plan ve kararlar: `docs/V2_PLAN.md`. Senin yapacakların: `docs/USER_ACTIONS.md` §1 (yayın) ve §7 (elle test).

**Hiçbir şey commit edilmedi.** "commit at" dediğinde commit ve push ayrı ayrı yapılır.

## Ne var

| Alan | Durum |
|---|---|
| AI kotası: Claude Code, Codex, Antigravity | Bitti. Diğer 6 araç yeni kurulumda gizli, Ayarlar'dan açılır |
| Mac izleme: CPU, GPU, bellek, disk, ağ, süreçler, sıcaklıklar, fan, güç, SMART, sağlık skoru | Bitti |
| Docker (yalnız okuma) | Bitti; açık, kapalı ve kurulu değil durumları denendi |
| Liquid Glass popover (gerçek saydam pencere) ve dört sekmeli Dashboard | Bitti |
| Ayarlar: yenileme hızı, menü çubuğu içeriği, Mac uyarıları, Antigravity canlı kota, Keep Awake | Bitti |
| Menü çubuğu: `48° · 76%` ve 🟠/🔴 uyarı noktası | Bitti |
| 7 günlük geçmiş, 1 sa / 24 sa / 7 g grafikleri, Antigravity 30 günlük istem grafiği | Bitti |
| Dört dil (EN, TR, DE, ES) | Bitti |
| Fan kontrolü (Auto / Cool / Max) | Yazıldı; arayüzde gizli, bu Mac'te tek komutla denenmeyi bekliyor |
| GitHub yayın betiği (imza + Apple onayı + DMG) | Yazıldı; imzasız paket düzeni doğrulandı; imzalı hâli sertifikanı bekliyor |
| README, CHANGELOG (2.0.0), site metni, lisans | Güncellendi |
| App Store ve Microsoft Store dosyaları | `stores/` altına toplandı, CI'dan çıktı; gerekirse yine derlenebilir |

**Doğrulama:**

| Kontrol | Sonuç |
|---|---|
| Rust testleri | 579/579 |
| Arayüz testleri | 86/86 |
| Clippy | temiz (varsayılan derleme ve App Store katmanıyla derleme) |
| Biçim | temiz |
| Arayüz ve site derlemesi | temiz |
| Fan yardımcısı iç testi | geçti |
| HTTP istemcisi bağımlılığı | yok |
| Okuma turu maliyeti | ~2,5 ms CPU |

## Bilmen gerekenler

1. **Developer ID sertifikası anahtar zincirinde yok.** Hesabın var ama sertifika oluşturulmamış. Xcode'dan tek seferlik bir adım (`USER_ACTIONS.md` §1). Gelene kadar imzalı DMG üretilemez.
2. **Popover'ın camı gerçek uygulamada gözle denenmedi.** Ekran kapalı ya da kilitliydi. Uygulama çalıştı ve menü çubuğu başlığını (`72° · 77%`) sistemden okudum, ama popover'ın görüntüsünü alamadım. Tarayıcı önizlemesi doğru görünüyor.
3. **Bu Mac'in sunmadığı değerler boş kalıyor, tahmin edilmiyor:** RAM sıcaklığı; CPU, Neural Engine ve RAM watt'ı.
4. **Codex maliyeti "—" görünüyor.** Codex'in kullanım kayıtlarında model adı yok, fiyat tablosunda da OpenAI modeli yok. Fiyat uydurmamak için böyle. İstersen `pricing-overrides.json` ile fiyat verebilirsin.
5. **Yapılmayan tek parça: Docker'da yeniden başlatma sayısı.** Bu bilgi yalnız `docker inspect` ile geliyor; o komut izinli okuma komutları arasında değil.
6. **Windows ve Linux** yalnız AI kotası tarafını kapsıyor ve kaynaktan derleniyor. Saydam popover Linux'ta kapalı.
7. **Menü çubuğundaki renkli nokta bir emoji** (🟠/🔴), çünkü macOS menü çubuğu başlığı düz metin.
8. **Logolar** sahiplerinin markası. Dosyaların kaynağı: `ui/src/assets/logos/README.md`.
9. **Site** (`site/`) güncellendi ama hiçbir yerde yayında değil; CI yalnız derliyor.
10. **Tauri komut satırı aracı 2.12.4'e yükseltildi.** Eski sürüm Liquid Glass ayarını tanımıyordu. Yalnız geliştirme aracı; uygulamaya girmiyor.
11. **App Store yolu:** `stores/` altındaki yolu yeniden açarsan, özellik kapalı derlemede App Store yapılandırmasının da verilmesi gerekiyor. Yazılı: `stores/README.md`.

## Senin yapacakların (sırayla)

1. **Fan denemesi** (yönetici şifren gerekiyor):
   ```
   sudo scripts/fanhelper-dev.sh install
   ```
   Kurulunca testi ben çalıştırır, geçerse fan düğmelerini açarım; ardından kaldırırsın.
2. **Developer ID sertifikası ve onay kimliği:** `USER_ACTIONS.md` §1.
3. **Yayın:**
   ```
   TEAM_ID=<TEAM_ID> scripts/release-macos.sh
   ```
   Ardından betiğin yazdırdığı `gh release create …` komutu.
4. **Elle test listesi:** `USER_ACTIONS.md` §7.
5. "commit at".
