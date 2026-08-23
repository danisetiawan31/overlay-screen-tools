# TDD — Screen Overlay Tool

## 1. Tech Stack

- Runtime/framework: **Tauri v2** — `src-tauri/` (Rust, "core") + React + TypeScript (`vite`) sebagai frontend/webview
- Styling: Tailwind CSS (dark theme sebagai default)
- Markdown rendering: `react-markdown` + `remark-gfm`, syntax highlighting via `rehype-highlight`/`shiki` (pilih salah satu saat implementasi)
- File watcher: crate `notify`
- Global hotkey: `tauri-plugin-global-shortcut` untuk F8 dan F9 dua-duanya (native `ShortcutState::Pressed`/`Released`)
- File picker: `@tauri-apps/plugin-dialog`
- HTTP client (Rust core): crate `reqwest` (termasuk multipart untuk upload audio blob)
- Config persistence: `config.json` lokal, serialize/deserialize via `serde` + `serde_json`
- Error handling: crate `anyhow` + `thiserror`
- Win32 API langsung (Windows-specific, verifikasi POC): crate `windows-sys` — dibutuhkan karena API level-Tauri untuk capture-exclusion dan reposisi window terbukti tidak cukup/tidak reliable, lihat §3. Juga dipakai untuk watcher thread yang melindungi native file dialog dari capture (lihat §3).
- Tray: `tauri::tray::TrayIconBuilder`

## 2. Arsitektur Proses

- **Tauri core** (`src-tauri/`, Rust): registrasi hotkey (`tauri-plugin-global-shortcut`), file watcher (`notify`), semua HTTP request ke Groq & OpenRouter (`reqwest`), baca/tulis `config.json` (`serde`), kontrol window (posisi, `set_content_protected`, tray).
- **Webview** (React frontend): UI dua tab (Notes & Q&A), render markdown, `getUserMedia` + `MediaRecorder` untuk capture audio mic (didukung karena WebView2 di Windows berbasis Chromium).
- **Komunikasi core ↔ webview**:
  - Request-response: `#[tauri::command]` di Rust, dipanggil dari frontend lewat `invoke('nama_command', payload)`.
  - Event satu arah: `app.emit('nama:event', payload)` dari Rust, `listen('nama:event', callback)` di frontend.
  - **Tipe payload di-generate dari Rust, bukan ditulis manual dua kali**: `tauri-specta` dipakai untuk generate file `bindings.ts` otomatis dari signature command dan tipe event yang didefinisikan di Rust (struct `serde`+`specta::Type`). Rust tetap satu-satunya sumber definisi tipe; TypeScript-nya di-generate saat build (`#[cfg(debug_assertions)]`), bukan didefinisikan ulang terpisah di frontend yang rawan desync.
- **Kenapa API call di core, bukan webview**: API key OpenRouter/Groq cuma pernah disentuh di Rust (core). Webview cuma kirim audio blob/terima hasil lewat command & event — supaya key nggak nyasar ke devtools webview yang gampang di-inspect.

## 3. Stealth Capture-Exclusion

**Status: TERVERIFIKASI via POC manual (toggle berulang, direkam capture software sungguhan) — bukan lagi hipotesis.** Mekanisme final beda dari rencana awal karena dua API level-Tauri terbukti tidak cukup:

- **`window.set_content_protected(true)` / `contentProtected: true` di `tauri.conf.json` HANYA menerapkan `WDA_EXCLUDEFROMCAPTURE` ke root HWND (container TAO).** Konten sungguhan render di child HWND terpisah (WebView2/Edge) yang TIDAK ikut terlindungi otomatis — kalau dibiarkan, window "kelihatan" ter-protect padahal isinya tetap bocor ke capture.
- **Wajib**: setelah window dibuat, ambil HWND native lewat `window.hwnd()`, terapkan `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` ke root HWND, lalu `EnumChildWindows(hwnd, ...)` untuk menerapkan affinity yang sama ke **setiap child HWND** (termasuk WebView2). Ini butuh crate `windows-sys` untuk akses langsung ke Win32 API — lihat §10.
- **`window.set_position()` (API level Tauri) terbukti tidak reliable** — pada kondisi tertentu diabaikan/tertahan oleh compositor DWM. **Wajib** ganti dengan `SetWindowPos` Win32 native (lewat `windows-sys`) untuk reposisi ke `(-9999, -9999)` (Concealed) dan balik ke posisi tersimpan (Normal) — bukan API Tauri.
- **Keputusan desain inti tetap sama, cuma jalur implementasinya native**: toggle visibility (F9 / tray Show-Hide) WAJIB lewat reposisi window, **BUKAN** `hide()`/`show()` Tauri maupun Win32 (`ShowWindow`). Alasan tetap sama seperti sebelumnya — hide/show adalah pola yang terbukti memicu bug rendering serupa di ekosistem Tauri.
- **State machine**: dua state — `Normal` (window di posisi terakhir yang disimpan user) dan `Concealed` (window di `-9999, -9999`). Transisi Normal↔Concealed HANYA lewat `SetWindowPos` native, tidak pernah lewat `hide()`/`show()` dalam bentuk apapun.
- **Belum terverifikasi, perlu dicek saat implementasi fitur lanjutan (bukan blocker POC)**: apakah `EnumChildWindows` perlu di-run ulang kalau WebView2 memunculkan child surface baru — **sudah diinvestigasi dan dijawab**, lihat detail lengkap di bawah.

**Kategori elemen UI dan implikasinya terhadap capture-exclusion:**
1. Elemen UI web biasa (React, modal HTML, dropdown custom, tooltip) — render di dalam canvas/DOM WebView2 yang sama, tidak membuat HWND baru. Proteksi yang diterapkan di awal tetap aktif otomatis, tidak perlu re-apply.
2. Dropdown `<select>` native — dirender lewat DirectComposition layer di bawah HWND render host yang sama (sudah ter-cover `EnumChildWindows`), tidak perlu penanganan khusus.
3. **Native file dialog (`@tauri-apps/plugin-dialog`) — WAJIB penanganan khusus.** `IFileDialog` Windows adalah top-level HWND terpisah milik proses yang sama, BUKAN child dari window utama — `EnumChildWindows` dari window utama tidak akan pernah menjangkaunya.

**Mekanisme wajib untuk melindungi file dialog dari capture:**
- Dialog Windows bersifat modal/blocking (thread pemanggil tertahan sampai user pilih file/cancel), jadi HWND dialog baru tidak bisa ditunggu di thread yang sama.
- **Wajib**: sebelum memanggil fungsi dialog yang blocking, spawn thread ringan terpisah yang polling `EnumWindows` (di-filter by process ID lewat `GetWindowThreadProcessId`) tiap ~30ms selama beberapa detik pertama, mencari top-level window baru milik proses yang sama, lalu langsung terapkan `SetWindowDisplayAffinity(hwnd_dialog, WDA_EXCLUDEFROMCAPTURE)` begitu ketemu. Thread pemanggil dialog tetap jalan blocking seperti biasa di thread lain.
- Butuh fitur `windows-sys` tambahan: `Win32_System_Threading` (untuk `GetCurrentProcessId`) — lihat §10.
- Pola ini generik, bukan cuma buat file dialog — kalau ke depannya ada native dialog/window lain yang dipicu app ini, mekanisme watcher yang sama bisa dipakai ulang.
- **Kenapa ini penting secara produk, bukan cuma teknis**: kalau dialog file picker kebuka pas lagi live dan ikut ter-capture, itu me-reveal ke penonton bahwa ada semacam tool catatan yang lagi dibuka — bertentangan dengan tujuan inti tool ini (invisible ke penonton).
- Validasi `windowBounds` (posisi normal tersimpan) terhadap monitor aktif tetap berlaku — lihat §8.

## 4. Global Hotkeys

| Hotkey | Aksi | Mekanisme |
|---|---|---|
| F8 (tahan & lepas) | Push-to-talk Q&A, auto-switch ke tab Q&A | `tauri-plugin-global-shortcut`, native `ShortcutState::Pressed`/`Released` |
| F9 (tekan sekali) | Toggle Normal↔Concealed (lihat §3 — reposisi via `SetWindowPos` native) | `tauri-plugin-global-shortcut`, `ShortcutState::Pressed` saja |
| F10 (Ghost Mode) | Di-defer ke backlog (lihat PRD Section 6) | — |

- Rust crate untuk hotkey ini di-compile sebagai bagian dari `cargo build` biasa — bukan fetch binary terpisah, jadi konsisten selama toolchain (Rust + MSVC Build Tools, lihat AGENTS.md §12) terpasang benar.
- **Belum terverifikasi, jangan diasumsikan aman**: apakah mekanisme deteksi Pressed/Released ini punya risiko di-flag antivirus (perilaku mirip keylogger secara struktural) — WAJIB dicek pas POC/implementasi awal, bukan diasumsikan otomatis aman.
- **Registrasi F8 dan F9 independen satu sama lain** — keduanya sama-sama lewat `tauri-plugin-global-shortcut`, tapi bisa gagal sendiri-sendiri (misal app lain sudah claim F8 tapi F9 aman, atau sebaliknya). Fallback tray "Show/Hide Overlay" (lihat §9) cuma menutup kegagalan **F9** (toggle visibility) — itu tidak memberi apa-apa kalau yang gagal justru F8.
- **Wajib: graceful degradation kalau registrasi F8 gagal, independen dari F9.** Kalau F8 gagal register, disable fitur push-to-talk/Q&A dengan warning jelas lewat tray notification — Notes mode, F9 (toggle Normal/Concealed), dan tray tetap harus berjalan penuh. Ini bukan fitur baru — ini penegasan ulang NFR "Graceful degradation" di PRD yang ditulis framework-agnostic, supaya tidak lolos begitu saja hanya karena sumber risikonya (native addon pihak ketiga vs kegagalan registrasi plugin resmi) kelihatan beda.

## 5. Audio Capture & STT

- Capture: `getUserMedia` + `MediaRecorder` di webview. Output `audio/webm;codecs=opus`, tidak perlu dikonversi — Groq API menerima `webm` secara native.
- **Permission mic**: dikonfigurasi lewat sistem capability/permission Tauri v2 (`src-tauri/capabilities/*.json`) — perlu dicek eksplisit saat implementasi apakah `getUserMedia` sebagai Web API standar butuh capability tambahan atau cukup permission prompt bawaan webview.
- HTTP call ke Groq (`whisper-large-v3-turbo`) dan OpenRouter: crate `reqwest` di Rust core (termasuk `multipart` untuk upload audio blob), dipanggil dari `#[tauri::command]`.
- Alur: F8 dilepas → webview stop `MediaRecorder` → blob dikirim lewat `invoke` → core kirim ke Groq → hasil di-`emit` balik ke webview (`transcript:result`, non-blocking) → di tick yang sama, core lanjut kirim ke OpenRouter pakai teks transkrip sebagai input.
- **Catatan soal "paralel"**: transkrip → AI itu sekuensial by nature (AI butuh teks transkrip sebagai input). "Non-blocking/paralel" artinya UI tidak menunggu AI selesai untuk menampilkan transkrip — bukan dua request berjalan bersamaan dari nol.
- **F8 ditekan lagi sebelum request sebelumnya selesai**: request lama di-discard, request baru menang. Tidak di-queue.
- **State Q&A di frontend adalah satu object yang di-overwrite**, bukan array yang di-push (`{ question, answer, status }`) — mencegah implementasi default yang nge-append jadi history list.
- **Minimum hold duration ~300–500ms** untuk F8 sebelum audio benar-benar dikirim — mencegah tekan-tanpa-sengaja yang tetap trigger request (biaya + noise).
- **Cek ketersediaan mic** (permission/device) saat app start atau percobaan F8 pertama, tampilkan warning jelas kalau gagal.
- Error di titik manapun (STT gagal, OpenRouter gagal) → event `qa:error`, tampil jelas — bukan gagal senyap.

## 6. Notes/Teleprompter Mode

- File picker: `@tauri-apps/plugin-dialog`, filter `.md`, dijalankan dari core.
- File watcher: crate `notify` di core, watch file terpilih → tiap event `change` → baca ulang isi → `emit` ke webview (`notes:update`). Jika terjadi kegagalan pembacaan (file terhapus, dipindah, atau permission berubah), core mengirim event `notes:error` BERSAMAAN dengan tray notification native agar UI menampilkan banner peringatan persisten.
- Rendering: `react-markdown` + `remark-gfm` (table/strikethrough/dst) + syntax highlighting untuk code block. Konten dikonfirmasi murni teks/markdown, tanpa gambar lokal.
- Scratchpad: state lokal di frontend, tidak perlu persist ke disk (fallback ad-hoc, bukan dokumen permanen).

## 7. Config Storage

Lokasi: `config.json` lokal di direktori data aplikasi (`app_data_dir()`).

Skema (bukan ERD relasional — cuma flat config, tidak ada database):
```json
{
  "openrouterApiKey": "string",
  "groqApiKey": "string",
  "windowBounds": { "x": 0, "y": 0, "width": 400, "height": 600 },
  "fontSize": 14,
  "lastOpenedNotesPath": "string | null",
  "hotkeys": { "pushToTalk": "F8", "toggleVisibility": "F9" }
}
```
- API key tersimpan plaintext — risiko yang diterima secara sadar untuk tool single-user di laptop pribadi.
- **Baca config wajib lewat `Result`**: kalau `serde_json::from_str` gagal parse (malformed JSON), match `Err` dan tampilkan error jelas lewat tray notification — jangan biarkan app gagal start tanpa pesan apapun.
- **First-run**: tidak ada UI setup wizard — user isi `openrouterApiKey`/`groqApiKey` dengan edit `config.json` manual. Kalau file belum ada atau key kosong saat app start, tampilkan pesan error jelas (tray notification), bukan gagal senyap.

## 8. Window Properties & Font Size

- Frameless: `decorations: false`. Draggable: atribut HTML `data-tauri-drag-region` di elemen titlebar custom.
- Always-on-top: `true`, `resizable: true` (opsional).
- Dark theme sebagai default styling (Tailwind).
- Font size: kontrol tombol **+ / −** di dalam UI overlay (bukan hotkey global) — `Ctrl+`/`Ctrl-` sengaja dihindari sebagai global hotkey karena itu shortcut zoom yang dipakai hampir semua app lain. Klik tombol update state `fontSize` di frontend, disimpan balik ke `config.json` lewat command.
- **Validasi `windowBounds` terhadap monitor aktif**: pakai Tauri Monitor API (`window.available_monitors()`) sebelum window dibuka saat startup — kalau posisi tersimpan di luar area semua monitor aktif, fallback ke posisi default/center. **Catatan prioritas**: user cuma pakai 1 layar laptop — skenario utama yang memicu bug ini (posisi tersimpan saat docked ke monitor eksternal) kemungkinan besar nggak akan kejadian di real usage. Validasi tetap dipertahankan karena murah, tapi bukan item kritikal untuk kondisi sekarang.

## 9. App Lifecycle & Tray

- Mekanisme: `tauri::tray::TrayIconBuilder` dengan context menu dua item: "Show/Hide Overlay" dan "Quit".
- **"Show/Hide Overlay"** adalah fallback manual kalau registrasi hotkey F9 gagal (konflik dengan app lain) — window frameless tanpa titlebar tidak punya cara lain untuk menutup/memunculkan app.
- Klik "Quit" → keluar dari aplikasi sepenuhnya.
- Tray icon menampilkan indikator sederhana saat F8 sedang ditahan (recording), supaya user tahu status tanpa harus melihat overlay langsung.

## 10. Dependencies Terkunci

```
# Rust (src-tauri/Cargo.toml)
tauri, tauri-plugin-global-shortcut, tauri-plugin-dialog
notify, reqwest, serde, serde_json, anyhow, thiserror
specta, tauri-specta (generate binding TypeScript dari command & event Rust — lihat §2)
windows-sys (fitur: Win32_UI_WindowsAndMessaging, Win32_Foundation, Win32_System_Threading — untuk EnumChildWindows/SetWindowDisplayAffinity/SetWindowPos native + watcher thread file dialog, lihat §3)

# npm (frontend)
react, react-dom, typescript
react-markdown, remark-gfm, rehype-highlight (atau shiki)
tailwindcss
```

## 11. Status Sinkronisasi PRD ↔ TDD

- PRD tidak perlu diubah untuk stack ini — seluruh isinya ditulis di level requirement/produk (capture-exclusion, tray, hotkey, error handling), bukan level implementasi framework tertentu.
- Font size (+/− button), tray icon (Show/Hide + Quit), Ghost Mode (di-defer ke backlog), packaging (di-skip untuk versi ini) — semua keputusan ini sudah tercermin di PRD Section 3, 4, dan 6.
- Status stack: **POC stealth capture-exclusion sudah lolos** (diverifikasi manual, toggle berulang, capture software sungguhan) — mekanisme final di §3 beda dari rencana awal (butuh `windows-sys` untuk child-HWND + reposisi native), tapi keputusan desain intinya (off-screen-move, bukan hide/show) terbukti benar. Feature loop implementasi (`docs/api-contract.md`) sudah bisa dimulai.
