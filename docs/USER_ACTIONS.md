# Quota Deck — Kullanıcıdan Gerekenler

Bu dosyada yalnızca kodla, CI ile veya bu macOS oturumunda güvenilir biçimde tamamlanamayacak işler bulunur. Codex’in yapabildiği geliştirme, test, yerel macOS build ve otomatik doğrulamalar buraya taşınmaz.

## 1. GitHub sürümü (macOS)

Dağıtım yalnız GitHub'dan, açık kaynak (GPL-3.0). App Store ve Microsoft Store dosyaları `stores/` altında park edildi; bakımları yok.

- [ ] **Developer ID sertifikası** (bir kez): Xcode → Settings → Accounts → Apple ID'n → Manage Certificates → `+` → **Developer ID Application**. Kontrol:
  ```
  security find-identity -v -p codesigning
  ```
  Listede `Developer ID Application: … (TEAM_ID)` görünmeli.
- [ ] **Apple onay (notarization) kimliği** (bir kez; appleid.apple.com'dan uygulamaya özel parola oluştur, komut onu sorar):
  ```
  xcrun notarytool store-credentials quotadeck-notary --apple-id <apple id e-postan> --team-id <TEAM_ID>
  ```
- [ ] **Sürümü üret** (imzalar, Apple'a onaylatır, DMG'ye zımbalar; hiçbir şey yayımlamaz):
  ```
  TEAM_ID=<TEAM_ID> scripts/release-macos.sh
  ```
- [ ] **Yayımla:** betiğin sonunda yazdırdığı `gh release create v2.0.0 …` komutunu çalıştır.

## 2. Mağazalar

Park edildi; yeniden açmak gerekirse adımlar `stores/README.md` ve `stores/apple/STORE.md`'de.

## 3. Gerçek insan QA’sı

- [ ] macOS oturumunun kilidini açıp `target/release/bundle/macos/Quota Deck.app` uygulamasını çalıştır; tek tray öğesini, Open/panel aç-kapat ve odak-kaybında gizlenme davranışını doğrula.
- [ ] Tray menüsündeki sağlayıcı özetlerini, Dashboard ve Refresh eylemlerini dene; panel ve dashboard Refresh düğmelerinin bekleme durumunu bitirdiğini ve hata varsa görünür metin ürettiğini doğrula.
- [ ] VoiceOver ile panel, ayarlar ve dashboard sekme/okuma sırasını dinle; çift okunan yüzdeleri ve kaybolan odağı not et.
- [ ] İmzalı sürümü 72 saat açık bırak; başlangıç ve bitiş RSS değerlerini kaydet. Artış %20’yi geçerse raporla.
- [ ] Gerçek Claude Code/Codex değerlerini araçların kendi ekranlarıyla yan yana karşılaştır ve farkı tarih/saatle bildir.

## 4. Windows doğrulaması

- [ ] Windows geliştirici makinesinde x64 ve arm64 NSIS paketlerini üret.
- [ ] Authenticode sertifikasıyla kurucu ve kurulan PE dosyalarının imzasını `Valid` olarak doğrula.
- [ ] Tray, panel odağı, tıklayınca gizleme, login startup kaydı ve `/S` sessiz kurulumu gerçek Windows oturumunda dene.
- [ ] Microsoft Partner Center hesabı, yayıncı adı, ürün rezervasyonu ve HTTPS kurucu adreslerini tamamla.

## 5. Linux doğrulaması

- [ ] Debian/Ubuntu veya Fedora masaüstünde `.deb`, `.rpm` ve AppImage paketlerini üret.
- [ ] GNOME/Cinnamon/Budgie/XFCE ve mümkünse KDE’de tray menüsü, panel konumu ve paket bağımlılıklarını dene.
- [ ] Yayın artefaktlarını GitHub Releases’e yükledikten sonra kalıcı bağlantıları bildir.

## 6. Yeni sağlayıcılar için gerçek veri

Quota Deck şema tahmin ederek parser yazmaz. Eklenmesini istediğin her araç için:

- [ ] Aracı kendi hesabınla kur ve en az iki normal oturum çalıştır.
- [ ] Bir oturumda mümkünse model değişimi, cache kullanımı, alt ajan veya kota hatası gibi sınır durumunu üret.
- [ ] Yerel log/veritabanı konumunu bildir; kimlik bilgisi dosyası gönderme.
- [ ] Codex’e dosyanın yalnız gerekli birkaç anonymize kaydını fixture’a dönüştürme izni ver.

- [ ] **Copilot CLI (mevcut sağlayıcı):** araç yeni native runtime'a geçmiş ve 2026-10-19 itibarıyla `~/.copilot/session-state` altında `events.jsonl` bırakmıyor; Quota Deck bu yüzden Copilot için hiçbir şey okuyamıyor. Bir Copilot oturumunu normal şekilde başlatıp kapat, ardından oturumun nereye yazıldığını bildir. Kimlik bilgisi dosyası gönderme.

- [ ] **Gemini CLI, OpenCode, pi/OMP, Amp, Droid (2026-10-24'te eklendi):** parser'lar gerçek dosya yerine ccusage'ın açık kaynak okuyucularındaki biçimden yazıldı; fixture'ların hepsi `synthetic_`. Her biri için aracı kurup iki oturum çalıştır, ardından şunları karşılaştır:
  - `quotadeckctl report daily --provider <anahtar>` çıktısı aracın kendi kullanım ekranıyla aynı mı (anahtarlar: `gemini-cli`, `opencode`, `pi-agent`, `amp`, `droid`).
  - Dosya konumları: `~/.gemini/tmp/*/chats/`, `~/.local/share/opencode/opencode.db`, `~/.pi/agent/sessions/`, `~/.local/share/amp/threads/`, `~/.factory/sessions/`.
  - Uyuşursa birkaç anonim satırı `providers/tests/fixtures/<sağlayıcı>/` altına gerçek fixture olarak ekle, sonra sitedeki "Tools it reads" listesine (`site/src/copy/en.ts`, `tr.ts`) ekle. Site, doğrulanmamış aracı listelemez.
- [ ] **Fiyat geçersiz kılma:** Gemini veya tabloda olmayan bir modeli fiyatlamak istersen uygulama veri klasörüne `pricing-overrides.json` koy (`{"models": {"gemini-2.5-pro": {"input": 0.00000125, "output": 0.00001, "cacheRead": 0.000000125}}}`, USD/token). Değişiklik bir sonraki açılışta geçmişi yeniden fiyatlar.
- [ ] **Kısayol:** Ayarlar → Klavye kısayolu → Açık; `⌘⌥Q` (Windows/Linux `Ctrl+Alt+Q`) paneli açıp kapatmalı. Linux Wayland'da çalışmaz; hata mesajı görünmeli.

Bir aracın yalnız OAuth/cookie/API ile kota sunduğu anlaşılırsa o araç Quota Deck’e eklenmez (Cursor, Windsurf, JetBrains bu yüzden yok).

## 7. Quota Deck v2 (Mac izleme) — elle denenecekler

Plan: `docs/V2_PLAN.md`. Bunlar yalnız doğrudan indirilen (notarized) sürüm için.

- [ ] **Popover ve Dashboard, açık ve koyu tema:** `npm --prefix ui exec tauri -- dev` (ya da imzalı derleme). Popover'ın camı arkadaki masaüstünü buzlu olarak gösteriyor mu (Liquid Glass, macOS 26+); menü çubuğu başlığı `48° · 76%` biçiminde mi; popover'daki modüller ve Dashboard'un dört sekmesi (Genel, Mac, Docker, AI) gerçek değerleri gösteriyor mu.
- [ ] **Docker açık/kapalı:** Docker Desktop'ı kapatınca Docker sekmesi "çalışmıyor" ekranına ve uyuyan konteyner animasyonuna geçmeli; açınca 5 sn içinde konteynerler gelmeli.
- [ ] **Antigravity canlı kota:** Ayarlar → Antigravity → "agy ile canlı kota" açılınca en geç 30 sn içinde haftalık yüzdeler görünmeli. Bu ayar açıkken `agy` hesabınla Google'a bağlanır.
- [ ] **Bildirimler:** CPU sıcaklığı / boş disk / bellek basıncı / sağlıksız konteyner uyarıları. Denemek için Ayarlar → Uyarılar'da disk eşiğini mevcut boş alanın üstüne çek; bildirim 30 dakikada bir kez gelmeli.
- [ ] **Keep Awake:** Popover'daki "Uyanık" düğmesi açıkken `pmset -g assertions` çıktısında `Quota Deck: Keep Awake` görünmeli; kapatınca kaybolmalı.
- [ ] **VoiceOver:** popover ve Dashboard sekmeleri sesli okunuyor mu.
- [ ] **Fan kontrolü (şimdilik arayüzde gizli):** bu Mac'te tek seferlik deneme, Developer ID gerekmeden:
  ```
  sudo scripts/fanhelper-dev.sh install
  ```
  Ardından `scripts/fanhelper-dev.sh test` fanı 8 sn Cool'a alıp Auto'ya döndürür ve devirleri yazar (bunu Claude da çalıştırabilir). Bitince kaldır:
  ```
  sudo scripts/fanhelper-dev.sh uninstall
  ```
  Test geçince `ui/src/liquid/MacTab.tsx` içinde `FAN_CONTROL_READY = true` yapılır; imzalı sürümde yardımcı ilk Cool/Max basışında SMAppService ile kurulur ve macOS Sistem Ayarları → Giriş Öğeleri'nden bir kez onay ister.
- [ ] **Logolar (K7):** Claude ve OpenAI işaretleri simple-icons'tan (CC0 dosya, marka hakkı sahiplerinde), Antigravity işareti antigravity.google'dan. Ticari yayından önce yeniden değerlendir (`ui/src/assets/logos/README.md`).

## Güvenlik notu

Sertifika, `.p8`, provisioning profile, API anahtarı, cookie, token veya auth dosyası Git’e eklenmez ve sohbet içine yapıştırılmaz. Gerekli sırlar yalnız yerel ortam değişkeni veya işletim sisteminin imzalama altyapısı üzerinden kullanılır.
