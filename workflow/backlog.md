# Backlog — Screen Overlay Tool

Fitur diturunkan dari `docs/PRD.md` Section 3 & 4. Status tracking (progress per fitur) terbentuk dari interaksi file ini dengan `workflow/done.md` selama loop implementasi jalan — checklist di bawah cuma starting point. Detail mekanisme tiap item sudah didefinisikan lengkap di TDD/api-contract — jangan diulang di sini, cukup jadi pointer.

## MVP

- [x] **Setup project & tooling** — project ini lanjut dari folder POC (`poc-overlay`), bukan scaffold baru. Wiring `tauri-specta`, GitHub Actions CI, Dependabot, konfirmasi `rust-analyzer`/`cargo clippy`/`cargo fmt` jalan — semua yang belum ada di POC minimal. Ref: AGENTS §12.
- [ ] **Stealth capture-exclusion** — Ref: TDD §3. Termasuk verifikasi resize window (drag resize-handle) pakai capture software sebelum item ini ditutup — POC awal cuma verifikasi toggle posisi, belum resize.
- [x] **Config persistence** — Ref: TDD §7, api-contract §2 (`config:error`).
- [x] **Window properties & font size** (termasuk resizable + `windowBounds` via move & resize event) — Ref: TDD §8, api-contract §1 (`update_font_size`), api-contract §4 (`windowBounds`).
- [x] **Tray icon & app lifecycle** (termasuk notifikasi kegagalan registrasi F9) — Ref: TDD §9, TDD §4.
- [ ] **Global hotkeys** (F8 push-to-talk, F9 toggle) — Ref: TDD §4, api-contract §1 (`get_app_state.qaAvailable`).
- [ ] **App shell — dua tab Notes/Q&A + auto-switch ke tab Q&A saat F8 mulai rekam** — Ref: PRD §4 poin 2, api-contract §2 (`qa:recording-started`).
- [ ] **Notes/Teleprompter mode** (termasuk toggle File Mode ↔ Scratchpad, scratchpad tetap ephemeral non-persist) — Ref: TDD §6, api-contract §1–2 (`notes:error` banner & tray notification).
- [ ] **Live Q&A mode** — Ref: TDD §5, api-contract §1–3.

## Deferred (dari PRD Section 6 — dipertimbangkan lagi nanti, bukan dihapus)

- [ ] Automatic viewer-question capture dari live chat platform
- [ ] Scrollable/persistent history Q&A dalam satu sesi
- [ ] Dukungan platform selain Windows 11
- [ ] Ghost Mode (click-through overlay)
- [ ] Packaging jadi installer `.exe`
