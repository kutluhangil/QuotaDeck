# Sende kalanlar

Kodun bitirdiği yer burası. Aşağıdakilerin hiçbiri bir commit'le çözülmüyor — hepsi ya bir
hesap, ya bir makine, ya da bir insanın kararını istiyor.

Her madde **neden bir commit'in yapamadığını** yazıyor. Bir maddede takılırsan, o gerekçe
genelde alternatifi de söylüyor.

Teknik dayanaklar: derleme yolları `docs/STORE.md`, mimari kararlar `QUOTA_DECK_BLUEPRINT.md`,
kabul kriterleri blueprint Ek A.

---

## Kod tarafı — son doğrulama (2026-08-01)

Hepsi bu makinede koşuldu, hepsi yeşil. CI aynılarını her push'ta tekrar koşuyor.

- [x] `cargo clippy --workspace --all-targets -- -D warnings` — temiz
- [x] `cargo test --workspace` — 285 test geçti, 0 başarısız
- [x] Perf bütçesi (`cargo test -p quotadeck-core --release --test perf -- --ignored`) — dördü de bütçe içinde:
      cold parse 69 ms / 160,3 MB (500 dosya), sıcak tik 3,4 ms ve **0 byte** okuma (500 imleç),
      bir saatlik izleme yalnızca eklenen 65 343 byte'ı okudu, tepe RSS 6,9 MB
- [x] `ui` — `tsc --noEmit` temiz, 47 vitest geçti
- [x] `site` — strict TypeScript kontrolü temiz, 6 sayfa derlendi (EN + TR), CSP kontrolü geçti
- [x] Çalışma ağacı temiz, `main` üzerinde

Blueprint Faz 0–12 kapalı. Bu dosyanın geri kalanındaki kutuların **hiçbiri bir commit'le
kapanmıyor**; sürüm numarası hariç, o da aşağıda kapatıldı.

---

## 0. Önce karar ver — üçü de kodu etkiliyor (biri kapandı)

### 0.1 Sürüm numarası

Mağazaya `1.0.0` gitmeli; App Store bir kez yükledikten sonra aynı sürüm numarasını ikinci kez
kabul etmiyor.

- [x] `app/tauri.conf.json` → `"version": "1.0.0"`
- [x] `Cargo.toml` `[workspace.package]` → `version = "1.0.0"` (üç crate de bu sürümü alıyor,
      `Cargo.lock` tazelendi)

Bundan sonrası sürüm artırımı: her mağaza gönderimi bir öncekinden büyük bir numara istiyor.

### 0.2 Fiyat

`docs/STORE.md` §4 tek seferlik satın alma diyor, aralık olarak $7.99–$12.99 yazıyor. Rakam
seçilmemiş durumda ve **site bilerek fiyat yazmıyor** — seçilmemiş bir rakamı sayfaya koymak,
sonradan değiştirmen gereken bir iddia olurdu.

- [ ] Fiyatı seç. CUStats $9.99'da duruyor; altına inmek "daha ucuz taklit", üstüne çıkmak
      "neden pahalı" sorusunu doğuruyor.
- [ ] Türkiye dâhil bölgesel fiyatlandırmayı App Store Connect'te kontrol et — Apple'ın otomatik
      dönüşümü bazı ülkelerde saçma rakamlar üretiyor.

### 0.3 Alan adı

Site şu an `quotadeck.app` varsayıyor. Farklı bir ad alırsan **iki dosya** değişecek:

- `site/astro.config.mjs` → `site: "https://..."`
- `site/public/sitemap.txt` → altı satırın tamamı

- [ ] Alan adını al ve bana adını söyle; iki dosyayı güncellerim.

---

## 1. Mac App Store

Hesabın açık, yani bu bölüm bugün başlayabilir. Sıra önemli: 1.1 olmadan 1.2 çalışmaz.

### 1.1 Sertifikalar — Apple Developer portalı

İkisi de **login keychain'de** olmalı; `scripts/appstore.sh` oradan okuyor.

- [ ] **Apple Distribution** sertifikası oluştur ve indir, çift tıkla keychain'e ekle
- [ ] **3rd Party Mac Developer Installer** sertifikası oluştur ve indir, çift tıkla ekle

Doğrulama — ikisi de listede görünmeli:

```
security find-identity -v -p codesigning
```

> Neden ben yapamıyorum: sertifika üretimi Apple hesabında oturum açmayı ve özel anahtarın bu
> makinenin keychain'inde oluşmasını gerektiriyor. Bir betik oturum açamaz.

### 1.2 App ID ve provisioning profile

- [ ] Portalda **App ID** oluştur: `com.kutluhangil.quotadeck`
      (Bundle ID zaten `app/tauri.conf.json`'da bu; değiştirme.)
- [ ] Capabilities'te **App Sandbox**'ın açık olduğundan emin ol
- [ ] **Mac App Store** tipinde provisioning profile oluştur, indir ve şu ada kaydet:

```
app/MacAppStore.provisionprofile
```

Bu dosya `.gitignore`'da değil ama **commit etme** — hesap verisi. Eklemek istersen söyle,
ignore'a yazayım.

### 1.3 App Store Connect API anahtarı

- [ ] App Store Connect → Users and Access → Integrations → App Store Connect API
- [ ] Yeni anahtar, rol **App Manager**
- [ ] `.p8` dosyasını indir (bir kez indirilebiliyor) ve `~/.appstoreconnect/private_keys/`
      altına koy
- [ ] Key ID ve Issuer ID'yi not al

### 1.4 App Store Connect'te uygulama kaydı

- [ ] Yeni macOS uygulaması oluştur, Bundle ID olarak yukarıdakini seç
- [ ] **Ad:** `Quota Deck` — başka hiçbir şey. `docs/STORE.md` §1: başlığa üçüncü parti ürün
      adı koymak belgelenmiş bir ret sebebi ve CUStats bunu v1.8.0'da pahalıya öğrendi
- [ ] **Alt başlık:** `See your AI coding quotas`
- [ ] **Anahtar kelimeler:** `quota, usage, tokens, menu bar, developer tools`
      — marka adı yok
- [ ] **Açıklama:** `docs/STORE.md` §2'deki metin, olduğu gibi
- [ ] **Gizlilik anketi:** her soruda **Data Not Collected**. Gerekçeleri `docs/STORE.md` §3'te;
      her biri CI'da doğrulanıyor, yani beyan denetlenirse arkasında duracak bir şey var
- [ ] **Kategori:** Developer Tools

### 1.5 Ekran görüntüleri

`docs/STORE.md` §8 altı görüntü istiyor, hepsi örnek veriden. İkisi hazır:

- [x] Panel, iki araç bildiriyor — `site/public/panel.png` (EN), `site/public/panel-tr.png` (TR)
- [ ] Menü çubuğu öğesi, sakin hâlde — imzalı derleme çalışırken çekilecek
- [ ] Horizon şeridi, imleç bir dilimin üstünde
- [ ] Pano, hafta aralığı, heatmap görünür
- [ ] Ayarlar — güven açıklaması ve durum satırı öncesi/sonrası
- [ ] Eşik bildirimi

Hiçbirinde sağlayıcı logosu olmayacak (§1).

> Ben tarayıcıda panel ve pano görüntüsü üretebiliyorum; **menü çubuğu öğesi ve bildirim**
> gerçek uygulamanın çalışmasını istiyor. Uygulamayı bir kez ayağa kaldırdığında geri kalanını
> ben çekebilirim.

### 1.6 Yükleme

Hepsi hazırsa tek komut:

```
TEAM_ID=... APPLE_API_KEY=... APPLE_API_ISSUER=... scripts/appstore.sh
```

Betik yüklemeden **önce** iki şeyi doğruluyor: sandbox yetkilendirmesinin imzaya gerçekten
girdiğini (yokluğu Apple tarafında `Asset Validation error 90296` olarak görünüyor) ve hiçbir
ağ yeteneğinin sızmadığını. İkisi de yirmi dakikalık bir yüklemeden sonra öğrenmek istemeyeceğin
şeyler.

- [ ] İlk yükleme geçti
- [ ] TestFlight ya da App Review'a gönderildi

---

## 2. Microsoft Store

### 2.1 Hesap

- [ ] Partner Center geliştirici hesabı aç (bir kerelik ücret var)
- [ ] Ürün adını rezerve et: `Quota Deck`
- [ ] **Publisher display name ürün adıyla aynı olamaz** — Microsoft bunu reddediyor.
      Farklı bir yayıncı adı seç (ör. kendi adın)

### 2.2 Derleme

Bir Windows makinesinde:

```
$env:WINDOWS_CERTIFICATE_THUMBPRINT = "CA sertifikasının thumbprint değeri"
scripts/msstore.ps1
```

x64 ve arm64 için NSIS `.exe` kurucuları üretir. Microsoft'un paketlenmemiş Win32 akışında
kurucu doğrudan yüklenmez; imzalı dosya değişmez, sürümlü bir HTTPS adresinde barındırılır.
Kurucunun ve kurduğu bütün PE dosyalarının CA destekli Authenticode imzası gerekir. Betik geçerli
imza görmezse Store artefaktını reddeder; `-AllowUnsignedLocalBuild` yalnız yerel deneme içindir.

- [ ] Windows kod imzalama sertifikası Tauri için yapılandırıldı
- [ ] x64 ve arm64 `*-setup.exe` üretildi ve Authenticode durumu `Valid`
- [ ] Kurucular değişmez, sürümlü HTTPS adreslerine yüklendi
- [ ] Partner Center'da EXE türü, doğru mimari ve sessiz kurulum anahtarı `/S` girildi
- [ ] Listeleme metni §1'deki kurallarla, açıklama §2'den, gizlilik §3'ten

### 2.3 Windows çalışma zamanı

- [ ] Ayarlar → Oturum açınca başlat seçeneğini aç; Görev Yöneticisi → Başlangıç uygulamaları
      ve `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` içinde Quota Deck'i doğrula
- [ ] Seçeneği kapat; kayıt değerinin kaldırıldığını ve yeniden girişte açılmadığını doğrula
- [ ] Tepsi, panel odağı, tıklayınca gizleme ve `/S` sessiz kurulumu gerçek Windows'ta doğrula

> Neden ben yapamıyorum: NSIS, Authenticode ve tepsi akışı gerçek bir Windows oturumu istiyor.

---

## 3. Linux

Mağaza yok, hesap yok. Tek eksik gerçek bir masaüstü oturumu.

Bir Linux makinesinde (Debian/Ubuntu veya Fedora):

```
scripts/linux.sh
```

`.deb`, `.rpm` ve AppImage üretiyor. Derleme bağımlılıkları betiğin başında yazıyor.

Elle doğrulanacaklar — hepsi CI'ın göremediği şeyler:

- [ ] Tepsi öğesi **çiziliyor** (`libayatana-appindicator3-1` yoksa hiç çizilmiyor)
- [ ] Sol tık menüyü açıyor, menünün ilk girdisi paneli açıyor
- [ ] Panel sağ üste yerleşiyor (GNOME/Cinnamon/Budgie/XFCE). KDE'de gösterge sağ altta,
      panel oraya inmiyor — bu bilinen ve `docs/STORE.md` §7'de yazılı bir fark
- [ ] Kompozitör varken panelin köşeleri yuvarlak; kompozitörsüz bir WM'de opak'a düşüyor
- [ ] `.deb` ve `.rpm` bağımlılık beyanlarıyla temiz kuruluyor

- [ ] GitHub Releases'e üç dosyayı yükle ve bana söyle — `site/src/copy/*.ts` içindeki
      "henüz yayında değil" durumunu gerçek bağlantılara çeviririm

> Neden ben yapamıyorum: bu bir masaüstü oturumu istiyor. CI derliyor, test ediyor ve perf
> bütçesini koşuyor ama ekranı yok.

---

## 4. Site — alan adı ve yayın

- [ ] Alan adını al (§0.3)
- [ ] Vercel'de yeni proje, GitHub deposunu bağla
- [ ] **Root Directory: `site`** — bu ayar dashboard'da, dosyadan verilemiyor
- [ ] Framework otomatik `Astro` gelmeli; gelmezse elle seç
- [ ] Alan adını projeye bağla

`site/vercel.json` derleme komutunu, çıktı dizinini ve güvenlik başlıklarını zaten taşıyor.
CSP `default-src 'none'` — sayfanın uygulama hakkında söylediği şeyi sayfanın kendisi için de
söylüyor.

- [ ] İlk deploy'dan sonra sayfayı bir kez gerçek alan adında aç ve font'ların yüklendiğini
      doğrula (CSP `font-src 'self'`; kendi sunucumuzdan geliyorlar, ama bir kez bak)

---

## 5. Elle QA — makine değil insan gerektirenler

### 5.1 VoiceOver

Panel a11y için tasarlandı: her çubuk `role="meter"`, her satır kendi kaynağını söylüyor,
görsel olarak yazılan yüzde ekran okuyucudan gizli çünkü metre onu zaten sınırlarıyla birlikte
duyuruyor.

- [ ] ⌘F5 ile VoiceOver aç, paneli baştan sona gez
- [ ] Sekme sırası mantıklı mı
- [ ] Yüzde iki kez okunuyor mu (okunuyorsa bir hata var, bana söyle)
- [ ] Ayarlar'a geçince odak yeni ekrana taşınıyor mu

> Neden ben yapamıyorum: ekran okuyucunun gerçekte ne söylediğini duymak gerekiyor. Kod doğru
> görünebilir ve yine de kötü duyulabilir.

### 5.2 72 saat kesintisiz çalışma

Blueprint Ek A'nın kapatılmamış iki maddesi:

- [ ] İmzalı derlemeyi 72 saat sandbox içinde açık bırak
- [ ] Bitişte RSS başlangıcın %20'sinden fazla artmamış olmalı (sızıntı testi)

Ölçüm: `Activity Monitor` ya da

```
ps -o rss=,comm= -p $(pgrep -f "Quota Deck")
```

> Neden CI yapamıyor: CI koşuları dakikalarla ölçülüyor. Sızıntı günlerle ortaya çıkıyor.

### 5.3 Gerçek veriyle bakış

Örnek veri gerçekçi ama uydurma. Kendi gerçek `~/.claude` ve `~/.codex` günlüklerinle:

- [ ] Codex'in bildirdiği L1 yüzdesi, `codex` CLI'ın kendi söylediğiyle uyuşuyor mu
- [ ] Claude Code durum satırını bağla, tahminin ölçüme dönüştüğünü gör
- [ ] Bir eşik aş ve bildirimin geldiğini doğrula

---

## 6. Ürün kararları — acele yok ama açık duruyor

- [ ] **Sağlayıcı sayısı.** Blueprint 15+ hedefliyor, üçü yazıldı: Claude Code, Codex,
      Copilot CLI. Kalanların hepsi "makinede yok" diye bekliyor — biçimini gerçek bir dosyayla
      doğrulamadan sağlayıcı yazmıyoruz, çünkü tahmine dayalı bir ayrıştırıcı sessizce yanlış
      sayı üretir. Bir aracı kurup birkaç oturum çalıştırırsan o sağlayıcıyı yazabilirim.
- [ ] **DE / ES yerelleştirme.** Altyapı hazır; katalog eklemek bir dosya. Talep gelirse.
- [ ] **Kaçak ajan tespiti** (blueprint §6.3). v1.1 için ayrılmış, farklılaştırıcı olabilir.

---

## 7. Önerilen sıra

1. §0 — kalan iki kararı ver (fiyat, alan adı); sürüm `1.0.0` olarak kapatıldı
2. §1.1–1.3 — Apple sertifikaları ve anahtarı. En uzun kuyruk burada
3. §5.3 — gerçek veriyle bir kez bak. Bir hata varsa mağazaya gitmeden önce çıksın
4. §1.4–1.6 — App Store Connect ve ilk yükleme
5. §4 — site yayına (App Review beklerken yapılacak iş)
6. §5.1, §5.2 — VoiceOver ve 72 saat, review kuyruğundayken
7. §2, §3 — Windows ve Linux, macOS onaylandıktan sonra

---

## Bana söylemen yeterli olanlar

Bunların hiçbiri senin işin değil; bir cümlelik bilgi bekliyorlar:

| Ne olduğunda | Ne yaparım |
|---|---|
| Alan adını aldın | `astro.config.mjs` + `sitemap.txt` güncellenir |
| Fiyata karar verdin | `docs/STORE.md` §4 kesinleşir |
| Linux paketleri Releases'te | Site indirme sayfası gerçek bağlantılara döner |
| Uygulama bir kez ayağa kalktı | Kalan dört mağaza ekran görüntüsü çekilir |
| Yeni bir AI aracı kurdun | O sağlayıcı yazılır — bir dosya, bir fixture testi, bir kayıt satırı |
| VoiceOver'da bir şey ters | Düzeltilir |
