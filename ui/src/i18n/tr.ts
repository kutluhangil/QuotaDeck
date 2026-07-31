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

  header: {
    settings: "Ayarlar",
    expand: "Panoyu aç",
  },

  footer: {
    updated: (time) => `${time} güncellendi`,
    reporting: (reporting, total) =>
      total === 0 ? "" : `${total} araçtan ${reporting} tanesi bildiriyor`,
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
    heading: (count) => `${count} araç sessiz`,
  },

  dashboard: {
    title: "Quota Deck",
    rangeLabel: "Aralık",
    range: { day: "Gün", week: "Hafta", month: "Ay" },
    rangeSpan: (days) => (days === 1 ? "Son 24 saat" : `Son ${days} gün`),
    rangeTokens: "Token",
    rangeCost: "Eşdeğer maliyet",
    retention: (days) => `Bu cihazda ${days} günlük geçmiş tutuluyor`,
    unpriced: (tokens) => `${tokens} token'ın fiyatı bilinmiyordu`,
    heatmapLabel: "Son bir aydaki günlük etkinlik",
    heatmapQuiet: "sakin",
    heatmapBusy: "yoğun",
  },

  empty: {
    noTools: {
      title: "Desteklenen araç bulunamadı",
      body: "Quota Deck, kodlama araçlarının zaten yazdığı oturum günlüklerini okur. Claude Code, Codex ya da desteklenen başka bir aracı kur; burada görünsün.",
      action: "Desteklenen araçlar",
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
    "never-reported": "Bu araç bir limit bildirmedi",
  },

  provider: {
    "claude-code": "Claude Code",
    codex: "Codex",
    "copilot-cli": "Copilot CLI",
    kimi: "Kimi",
    "gemini-cli": "Gemini CLI",
    qwen: "Qwen Code",
    opencode: "OpenCode",
    amp: "Amp",
    droid: "Droid",
    codebuff: "Codebuff",
    hermes: "Hermes",
    "pi-agent": "pi-agent",
    goose: "Goose",
    kilo: "Kilo",
    openclaw: "OpenClaw",
    antigravity: "Antigravity",
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
    back: "Bitti",

    languageTitle: "Dil",
    languageSystem: "Sistemle aynı",
    languageEnglish: "English",
    languageTurkish: "Türkçe",
    languageHint: "Tarih ve saat biçimleri sistemin bölge ayarlarını izlemeye devam eder.",

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
      "Claude Code, 5 saatlik ve haftalık limitlerinin gerçek yüzdesini kendi durum satırına veriyor. Bağlarsan tahminin yerini ölçüm alır. Bu cihazdan hiçbir şey çıkmaz ve hiçbir kimlik bilgisi okunmaz.",
    statuslineUnsupported: "Bu makinede Claude Code ayarları bulunamadı.",
    statuslineConnect: "Durum satırını bağla",
    statuslineRevert: "Bağlantıyı kes",
    statuslineInstalled: "Bağlı",
    statuslineFile: (path) => `${path} dosyasını düzenler`,
    statuslineBefore: "Şimdi",
    statuslineAfter: "Bağladıktan sonra",
    statuslineNoPrevious:
      "Ayarlanmış bir durum satırın yok. Bağlantıyı kesmek ayarı tekrar kaldırır.",
    statuslineChains:
      "Mevcut durum satırın çalışmaya devam eder — bizimki onun çıktısını olduğu gibi geçirir.",
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
      "Yalnızca okuma ve yalnızca oturum günlükleri. Sağlayıcı kimlik dosyaları hiç açılmaz. Geri alma anında geçerli olur.",

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
  },
};
