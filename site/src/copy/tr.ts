/**
 * Türkçe metin.
 *
 * `Copy` olarak tiplenmiş, yani `en.ts`'e eklenip buraya çevrilmeyen bir anahtar derlemeyi
 * kırar. Ürün adları — Quota Deck, Claude Code, Codex — özgün hâllerinde kalır.
 */

import type { Copy } from "./en";

export const tr: Copy = {
  lang: "tr",
  locale: "tr",

  meta: {
    title: "Quota Deck — AI kodlama kotalarını gör",
    description:
      "Kodlama araçlarının zaten diske yazdığı oturum günlüklerini okuyan, her kayan pencerenin ne kadarını harcadığını gösteren bir menü çubuğu uygulaması. Hesap yok, giriş yok, ağ isteği yok.",
  },

  nav: {
    home: "Quota Deck",
    download: "İndir",
    privacy: "Gizlilik",
    skip: "İçeriğe geç",
    language: { label: "Dil", en: "English", tr: "Türkçe", enShort: "EN", trShort: "TR" },
    theme: { label: "Tema", system: "Sistemi izle", light: "Açık", dark: "Koyu" },
  },

  hero: {
    title: "AI kodlama kotalarını gör.",
    lede: "Quota Deck, kodlama araçlarının zaten kendi diskine yazdığı oturum günlüklerini okur ve her kayan pencerenin ne kadarını harcadığını gösterir — araç cevap vermeyi kesmeden önce.",
    claim: "Hesap yok. Giriş yok. Ağ yok. Hiçbir zaman.",
    claimNote:
      "Uygulama, dışarı bağlantıya izin verecek yetkilendirme olmadan derleniyor; yani bu satırı bu sayfadaki bir iddia değil, kodun imzası garanti ediyor.",
    download: "İndir",
    how: "Nasıl doğru olabiliyor",
    shot: {
      src: "/panel-tr.png",
      alt: "Quota Deck paneli: iki araç, her birinde beş saatlik, haftalık ve hız satırları.",
      caption: "Panel, örnek veriyle. Bu görüntüde gerçek bir kullanım ya da yol yok.",
    },
  },

  features: {
    title: "Ne yapar",
    items: [
      {
        title: "Gerekene kadar sessiz",
        body: "Menü çubuğundaki öğe tek renkli bir glif. Ancak %85'in üstünde renk alıyor — sürekli yanan bir öğe, insanların menü çubuğu uygulamalarını silme sebebi.",
      },
      {
        title: "Her limit bir satır",
        body: "Bir araç aynı anda birkaç pencere bildirebilir: beş saat, bir hafta, bir ay. Her biri aynı dört sütunlu satırı alıyor, çünkü haftalık bir tavan işi tam olarak beş saatlik kadar durdurur.",
      },
      {
        title: "Ufuk",
        body: "Kota bir pil değil, bir gelgittir. Kullanım sağdan girer, sola kayar ve kota geri gelirken pencerenin kenarından düşer. Araç başına tek bir şerit; hem kotanın nereye gittiğini gösteriyor hem de kayan pencere modelini öğretiyor.",
      },
      {
        title: "Ne olduğunu söyleyen tahmin",
        body: "Hız satırı içi boş bir çerçeveyle çiziliyor ve bir öngörü olarak etiketleniyor, asla bir okuma olarak değil. Tahmini ölçümmüş gibi göstermek, bu kategoride güveni kaybetmenin en hızlı yolu.",
      },
      {
        title: "Senin seçtiğin uyarılar",
        body: "Bir limit senin belirlediğin düzeyi geçtiğinde bildirim; limit başına pencere başına bir kez, bir saatliğine ya da yarına kadar susturulabilir.",
      },
      {
        title: "Bir aylık geçmiş, burada",
        body: "Saatlik toplamlar ve eşdeğer API maliyeti; bu cihazda bir ay tutuluyor, başka hiçbir yerde değil.",
      },
    ],
  },

  trust: {
    title: "Ölçülmüş ya da açıkça tahmin",
    body: "Bazı araçlar kalan yüzdelerini kendi günlüklerine yazıyor, bazıları yazmıyor. Panel bu ikisini asla birbirine karıştırmıyor: dolu mavi işaret sayıyı aracın kendisinin bildirdiği anlamına geliyor, içi boş halka ise token sayımından ve senin seçtiğin plandan hesaplandığı. Hiçbir şey sessizce varsayılmıyor — planı seçilmemiş bir araç hiç yüzde göstermiyor.",
    measured: "ölçüldü — araç bunu kendi bildirdi",
    estimated: "tahmin — bu makinede hesaplandı",
    anatomy: {
      title: "Bir satır, dört sütun",
      lede: "Bir araç aynı anda birkaç pencere bildirebilir. Her biri aynı biçimde bir satır alır, çünkü haftalık bir tavan işi tam olarak beş saatlik biri kadar sert durduruyor.",
      sample: {
        tool: "Codex",
        window: "5 saatlik pencere",
        percent: "%78",
        countdown: "2sa 14dk",
        caption: "Örnek değerler. Rengi seviyeden yalnızca çubuk alıyor, satırdaki başka hiçbir şey almıyor.",
      },
      columns: [
        { label: "Kaynak", body: "Dolu: bu sayıyı aracın kendisi bildirdi. İçi boş: burada hesaplandı." },
        { label: "Seviye", body: "Pencerenin ne kadarı dolu. Rengin bir şey ifade etmesine izin verilen tek yer." },
        { label: "Harcanan", body: "Aynı değer sayıyla. Tablo rakamlarıyla dizildi, yani ilerlerken satırı kaydıramaz." },
        { label: "Sıfırlanma", body: "Pencerenin işin sürmesine yetecek kadar kayacağı an." },
      ],
    },
  },

  privacy: {
    title: "Nasıl doğru olabiliyor",
    lede: "Gizlilik iddiası bir politika değil. Derlemeye ait dört olgu ve her biri doğru olmayı bıraktığı anda CI kırmızıya dönüyor.",
    items: [
      {
        title: "Bağımlılık ağacında HTTP istemcisi yok",
        body: "Ne reqwest, ne hyper, ne ureq. CI her push'ta bağımlılık ağacını tarıyor ve biri belirirse derlemeyi düşürüyor.",
      },
      {
        title: "Dışarı bağlantı yetkilendirmesi yok",
        body: "macOS yetkilendirme dosyasında com.apple.security.network.client yok. İddiayı hiçbir yetkilendirmenin desteklemediği Linux'ta ise paketleme betiği bağımlılık ağacını denetliyor.",
      },
      {
        title: "Kimlik bilgileri hiç okunmuyor",
        body: "Keychain ve Windows Credential Manager hiç açılmıyor. Sağlayıcı kimlik dosyaları hiç açılmıyor, listelenmiyor, varlığı bile yoklanmıyor — CI bunu da tarıyor.",
      },
      {
        title: "Okumalar salt okunur",
        body: "Yalnızca oturum ve telemetri günlükleri; hiçbirine yazılmıyor. App Store sürümü ev klasörü erişimini salt okunur tutuyor; isteğe bağlı Claude Code durum satırı için elle uygulayacağın tam zincirli komutu gösterip kopyalıyor.",
      },
    ],
    why: {
      title: "Neden böyle kuruldu",
      body: "Akla ilk gelen mimari — sağlayıcının OAuth token'ını Keychain'den okuyup kullanım API'sine sormak — dokunduğu her sağlayıcının tüketici şartlarını ihlal ediyor. Anthropic tam olarak bunu Ocak 2026'da uygulamaya soktu ve üçüncü parti araçlar bir gecede çalışmayı kesti. O yaklaşımın riski geliştiriciye değil, uygulamaya para veren insanların hesaplarına biniyor. Yerel günlük dosyaları ise kullanıcının kendi diskindeki kendi verisi ve atılacak bir istek yok.",
    },
  },

  performance: {
    title: "Bütçe bir özelliktir",
    lede: "Bu kategorinin batarya tüketimiyle anılan bir geçmişi var; o yüzden sınırlar umut edilmiyor, CI'da doğrulanıyor. Kırmızı bir bütçe birleştirmeyi engelliyor.",
    columns: { metric: "Ne", budget: "Bütçe", measured: "Ölçülen" },
    note: "500 dosyalık 160 MB'lık sentetik bir külliyat üzerinde ölçüldü. Bellek rakamı okuyucunun kendi tepe değeri, uygulamanın tamamı değil — bir Tauri uygulaması sistem WebView'ını da taşıyor ve 60 MB'lık tavan onun için.",
    rows: [
      { metric: "Soğuk ayrıştırma, 160 MB günlük", budget: "3 sn", measured: "65 ms" },
      { metric: "500 açık imleç üzerinde bir tur", budget: "20 ms", measured: "3 ms" },
      { metric: "Saatlik disk okuma", budget: "5 MB", measured: "65 KB" },
      { metric: "Tepe bellek kullanımı", budget: "60 MB", measured: "7,3 MB" },
      { metric: "Ağ, her zaman", budget: "0 bayt", measured: "0 bayt" },
    ],
  },

  providers: {
    title: "Okuduğu araçlar",
    lede: "Kurulu olmayan bir araç tam olarak bunu bildiriyor. Onun için hiçbir şey uydurulmuyor ve limit bildirmemiş bir araca yüzde icat edilmiyor.",
    shipping: "Şu an okunuyor",
    plannedNote:
      "Her yeni araç bir dosya, bir fixture testi ve bir satır kayıt demek. Buraya ancak günlük biçimi gerçek bir dosyayla doğrulandıktan sonra yazılıyorlar.",
    list: [
      { name: "Claude Code", detail: "Durum satırı bağlandığında ölçülmüş; öncesinde tahmin." },
      { name: "Codex", detail: "Ölçülmüş. Codex gerçek limit verisini kendi günlüğüne yazıyor." },
      { name: "Copilot CLI", detail: "Tavan, yayımlanan plan hakkından kesin; harcama tarafı yalnızca komut satırı oturumları." },
    ],
  },

  platforms: {
    title: "Üç masaüstü",
    items: [
      {
        name: "macOS",
        body: "Dock ikonu olmayan bir menü çubuğu öğesi. Mac App Store için sandbox içinde, yani bir kez sorulan tek bir klasör izni — o da sistemin kendi penceresinde.",
      },
      {
        name: "Windows",
        body: "Görev çubuğu tepsi öğesi. Microsoft Store üzerinden MSIX; imzayı mağaza atıyor, güncellemeler de mağazadan geliyor.",
      },
      {
        name: "Linux",
        body: "Tepsi göstergesi; .deb, .rpm ve AppImage olarak. Flathub ya da Snap yok: ikisi de burada bir şey satın almıyor, çünkü beyan edilecek bir sandbox izni ve gerekçelendirilecek bir ağ yeteneği yok.",
      },
    ],
    caveat: {
      title: "Linux'ta farklı olan ne",
      body: "Linux tepsisinin arkasındaki protokol ne tıklama olayı ne de ikon konumu taşıyor. Bu yüzden sol tık menüyü açıyor, menünün kendi girdisi paneli açıyor ve panel, konumunu kimsenin bildirmediği bir öğenin altına değil sağ üste yerleşiyor. Tepsisi varsayılan olarak sağ altta duran KDE'de panel onu takip etmiyor.",
    },
  },

  faq: {
    title: "Sorular",
    items: [
      {
        q: "API anahtarım gerekiyor mu?",
        a: "Hayır. Girilecek bir yer de yok, kullanılabileceği bir istek de.",
      },
      {
        q: "Sağlayıcı hesabım askıya alınır mı?",
        a: "Hayır — o risk, senin adına bir sağlayıcının API'sine kimlik doğrulayan araçlardan geliyor. Bu uygulama hiç kimlik doğrulamıyor ve hiç bağlanmıyor. Kendi araçlarının kendi diskine yazdığı dosyaları okuyor.",
      },
      {
        q: "macOS neden ev klasörümü istiyor?",
        a: "Çünkü sandbox içindeki bir uygulama, sen bir şey vermeden kendi kabının dışında hiçbir şeyi okuyamaz. Bir kez sorulan tek bir izin, o da sistemin kendi penceresinde, ve istediğin an Ayarlar'dan geri alınabiliyor. Her iki durumda da cihazdan hiçbir şey çıkmıyor.",
      },
      {
        q: "Bir sayı neden bazen tahmin?",
        a: "Çünkü her araç kalan kotasını yayımlamıyor. Yayımlayanda panel ölçümü gösteriyor. Yayımlamayanda tahmini gösteriyor ve aynı satırda bunu söylüyor. Birini diğerinin kılığında asla göstermiyor.",
      },
      {
        q: "Abonelik var mı?",
        a: "Yok. Tek seferlik satın alma. Zaten ödediğin şeyi sana söyleyen bir araç ayda bir fatura kesmemeli.",
      },
      {
        q: "Örnek veri gerçek mi?",
        a: "Değil ve uygulama açıkken bunu söylüyor. Desteklenen hiçbir aracın kurulu olmadığı bir makinede uygulamanın çalıştığının görülebilmesi için var — boş bir panel bozuk bir panelden ayırt edilemez. Menü çubuğu bu süre boyunca gerçek kullanımını bildirmeye devam ediyor.",
      },
    ],
  },

  download: {
    title: "İndir",
    lede: "Henüz hiçbir sürüm yayında değil. Üç derleme yolu da hazır ve CI'da çalışıyor; kalan kısım geliştirici hesabı ve her türden bir makine istiyor, ikisi de bir commit'in üretebileceği şeyler değil.",
    statusPending: "Henüz yayında değil",
    copyCommand: "Kopyala",
    copiedCommand: "Kopyalandı",
    build: "Kendin derle",
    buildLede:
      "Depo üçünde de derleniyor. Tek önkoşul Rust ve Node; Linux derlemesi ayrıca WebKitGTK ve AppIndicator geliştirme paketlerini istiyor, betik hangileri olduğunu yazıyor.",
    items: [
      {
        name: "macOS",
        detail: "Mac App Store, imzalı bir .pkg olarak. Sandbox içinde, klasör izni bir kez soruluyor.",
        blocked: "Bekleyen: Apple Distribution sertifikası ve App Store Connect gönderimi.",
        command: "TEAM_ID=... scripts/appstore.sh",
      },
      {
        name: "Windows",
        detail: "Microsoft Store, x64 ve arm64 için MSIX paketi. İmzayı mağaza attığı için sertifika satın alınması gerekmiyor.",
        blocked: "Bekleyen: Partner Center hesabı ve rezerve edilmiş bir ürün adı.",
        command: "scripts/msstore.ps1",
      },
      {
        name: "Linux",
        detail: "İki paket yöneticisi ailesi için .deb ve .rpm, artı ikisinin de kapsamadığı dağıtımlarda root olmadan çalışan bir AppImage.",
        blocked: "Bekleyen: gerçek bir masaüstü oturumunda elle çalıştırma. CI her push'ta derliyor ve test ediyor.",
        command: "scripts/linux.sh",
      },
    ],
    sourceTitle: "Kaynak",
    sourceBody: "Depo; her şeyin üzerine kurulduğu blueprint dâhil.",
    sourceLink: "github.com/kutluhangil/QuotaDeck",
    sourceHref: "https://github.com/kutluhangil/QuotaDeck",
  },

  privacyPage: {
    title: "Gizlilik",
    lede: "Quota Deck hiçbir şey toplamıyor, çünkü içinde toplayabilecek bir şey yok. Bu sayfa o cümlenin uzun hâli.",
    sections: [
      {
        title: "Cihazından ne çıkıyor",
        body: "Hiçbir şey. Bağımlılık ağacında HTTP istemcisi ve dışarı bağlantıya izin verecek bir yetkilendirme yok; yani bu bir niyet sözü değil, ikili dosyanın bir özelliği — ve ikisinden biri değişirse CI derlemeyi düşürüyor.",
      },
      {
        title: "Ne okunuyor",
        body: "Desteklenen araçların ev klasörüne yazdığı oturum ve telemetri günlükleri salt okunur ve baştan değil bir bayt konumundan itibaren okunuyor. İsteğe bağlı Claude Code durum satırı bağlantısını incelemek için ~/.claude/settings.json da açılıyor ve statusLine.command kullanılıyor. Her iki kurulum akışında da nesne değişmeden veya senden değiştirmen istenmeden önce, tam geri alma için önceki statusLine değeri Quota Deck'in yerel veri klasöründe saklanıyor.",
      },
      {
        title: "Ne asla okunmuyor",
        body: "Keychain. Windows Credential Manager. Herhangi bir sağlayıcı kimlik dosyası — auth.json, credentials.json, .credentials — hiç açılmıyor, hiç listelenmiyor, varlığı bile yoklanmıyor.",
      },
      {
        title: "Ne yazılıyor",
        body: "Saatlik kullanım toplamları, ayarların ve — isteğe bağlı bağlantıyı kurmayı seçtikten sonra — önceki statusLine değerinin tamamı uygulamanın kendi veri klasörüne yazılıyor. App Store sürümü başka hiçbir yere yazmıyor; tam zincirli JSON'u ve geri alma nesnesini veriyor, Claude ayarını sen değiştiriyorsun.",
      },
      {
        title: "Analitik, çökme raporu, telemetri",
        body: "Üçü de yok. Gidebilecekleri bir uç nokta yok.",
      },
      {
        title: "Mağaza beyanı",
        body: "App Store Connect'in gizlilik anketindeki her soruda ve Partner Center'daki karşılığında: Veri Toplanmıyor.",
      },
    ],
  },

  footer: {
    tagline: "Yerel, çevrimdışı ve bu konuda sessiz.",
    source: "Kaynak",
    privacy: "Gizlilik",
  },
};
