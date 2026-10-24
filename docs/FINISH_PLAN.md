# Bitiş planı (2026-10-24)

Kaynaklar: CodexBar incelemesi (kutluhangil/CodexBar, upstream steipete/CodexBar), ccusage'ın
Rust okuyucuları (biçim referansı) ve rakip analizi. Kararlar:

- **Veri yerel kalır.** Ağ isteği, OAuth, çerez, API anahtarı yok. CodexBar'dan yalnızca yerel
  dosya okuyan kaynaklar alınır; Cursor, Copilot API, claude.ai web, kredi bakiyesi gibi yalnızca
  ağla gelen veriler kapsam dışıdır.
- **Panel CodexBar düzenine geçer:** ikonlu sağlayıcı sekmeleri + Genel bakış, seçili sağlayıcı için
  ayrıntı kartı. Horizon şeridi ve ölçüldü/tahmini rozeti korunur.
- **Yeni sağlayıcılar:** pi/OMP, Gemini CLI, Amp, Droid, OpenCode.

Her faz sonunda: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`,
`cargo test --workspace`, perf bütçesi, `ui` içinde `npm run typecheck && npm test`.
Commit/push yalnızca Kutluhan isteyince.

## F0 — CI yeşil
- [x] Windows clippy `needless_return` (`app/src/lib.rs`).
- [x] Linux perf testi: `ru_maxrss` yerine `VmHWM` (`core/tests/perf.rs`).
- [x] Windows'ta 4 test: `/tmp` ve `.exe` varsayımları (`app/src/deck.rs`, `app/src/statusline.rs`).
- [ ] CI'da doğrulama (push gerektirir).

## F1 — Çekirdek: JSONL dışı kaynaklar
Yeni sağlayıcıların üçü bütün halinde yeniden yazılan JSON, biri SQLite kullanıyor.
- `Provider::document_globs` + `Provider::parse_document`: dosya boyutu ya da kimliği değişince
  tamamı (üst sınır 32 MB) yeniden okunur. Her olay kararlı bir `DedupKey` taşır ya da
  `Accounting::Cumulative` kullanır; yeniden okuma sayıyı iki katına çıkaramaz.
- `Provider::database_globs` + `Provider::read_database`: yalnızca okuma kipinde, `time_created`
  filigranından (watermark) sonraki satırlar. İmleç filigranı checkpoint'te saklanır.
- Yeni bağımlılık: `rusqlite` (bundled). Etki: ikili dosyaya yaklaşık 1–1,5 MB, bellek yalnızca
  sorgu sırasında. Ağ kodu içermez; `constraints` işi değişmez.

## F2 — Sağlayıcılar
Her biri: tek dosya + sentetik fixture + `providers/mod.rs` kaydı. Gerçek log olmadığı için
fixture'lar `synthetic_` önekli; gerçek logla doğrulama `USER_ACTIONS.md`'ye eklenir.
| Sağlayıcı | Kaynak | Biçim | Kota |
|---|---|---|---|
| pi / OMP | `~/.pi/agent/sessions/**/*.jsonl`, `~/.omp/agent/sessions` | JSONL | yok |
| Gemini CLI | `~/.gemini/tmp/*/chats/*.json(l)` | JSON belge / JSONL | yok |
| Amp | `~/.local/share/amp/threads/*.json` | JSON belge | yok |
| Droid | `~/.factory/sessions/**/*.settings.json` | JSON belge, kümülatif | yok |
| OpenCode | `~/.local/share/opencode/opencode.db`, eski `storage/message/**/*.json` | SQLite / JSON | yok |
Kota yazmayan araçlar yalnızca token ve maliyet gösterir; kota uydurulmaz. Fiyat: Claude
modelleri mevcut tablodan; aracın kendi yazdığı maliyet varsa o kullanılır; yoksa `Unpriced`.

## F3 — CodexBar düzeninde panel
- Üstte sekme çubuğu: Genel bakış + her sağlayıcı için ikon, ad ve altta mini kullanım çubuğu.
- Ayrıntı kartı: başlık (ad, "şimdi güncellendi", plan), pencere başına çubuk +
  "%X kullanıldı · N sonra sıfırlanır", hız satırı ("önde +18% · 11:20'de biter"), Horizon,
  Maliyet bölümü (bugün / 30 gün, token), alt eylemler (Dashboard, Ayarlar, Çıkış).
- Genel bakış: sağlayıcı başına sıkı satır; tıklayınca o sekmeye geçer.
- Ayar: **kullanılan / kalan** gösterimi (panel, tepsi, widget aynı tercihi kullanır).

## F4 — Yeni ölçüler (yerel loglardan)
- Prompt-cache geri sayımı: son asistan yanıtı + TTL (5 dk; 1 sa yazımı varsa 1 sa). Tahmini rozetli.
- Önbellek isabet oranı: cache_read / (input + cache_read + cache_write).
- Bağlam penceresi doluluğu: statusline yardımcısı `context_window` alanını da kaydeder.
- Oturum listesi (Dashboard): oturum başına başlangıç, süre, model, token, maliyet.

## F5 — Veri
- Saklama süresi seçeneği: 32 gün (varsayılan) / 90 gün / 365 gün.
- Kullanıcı fiyat dosyası: uygulama veri klasöründe `pricing-overrides.json`; yerleşik tabloyu
  model adına göre geçersiz kılar, doğrulanır, hatalı satır yok sayılır ve bildirilir.
- Windows: WSL dağıtımlarındaki `~/.claude` ve `~/.codex` kökleri (`\\wsl.localhost\<distro>\home\*`).

## F6 — CLI raporları
- `quotadeck report daily|monthly|session [--since YYYY-MM-DD] [--until …] [--provider …] [--json]`.

## F7 — Kapanış
- Genel klavye kısayolu (panel aç/kapat). Bağımlılık: `tauri-plugin-global-shortcut`.
- Site, i18n (en/tr/de/es), mağaza metni, `RELEASE_CHECKLIST.md`, `USER_ACTIONS.md` güncellemesi.

## Kapsam dışı (bilerek)
OAuth/çerez/API ile veri, güncelleme kontrolü, leaderboard/paylaşım, yerel HTTP sunucu,
kullanıcı betiğini veri kaynağı olarak çalıştırma (App Store sandbox'ı izin vermez).
