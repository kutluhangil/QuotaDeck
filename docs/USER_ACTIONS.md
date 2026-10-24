# Quota Deck — Kullanıcıdan Gerekenler

Bu dosyada yalnızca kodla, CI ile veya bu macOS oturumunda güvenilir biçimde tamamlanamayacak işler bulunur. Codex’in yapabildiği geliştirme, test, yerel macOS build ve otomatik doğrulamalar buraya taşınmaz.

## 1. Ürün kararları

- [ ] Tek seferlik satış fiyatını seç.
- [ ] Kullanılacak alan adını satın al ve kesin alan adını bildir.
- [ ] CLI için dağıtım tercihini kesinleştir: ayrı GitHub/Homebrew artefaktı ya da yalnız uygulama içi özellik.

## 2. Apple hesabı ve App Store Connect

- [ ] Login Keychain’e `Apple Distribution` sertifikasını ve özel anahtarını ekle.
- [ ] Login Keychain’e `3rd Party Mac Developer Installer` sertifikasını ve özel anahtarını ekle.
- [ ] `com.kutluhangil.quotadeck` için App Sandbox açık Mac App Store provisioning profile oluştur ve `app/MacAppStore.provisionprofile` olarak yerleştir; commit etme.
- [ ] App Store Connect API anahtarı oluştur; `.p8` dosyasını güvenli konumda tut ve Key ID/Issuer ID değerlerini ortam değişkenleriyle sağla.
- [ ] App Store Connect’te uygulama kaydını, fiyat/bölge fiyatlarını, gizlilik anketini ve listeleme metnini tamamla.
- [ ] `<TEAM_ID>.com.kutluhangil.quotadeck.shared` App Group'unu oluştur ve hem uygulamaya hem widget'a ekle. Widget'ın okuyabildiği tek yer burası.
- [ ] Widget için ayrı App ID oluştur: `com.kutluhangil.quotadeck.widget`.
- [ ] Widget için Mac App Store provisioning profile oluştur ve `app/widget/QuotaDeckWidget.provisionprofile` olarak yerleştir; commit etme. Appex kendi profilini gömer, host'unki onu kapsamaz.
- [ ] İlk imzalı `.pkg` yüklemesinden sonra TestFlight veya App Review gönderimini başlat.

## 3. Gerçek insan QA’sı

- [ ] macOS oturumunun kilidini açıp `target/release/bundle/macos/Quota Deck.app` uygulamasını çalıştır; tek tray öğesini, Open/panel aç-kapat ve odak-kaybında gizlenme davranışını doğrula.
- [ ] Tray menüsündeki sağlayıcı özetlerini, Dashboard ve Refresh eylemlerini dene; panel ve dashboard Refresh düğmelerinin bekleme durumunu bitirdiğini ve hata varsa görünür metin ürettiğini doğrula.
- [ ] VoiceOver ile panel, ayarlar ve dashboard sekme/okuma sırasını dinle; çift okunan yüzdeleri ve kaybolan odağı not et.
- [ ] İmzalı sandbox build’ini 72 saat açık bırak; başlangıç ve bitiş RSS değerlerini kaydet. Artış %20’yi geçerse raporla.
- [ ] Mağaza ekran görüntüleri için gerçek tray öğesi ve yerel bildirim görüntülerini onayla.
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

## Güvenlik notu

Sertifika, `.p8`, provisioning profile, API anahtarı, cookie, token veya auth dosyası Git’e eklenmez ve sohbet içine yapıştırılmaz. Gerekli sırlar yalnız yerel ortam değişkeni veya işletim sisteminin imzalama altyapısı üzerinden kullanılır.
