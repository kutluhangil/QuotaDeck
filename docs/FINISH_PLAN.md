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

## F1 — Çekirdek: JSONL dışı kaynaklar ✅
- [x] `Provider::document_globs` + `parse_document`: boyut/değişiklik zamanı değişince tamamı
  okunur (üst sınır 32 MB). Dosya değişiklik zamanı `LineSource::modified` ile gelir.
- [x] `Provider::database_globs` + `read_database`: filigranlı, partili okuma; filigran
  ilerledikçe sonraki geçiş kaldığı yerden sürer, yetişince damga oturur.
- [x] Kümülatif kayıtlar artık maliyetin de yalnız artışını yazar.
- [x] Keşif, sabit yol parçalarını listelemeden doğrudan kontrol eder (kimlik dosyalarının
  adları bile okunmaz).
- [x] `rusqlite` 0.40 (bundled) eklendi; ağaçta HTTP istemcisi yok.

## F2 — Sağlayıcılar ✅ (gerçek logla doğrulama Kutluhan'da)
- [x] pi / OMP, Gemini CLI, Amp, Droid, OpenCode — her biri tek dosya + sentetik fixture + kayıt.
- [x] Kurulu olmayan araç tepsi menüsünde satır almaz.
- [ ] Gerçek logla karşılaştırma ve site listesine ekleme (`docs/USER_ACTIONS.md` §6).

## F3 — CodexBar düzeninde panel ✅
- [x] Sekme çubuğu (Genel bakış + ikonlu sağlayıcılar, mini ölçer, ok tuşlarıyla gezinme).
- [x] Ayrıntı kartı: pencere blokları, hız satırı, Horizon, Maliyet (bugün / 30 gün).
- [x] Genel bakış satırları; limitsiz araçlar için token/maliyet satırı.
- [x] Kullanılan / kalan ayarı: panel ve tepsi (menü + compact başlık).
- [ ] Widget kalan gösterimi (Swift tarafı; bu oturumda yapılmadı).

## F4 — Yeni ölçüler ✅
- [x] Prompt-cache durumu ve bağlam doluluğu: Claude Code statusline'ının resmi
  `prompt_cache` ve `context_window` alanlarından, ölçülmüş olarak (tahmin değil).
- [x] 30 günlük önbellek isabet oranı.
- [x] Dashboard'da oturum listesi (son 300 oturum, checkpoint'te saklanır).
- [x] Checkpoint sürümü 3: eski checkpoint'ler bir kez yeniden okunur.

## F5 — Veri ✅
- [x] Saklama 32/90/365 gün (zaten vardı).
- [x] `pricing-overrides.json`: geçersiz satır atlanır ve loga yazılır; değişiklik geçmişi
  yeniden fiyatlar.
- [~] WSL: otomatik tarama yerine ek klasör ayarında Windows'a özel ipucu. Gerekçe: kapalı bir
  WSL dağıtımına `\\wsl.localhost` üzerinden erişmek onu başlatabilir; Windows'ta test edilmeden
  her yenilemede bunu yapmak riskli.

## F6 — CLI raporları ✅
- [x] `quotadeckctl report daily|monthly|session [--provider] [--since] [--until] [--json]`.

## F7 — Kapanış ✅
- [x] Genel klavye kısayolu `⌘⌥Q` / `Ctrl+Alt+Q`, isteğe bağlı (varsayılan kapalı).
- [x] i18n (en/tr/de/es), DISCOVERY, USER_ACTIONS, CI adım adı.
- [ ] Commit/push ve CI'da Windows doğrulaması (Kutluhan isteyince).

## Kapsam dışı (bilerek)
OAuth/çerez/API ile veri, güncelleme kontrolü, leaderboard/paylaşım, yerel HTTP sunucu,
kullanıcı betiğini veri kaynağı olarak çalıştırma (App Store sandbox'ı izin vermez).
