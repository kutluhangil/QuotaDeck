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

Öncelikli adaylar: OpenCode, Windsurf, JetBrains, Cursor, Kimi, Gemini CLI ve Qwen Code. Bir aracın yalnız OAuth/cookie/API ile kota sunduğu anlaşılırsa o araç Quota Deck’e eklenmez.

## Güvenlik notu

Sertifika, `.p8`, provisioning profile, API anahtarı, cookie, token veya auth dosyası Git’e eklenmez ve sohbet içine yapıştırılmaz. Gerekli sırlar yalnız yerel ortam değişkeni veya işletim sisteminin imzalama altyapısı üzerinden kullanılır.
