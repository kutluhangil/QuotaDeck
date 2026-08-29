/**
 * The Turkish catalogue.
 *
 * Typed as `Catalogue`, so a key added to `en.ts` and not translated here fails the build.
 * Product names — Quota Deck, Claude Code, Codex — stay in their original form.
 */

import type { HostPlatform } from "../platform";
import type { ProviderId } from "../types";
import type { Catalogue } from "./en";

/**
 * Bu masaüstünün tepsi öğesinin durduğu şeride verdiği ad. Cümlenin başında da geçtiği için
 * hepsi ek almadan, yalın hâlde yazıldı.
 */
const surfaces: Record<HostPlatform, string> = {
  macos: "Menü çubuğu",
  windows: "Görev çubuğu",
  linux: "Tepsi",
};

const planHintDefault =
  "Limitlerinin ne kadar dolduğunu tahmin etmek için kullanılır. Bunu hesaplamak için hiçbir yere bir şey gönderilmez.";

const planHints: Partial<Record<ProviderId, string>> = {
  "claude-code":
    "Limitlerinin ne kadar dolduğunu tahmin etmek için kullanılır. Anthropic bunlar için bir sayı yayımlamıyor; aşağıdaki durum satırını bağlayana kadar bu bir tahmindir.",
  "copilot-cli":
    "GitHub her planın kredi hakkını yayımlıyor, dolayısıyla tavan kesin. Tahmin harcama tarafında: burada yalnızca komut satırı oturumları sayılır, o da ancak bittikten sonra. Editöründe veya web'de harcanan krediler bu makineden görünmez.",
};

export const tr: Catalogue = {
  appName: "Quota Deck",
  shellFailed: (reason) => `Uygulama bir işlemi tamamlayamadı: ${reason}`,
  refreshFailed: (reason) => `Kullanım yenilenemedi: ${reason}`,

  header: {
    settings: "Ayarlar",
  },

  footer: {
    updated: (time) => `${time} güncellendi`,
    reporting: (reporting, total) =>
      total === 0 ? "" : `${total} araçtan ${reporting} tanesi bildiriyor`,
    dashboard: "Pano",
    refresh: "Yenile",
    quit: "Çık",
  },

  status: {
    ample: "İyi",
    tight: "Dikkat",
    critical: "Kritik",
  },

  health: {
    rebuilding: (_reason) =>
      "Kullanım güncel fiyat tablosuyla yeniden oluşturuluyor. Kısmi sonuçlar henüz kesin değil.",
    stale: (reason) =>
      `Son yenileme başarısız oldu. Son başarılı ölçüm gösteriliyor.${reason ? ` ${reason}` : ""}`,
    error: (reason) =>
      `Kullanım yenilenemedi.${reason ? ` ${reason}` : " Yenilemeyi tekrar dene."}`,
    unavailable: (reason) =>
      `Bu araç şu anda kullanılamıyor.${reason ? ` ${reason}` : ""}`,
  },

  filters: {
    all: "Tümü",
    label: "Tek araç göster",
  },

  units: {
    day: "g",
    hour: "sa",
    minute: "dk",
  },

  relative: {
    justNow: "az önce",
    ago: (duration) => `${duration} önce`,
  },

  confidence: {
    measured: "ölçüldü",
    estimated: "tahmin",
    idle: "boşta",
  },

  window: {
    session: "oturum",
    weekly: "haftalık",
    monthly: "aylık",
    other: (minutes) => `${minutes} dk`,
  },

  strip: {
    now: "şimdi",
    tokens: (tokens) => `${tokens} token`,
    quiet: "kullanım yok",
    summary: (duration, tokens) => `Son ${duration} içindeki kullanım: ${tokens} token`,
  },

  pace: {
    projected: (percent) => `bu hızla ${percent}`,
    /** A label rather than a sentence, so the clock format stays out of the grammar. */
    exhausted: (clock, duration) => `Dolma ${clock} · ${duration}`,
    risk: {
      healthy: "yolunda",
      "at-risk": "riskli",
      over: "aşıyor",
    },
    label: (percent) => `Öngörülen ${percent}`,
    rowLabel: "Hız",
  },

  card: {
    resetsAt: (time) => `${time} sıfırlanır`,
    resetsIn: (duration) => `${duration} sonra boşalır`,
    noReset: "Sıfırlanma zamanı bildirilmedi",
    todayTokens: (tokens) => `bugün ${tokens} token`,
    todayCost: (amount) => `bugün ${amount}`,
    costPartial: (tokens) => `artı fiyatı bilinmeyen ${tokens} token`,
    lastActivity: (when) => `Son kullanım ${when}`,
    neverUsed: "Henüz kullanım kaydı yok",
    pickPlan: "Tahmin görmek için planını seç",
    pickPlanAction: "Plan seç",
    limitLabel: (window) => `${window} limiti`,
  },

  quiet: {
    /** Turkish does not mark the plural after a numeral, so one form serves every count. */
    heading: (count) => `${count} araç kurulu değil`,
  },

  dashboard: {
    title: "Quota Deck",
    rangeLabel: "Aralık",
    range: { day: "Gün", week: "Hafta", month: "Ay", quarter: "Çeyrek", year: "Yıl" },
    rangeSpan: (days) => (days === 1 ? "Son 24 saat" : `Son ${days} gün`),
    rangeTokens: "Token",
    rangeCost: "Eşdeğer maliyet",
    retention: (days) => `Bu cihazda ${days} günlük geçmiş tutuluyor`,
    unpriced: (tokens) => `${tokens} token'ın fiyatı bilinmiyordu`,
    heatmapLabel: "Son bir aydaki günlük etkinlik",
    heatmapQuiet: "sakin",
    heatmapBusy: "yoğun",
    customRange: "Tarihler",
    rangeFrom: "Başlangıç",
    rangeTo: "Bitiş",
    hourlyHistory: "Saatlik geçmiş; kayıtlar başlangıç saatine göre alınır.",
    copyJson: "JSON kopyala",
    copyCsv: "CSV kopyala",
    exporting: "Hazırlanıyor…",
    copied: (format, rows) => `${format} kopyalandı: ${rows} satır`,
    exportFailed: (reason) => `Geçmiş dışa aktarılamadı: ${reason}`,
    exportClamped: (from, to) =>
      `Kopyalanan saatlik geçmiş ${from} ile ${to} arasında; seçilen başlangıç bu cihazda tutulan geçmişten daha eski.`,
    exportUnavailable: "Geçerli kullanım geçmişi tamamlanana kadar dışa aktarma kullanılamaz.",
    rebuilding: (from, to) =>
      `${to} günlük geçmiş yerel günlüklerden yeniden oluşturulurken tam ${from} günlük geçmiş korunuyor.`,
    rebuildFailed: (reason) => `Geçmiş yeniden oluşturma işlemi bekliyor: ${reason}`,
  },

  breakdown: {
    models: "Nereye harcandı",
    projects: "Hangi dizinde harcandı",
    unreported: "Model bildirilmedi",
    unattributed: "Dizin bildirilmedi",
    empty: "Bu aralıkta sayılan bir şey yok",
    dropped: (count) => `${count} kayıt ilişkilendirilemedi — ayrı model sayısı çok fazla`,
    droppedProjects: (count) =>
      `${count} kayıt ilişkilendirilemedi — ayrı dizin sayısı çok fazla`,
    // `percent` zaten biçimlenmiş geliyor (`%42`); ikinci bir yüzde işareti eklenmez. Ek uyumu
    // sayının okunuşuna bağlı olduğu için "kadarı" ile bağlanıyor, kesme işaretiyle değil.
    share: (label, percent) => `${label} — bu aralığın ${percent} kadarı`,
    listLabel: (tool) => `${tool} bu aralıkta neye harcadı, model bazında`,
    projectListLabel: (tool) => `${tool} bu aralıkta neye harcadı, dizin bazında`,
    agents: "Kim harcadı",
    origin: {
      main: "Ana konuşma",
      subagent: "Alt agent'lar",
      workflow: "Workflow agent'ları",
    },
    droppedAgents: (count) => `${count} kayıt ilişkilendirilemedi — ayrı agent türü çok fazla`,
    agentListLabel: (tool) => `${tool} bu aralıkta neye harcadı, iş türü bazında`,
  },

  burst: {
    label: "Agent'lar",
    meta: (factor) => `olağan saatin ${factor} katı`,
    detail: (tokens, factor) =>
      `Agent'lar son bir saatte ${tokens} token harcadı — senin olağan bir saatinin yaklaşık ${factor} katı.`,
  },

  empty: {
    noTools: {
      title: "Desteklenen araç bulunamadı",
      body: "Quota Deck, kodlama araçlarının zaten yazdığı oturum günlüklerini okur. Claude Code, Codex ya da desteklenen başka bir aracı kur; burada görünsün.",
      action: "Desteklenen araçlar",
    },
    providersDisabled: {
      title: "Tüm araçlar gizli",
      body: "Yerel günlükleri yeniden okumak için Ayarlar'da en az bir aracı etkinleştir.",
    },
    noPermission: {
      title: "Klasör erişimi gerekiyor",
      body: "Quota Deck'in ev klasöründeki oturum günlüklerini okuması gerekiyor. Bunu bir kez verirsin ve hiçbir şey bu cihazdan çıkmaz.",
      action: "Klasör seç",
    },
    demoAction: "Örnek veriyle gör",
    scanning: "Oturum günlükleri okunuyor…",
  },

  unavailable: {
    "not-installed": "Kurulu değil",
    "no-logs-found": "Henüz oturum günlüğü yok",
    "permission-denied": "Bu klasöre erişim yok",
    "read-error": "Bir oturum günlüğü okunamadı",
    "never-reported": "Bu araç bir limit bildirmedi",
  },

  provider: {
    "claude-code": "Claude Code",
    codex: "Codex",
    "copilot-cli": "Copilot CLI",
  },

  settings: {
    title: "Ayarlar",
    trayTitle: (platform: HostPlatform) => surfaces[platform],
    trayGlyph: "Simge",
    trayGlyphHint: "Tek bir çubuk. Gerekmedikçe ne sayı ne renk.",
    trayCompact: "Yüzde",
    trayCompactHint: "Bildirilen en yüksek kullanım, sayı olarak.",
    trayStrip: "Ufuk",
    trayStripHint: "Panelin zaman şeridinin küçük hâli.",
    themeTitle: "Görünüm",
    themeSystem: "Sistemle aynı",
    themeDark: "Koyu",
    themeLight: "Açık",
    retentionTitle: "Bu cihazda tutulan geçmiş",
    retentionDays: (days) => `${days} gün`,
    retentionHint: "Daha uzun bir dönem seçmek yalnız yerel günlükleri yeniden okur. Son tam geçmiş bitene kadar kullanılabilir kalır.",
    retentionRebuilding: (from, to) =>
      `${to} günlük geçmiş yerel günlüklerden yeniden oluşturulurken tam ${from} günlük geçmiş korunuyor.`,
    back: "Bitti",
    settingsFailed: (reason) => `Ayarlar kaydedilemedi: ${reason}`,
    providersTitle: "Araçlar",
    providersHint: "Devre dışı araçlar okunmaz, izlenmez, uyarı üretmez ve dışa aktarılmaz.",
    providerEnabled: (provider) => `${provider} günlüklerini oku`,
    providerUp: (provider) => `${provider} aracını yukarı taşı`,
    providerDown: (provider) => `${provider} aracını aşağı taşı`,

    rootsTitle: (provider) => `${provider} · ek günlük klasörleri`,
    rootsHint:
      "Bu aracın kendi günlükleriyle aynı kotaya katılır. İkinci bir aboneliğin değil, ortak diskteki ikinci bir makinenin günlükleri için.",
    rootsUnsupported:
      "Bu sürüm ev klasörünü tek bir izinle okuyor ve ikincisini açamıyor; ek klasörler burada kullanılamaz.",
    rootsPlaceholder: "Klasörün tam yolu",
    rootsAdd: "Klasör ekle",
    rootsRemove: (path) => `${path} klasörünü kaldır`,
    rootsEmpty: "Ek klasör yok.",
    rootsInvalidEmpty: "Önce klasörün tam yolunu yaz.",
    rootsInvalidRelative:
      "Tam yolu yaz. Neye göre olduğu belli olmayan bir yol, her açılışta başka bir klasörü gösterir.",
    rootsInvalidDuplicate: "Bu klasör zaten listede.",
    rootsInvalidTooMany: (limit) => `Araç başına en fazla ${limit} ek klasör.`,

    instancesTitle: "Ayrı hesaplar",
    instancesHint:
      "Aynı aracın ikinci girişinin kendi limiti vardır. Ayrı takip etmek için ekle; kendi planı, eşikleri ve geçmişi olur. Yalnızca aşağıda verdiğin günlük klasörlerini okur — birinciden hiçbir şey kopyalanmaz, onunla hiçbir şey paylaşılmaz.",
    instancesEmpty: "Ayrı hesap yok.",
    instancesTool: "Araç",
    instancesNamePlaceholder: "kisa-ad",
    instancesLabelPlaceholder: "Kartta görünecek ad",
    instancesAdd: "Hesap ekle",
    instancesRemove: (name) => `${name} hesabını kaldır`,
    instancesInvalidName:
      "Küçük harf, rakam ve tire kullan — bu, hesabın saklandığı anahtar olur.",

    languageTitle: "Dil",
    languageSystem: "Sistemle aynı",
    languageEnglish: "English",
    languageTurkish: "Türkçe",
    languageHint: "Tarih ve saat biçimleri sistemin bölge ayarlarını izlemeye devam eder.",

    startupTitle: "Oturum açılışına kaydet",
    startupOn: "Kaydı ekle",
    startupOff: "Kaydı kaldır",
    startupHint: "Bu ayar Windows başlangıç kaydını yönetir. Windows Ayarları veya Görev Yöneticisi bu kaydı ayrıca devre dışı bırakabilir.",
    startupFailed: (reason) => `Windows başlangıç ayarı değiştirilemedi: ${reason}`,

    planTitle: (provider) => `${provider} planı`,
    planHint: (provider) => planHints[provider] ?? planHintDefault,
    planNone: "Seçili değil",
    planNoneHint: "Tahmin gösterilmez. Hiçbir şey varsayılmaz.",

    alertsTitle: "Şu düzeylerde uyar",
    alertsHint:
      "Bir limit bunlardan birini geçtiğinde bildirim gelir; limit başına pencere başına bir kez. macOS ilkinden önce izin ister.",
    alertsThreshold: (percent) => `%${percent}`,
    alertsOff: "Bu araç için uyarı yok",
    muteTitle: "Sessiz",
    muteHour: "Bir saatliğine",
    muteToday: "Yarına kadar",
    muteClear: "Uyarıları geri aç",
    mutedUntil: (time) => `${time} saatine kadar sessiz`,

    statuslineTitle: "Ölçülmüş limitler",
    statuslineBody:
      "Claude Code, 5 saatlik ve haftalık limitlerinin gerçek yüzdesini kendi durum satırına veriyor. Quota Deck bağlantıyı incelemek için statusLine.command alanını okur. Her iki kurulum akışında da nesne değişmeden veya senden değiştirmen istenmeden önce, tam geri alma için önceki statusLine değerini kendi yerel veri klasöründe saklar. Bu cihazdan hiçbir şey çıkmaz.",
    statuslineUnsupported: "Claude Code durum satırı ayarı henüz incelenemiyor.",
    statuslineConnect: "Durum satırını bağla",
    statuslineConnecting: "Bağlanıyor…",
    statuslineRevert: "Bağlantıyı kes",
    statuslineReverting: "Bağlantı kesiliyor…",
    statuslineInstalled: "Bağlı",
    statuslineFile: (path) => `${path} dosyasını düzenler`,
    statuslineBefore: "Şimdi",
    statuslineAfter: "Bağladıktan sonra",
    statuslineNoPrevious:
      "Ayarlanmış bir durum satırın yok. Bağlantıyı kesmek ayarı tekrar kaldırır.",
    statuslineChains:
      "Mevcut durum satırın çalışmaya devam eder — bizimki onun çıktısını olduğu gibi geçirir.",
    statuslineManualNotice:
      "App Store sürümü Claude Code ayarlarını yalnızca okuyabilir. Quota Deck bu dosyayı değiştirmez.",
    statuslineManualInstruction:
      "Üst düzey statusLine değerini aşağıdaki tam JSON nesnesiyle değiştir. Zorunlu type alanını içerir ve diğer statusLine alanlarını korur.",
    statuslineManualRestore:
      "Bağlantıyı kaldırmak için statusLine.command değerini aşağıdaki eski komuta geri getir.",
    statuslineManualRestoreObject:
      "Bağlantıyı kaldırmak için üst düzey statusLine değerini aşağıdaki özgün JSON nesnesiyle değiştir.",
    statuslineManualRemove:
      "Bağlantıyı kaldırmak için statusLine alanını ayar dosyasından kaldır.",
    statuslineManualRemoveCommand:
      "Bağlantıyı kaldırmak için yalnız statusLine.command alanını kaldır ve diğer statusLine alanlarını koru.",
    statuslineCopyCommand: "statusLine JSON'unu kopyala",
    statuslineCopyPrevious: "Eski komutu kopyala",
    statuslineCopyPreviousObject: "Önceki statusLine JSON'unu kopyala",
    statuslineCopied: "Komut kopyalandı",
    statuslineCopyFailed: (reason) => `Komut kopyalanamadı: ${reason}`,
    statuslineRefresh: "Durumu yeniden kontrol et",
    statuslineRefreshing: "Kontrol ediliyor…",
    statuslineWaiting:
      "Henüz okuma yok. Claude Code bunu yalnızca etkileşimli bir oturumda, ilk yanıtından sonra gönderir.",
    statuslineReadings: (count, when) => `${count} okuma, sonuncusu ${when}`,
    statuslineFailed: (reason) => `Ayar değiştirilemedi: ${reason}`,

    accessTitle: "Klasör erişimi",
    accessGranted: (path) => `${path} okunuyor`,
    accessMissing: "Henüz bir klasör seçilmedi.",
    accessChoose: "Klasör seç",
    accessRevoke: "Erişimi geri al",
    accessFailed: (reason) => `Kayıtlı izin kullanılamadı: ${reason}`,
    accessHint:
      "Salt okunur oturum günlükleri ve isteğe bağlı bağlantı için Claude ayarlarındaki statusLine.command alanı. Sağlayıcı kimlik dosyaları hiç açılmaz. Geri alma anında geçerli olur.",

    demoTitle: "Örnek veri",
    demoOn: "Örnek göster",
    demoOff: "Kendi makinemi göster",
    demoHint: (platform: HostPlatform) =>
      `Gerçekçi ama uydurma sayılar; hiçbir araç kurulmadan uygulamanın nasıl çalıştığı görülebilsin. ${surfaces[platform]} gerçek kullanımını bildirmeye devam eder.`,
  },

  a11y: {
    tools: "İzlenen araçlar",
    settingsRegion: "Ayarlar",
    status: "Durum",
    panelActions: "Panel işlemleri",
    footerActions: "İşlemler",
    windows: (provider) => `${provider} limitleri`,
    source: (source) => `Kaynak: ${source}`,
  },
};
