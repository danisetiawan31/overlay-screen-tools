# Done — Screen Overlay Tool

- [x] Inisialisasi Project & Dependencies — selesai, spec: workflow/backlog.md
  Catatan: sesuai spec
- [x] Setup project & tooling (tauri-specta, GitHub Actions CI, Dependabot, Quality Gates) — selesai, spec: workflow/backlog.md & AGENTS §12
  Catatan: CI (.github/workflows/ci.yml) & Dependabot (.github/dependabot.yml) telah dikonfigurasi dan dipush ke GitHub.
- [x] Config persistence (Backend/Core) — selesai, spec: docs/TDD.md §7 & docs/api-contract.md §2
  Catatan: sesuai spec. Memuat config.json dari app_data_dir() saat setup, emit config:error jika missing/malformed/empty key, dan unit test 4 skenario lolos.
- [x] Window properties & font size (Tahap 1 & 2: fontSize & windowBounds) — selesai, spec: docs/TDD.md §8 & docs/api-contract.md §1 & §4
  Catatan: sesuai spec. Command update_font_size divalidasi range 10-32px & diekspor via tauri-specta; windowBounds divalidasi terhadap monitor aktif saat startup, event Moved/Resized di-debounce (500ms) dengan proteksi is_concealed dan flush-on-exit; 14 unit test lolos.
- [x] Tray icon & app lifecycle (Tahap 1 & 2: tray, menu, quit flow & notification wiring) — selesai, spec: docs/TDD.md §9 & docs/PRD.md §3.3 & AGENTS §10
  Catatan: sesuai spec. Inisialisasi TrayIconBuilder dengan context menu 2 item ("Show/Hide Overlay" & "Quit"); logic toggle diekstrak ke toggle_overlay_visibility dengan OverlayState::toggle_concealed; menu Quit memicu close lifecycle & flush-on-exit windowBounds; plugin notification dikonfigurasi pada kegagalan F9 dan config:error; 17 unit test lolos.





