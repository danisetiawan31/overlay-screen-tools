# AGENTS.md — Screen Overlay Tool

Baca file ini dulu sebelum mengerjakan apapun. Ini rules aktif yang berlaku lintas fitur — detail per-fitur ada di `workflow/<nama_fitur>.md`.

## 0. Prasyarat Mutlak: POC Stealth Capture-Exclusion

**STATUS: LOLOS PENUH.** Diverifikasi manual oleh user — termasuk state paling kritis (window di posisi Normal, on-screen, kelihatan mata telanjang, tetap bersih di capture software). State "Concealed" (off-screen) sengaja dicatat BUKAN ujian sesungguhnya dari `SetWindowDisplayAffinity` — off-screen otomatis tidak ke-capture apapun kondisinya, terlepas dari affinity aktif atau tidak. Yang membuktikan mekanisme ini benar-benar bekerja adalah state Normal tetap bersih di capture meski window sungguhan tampil di layar fisik.

Mekanisme final beda dari rencana awal: API level-Tauri (`set_content_protected`, `set_position`) terbukti tidak cukup sendirian — implementasi final pakai Win32 API langsung lewat crate `windows-sys` (lihat TDD Section 3 untuk detail lengkap, termasuk mekanisme perlindungan native file dialog via background watcher thread). Feature loop implementasi sudah boleh dimulai.

Riwayat kenapa gate ini ada: mekanisme `WDA_EXCLUDEFROMCAPTURE` di Windows historically rapuh (lihat Tauri issue [#14189](https://github.com/tauri-apps/tauri/issues/14189)) — itu sebabnya POC ini wajib lolos dulu sebelum dokumen lain ditulis, dan sekarang sudah terbukti bisa dicapai di mesin user dengan mekanisme yang sudah dikunci di TDD.

## 1. Dokumen Acuan (source of truth — WAJIB dibaca sebelum implementasi)

- `docs/PRD.md` — latar belakang, scope, fitur MVP
- `docs/TDD.md` — arsitektur core/webview, keputusan teknis (stealth capture, hotkey, audio/STT, config)
- `docs/api-contract.md` — kontrak command & event Tauri lengkap (nama, arah, payload shape) — *disusun setelah file ini, lihat larangan §11 soal urutan*

Dokumen acuan di atas diubah oleh user, bukan agent sepihak. Agent dilarang berimprovisasi menambah/mengubah requirement sendiri tanpa konfirmasi user. Namun, kalau saat implementasi/planning ditemukan requirement/kontrak lama yang perlu direvisi demi best practice, agent sah mengusulkan revisi ke user (termasuk untuk fitur berstatus "Selesai" — lihat §4).

## 2. Tech Stack

| Layer | Teknologi |
|---|---|
| Core (backend) | Rust (`src-tauri/`), Tauri v2 |
| Frontend/webview | React + TypeScript (`vite`), Tailwind CSS |
| Markdown rendering | `react-markdown` + `remark-gfm` + `rehype-highlight`/`shiki` |
| File watcher | crate `notify` |
| Global hotkey | `tauri-plugin-global-shortcut` (F8 & F9, native Pressed/Released) |
| File picker | `@tauri-apps/plugin-dialog` |
| HTTP client (core) | crate `reqwest` |
| Error handling (core) | crate `anyhow` + `thiserror` |
| Win32 API langsung | crate `windows-sys` (capture-exclusion child-HWND + reposisi window native + watcher thread proteksi file dialog, lihat TDD §3) |
| Type-safe IPC | `tauri-specta` (generate binding TypeScript dari command & event Rust) |
| Config persistence | `config.json` lokal via `serde`/`serde_json` |
| Testing frontend | Vitest (unit), `@testing-library/react` |
| Testing core | `cargo test` (unit Rust) |
| Testing E2E | WebdriverIO + `tauri-driver`/`@wdio/tauri-service` |

## 3. Prinsip Kerja — ATURAN PALING PENTING DI FILE INI

1. **Selalu rencana dulu, baru eksekusi.** Sebelum menulis kode untuk task apapun, tulis dulu rencana implementasi berupa task list langkah-langkah kecil. Tunggu persetujuan eksplisit dari user sebelum mulai coding.
2. **Satu langkah kecil per iterasi — bukan satu fitur, apalagi semua fitur.** Definisi "langkah kecil" untuk project ini:
   - Core (Rust): 1 `#[tauri::command]`/handler event + logic pendukungnya (bukan seluruh domain/mode sekaligus)
   - Frontend: 1 komponen atau 1 tab (Notes / Q&A), bukan seluruh alur UI dari awal sampai akhir
   - Config schema: 1 perubahan field per langkah
3. **Berhenti setelah 1 langkah selesai.** Laporkan pakai format berikut (wajib, bukan opsional):

   ## Laporan: <nama task/tahap>

   ### 1. Checklist scope (mirror nomor requirement di prompt/spec)
   - [x] <requirement> — <1 baris implementasi>
   - [ ] <requirement> — SKIP, alasan: <kenapa>

   ### 2. File berubah
   - `path/file` (baru/modify) — <1 baris isi, JANGAN paste kode mentah>

   ### 3. Verifikasi — command + output APA ADANYA (bukan parafrase/ringkasan)

   ```bash
   $ <command persis yang dijalankan>
   <output penting, bukan output mentah lengkap>
   ```

   ### 4. Deviasi & konsistensi

   <Wajib diisi walau "tidak ada deviasi". Kalau task menyentuh konvensi yang rawan dilanggar diam-diam (penamaan command/event — §10, larangan `any`/`.unwrap()` sembarangan — §5), sebutkan CARA mengeceknya, bukan cuma klaim "sudah sesuai".>

   Task kecil (1 file, 1 handler) — Section 1 & 4 boleh dipadetin jadi 1-2 baris. Section 3 TIDAK BOLEH diskip dalam kondisi apa pun.

   Tunggu review/persetujuan user sebelum lanjut ke langkah berikutnya. JANGAN otomatis lanjut tanpa diminta.

4. **Jangan mengasumsikan requirement yang tidak eksplisit ada di `docs/`.** Ambigu → tanya. Jangan menebak lalu diam-diam mengimplementasikan tebakan itu.
5. **Ikuti urutan dependency logis** — core (command handler, integrasi Groq/OpenRouter, config) dulu, baru frontend/UI yang menyertainya. Jangan bangun komponen yang bergantung ke command/event yang belum ada & belum teruji.
6. **Karena user baru di Rust**: setiap kali memotong keputusan teknis yang Rust-spesifik (ownership, `Option<T>`, `Result<T,E>`, lifetime, dst), sertakan 1 baris analogi ke konsep TypeScript yang setara (misal `Option<T>` ↔ `null`/`undefined`, `Result<T,E>` ↔ try/catch) di laporan §3 poin 4 — supaya user tetap bisa mengevaluasi keputusan tanpa harus jadi expert Rust dulu.

## 4. Kebebasan Implementasi

- **Scope vs Detail Teknis**: Scope/requirement wajib eksplisit dari `docs/` atau dikonfirmasi user. Untuk detail teknis, agent bebas berimprovisasi asal ada benefit konkret. Catat improvisasi di "Catatan" `done.md`.
- **Modifikasi & Rollback Fitur "Selesai"**: Status "Selesai" di `backlog.md` adalah checkpoint, bukan segel permanen. Modifikasi dengan benefit konkret boleh diusulkan (tetap wajib approval eksplisit). Rollback diperbolehkan kalau modifikasi ternyata salah arah. Dicatat sebagai entry **Addendum** baru di `done.md`, entry asli tidak dihapus.

## 5. Konvensi TypeScript, Rust & Lint

**TypeScript (frontend)**
- `tsconfig.json`: `"strict": true` wajib aktif.
- **`any` dilarang keras** — ESLint rule `@typescript-eslint/no-explicit-any` di-set `"error"`. Tipe belum jelas → `unknown` + narrowing.
- Lint (`eslint`) DAN type-check (`tsc --noEmit`) wajib lolos berdua, gate terpisah.

**Rust (core)**
- `cargo clippy` wajib lolos — gate terpisah, sejajar dengan `tsc --noEmit` di sisi frontend. Ini yang menangkap pattern non-idiomatic yang gampang lolos tanpa sadar sewaktu belum terbiasa dengan Rust.
- `cargo fmt` dijalankan otomatis, zero-config, sebelum commit.
- **`.unwrap()`/`.expect()` dilarang keras di production code path** (analog larangan `any`) — itu bikin app panic mendadak tanpa penanganan. Pakai `Result`/`?` operator atau `match` eksplisit. Pengecualian cuma di test code.
- **Semua `#[tauri::command]` wajib return `Result<T, String>`** (bukan `T` polos), termasuk command yang secara logika "hampir nggak mungkin gagal" — supaya `invoke()` di frontend selalu punya jalur `.catch()` generik buat kegagalan tak terduga, di luar event-event spesifik yang sudah didefinisikan di `docs/api-contract.md`.
- **Blok `unsafe` (Win32 FFI lewat `windows-sys`) wajib dijaga ketat**: scope `unsafe { }` sekecil mungkin (cuma bungkus pemanggilan FFI itu sendiri, bukan seluruh fungsi), dan setiap blok wajib punya komentar `// SAFETY: <alasan kenapa ini aman>` menjelaskan invariant yang dijamin (misal "HWND valid karena baru diambil dari window.hwnd() di baris sebelumnya"). Ini bukan gaya penulisan opsional — kode ini menyentuh Win32 API langsung untuk capture-exclusion, kesalahan di sini bisa berujung undefined behavior yang sulit di-debug.
- Payload command/event (request/response tiap command, payload tiap event) wajib punya tipe eksplisit yang **di-generate dari Rust lewat `tauri-specta`** (bukan didefinisikan ulang manual di TypeScript) — Rust struct (`serde`+`specta::Type`) adalah satu-satunya sumber definisi, file `bindings.ts` yang dihasilkan dipakai apa adanya di frontend. Ini bukan sekadar niat "harus di-share" — tanpa `tauri-specta`, Rust dan TypeScript adalah dua bahasa terpisah yang tidak bisa saling import tipe, jadi mekanisme generate ini WAJIB dipakai, bukan opsional.

## 6. Kebijakan Test & Retry

- **Unit test frontend (Vitest)**: komponen renderer (`@testing-library/react`), logic non-trivial di frontend.
- **Unit test core (`cargo test`)**: logic di Rust (parsing config, validasi `windowBounds`, handler command — dengan mock HTTP client untuk Groq/OpenRouter).
- **Global hotkey (F8/F9) — JANGAN disimulasikan sebagai physical keypress asli di test otomatis.** Itu event system-wide yang bisa nyasar ke app lain yang sedang fokus kalau benar-benar dipicu. Test callback yang didaftarkan ke `tauri-plugin-global-shortcut` secara langsung (panggil fungsi handler dengan event palsu), bukan lewat OS asli.
- **E2E (WebdriverIO + `tauri-driver`/`@wdio/tauri-service`)**: jalur resmi Tauri untuk testing desktop app end-to-end (Playwright tidak official-support Tauri karena webview bukan Chromium standalone). Dipakai untuk alur lintas-proses (frontend kirim aksi → core → frontend terima hasil lewat event), bukan pengganti unit test untuk logic terisolasi.
- **Wajib: core skip registrasi hotkey global asli saat E2E.** Sama seperti bootstrap app pada umumnya, E2E menjalankan proses core yang sungguhan — kalau registrasi `tauri-plugin-global-shortcut` ikut jalan apa adanya, F8/F9 beneran ke-claim sebagai global hotkey di OS selama test berjalan. Core wajib baca env flag (misal `E2E_TEST_MODE=true`) — kalau true, skip registrasi hotkey global yang sungguhan, expose jalur test-only untuk WebdriverIO memanggil handler F8/F9 langsung. **Mekanisme eksak (cara env flag dibaca & jalur test-only di-expose) perlu diverifikasi saat POC/implementasi awal** — belum ada referensi baku di ekosistem Tauri untuk pola ini.
- **Keamanan Endpoint Override & E2E**: `is_e2e_test_mode()` dan endpoint override (`GROQ_API_URL`/`OPENROUTER_API_URL`) WAJIB di-gate ganda `#[cfg(debug_assertions)]` (compile-time) DAN runtime check. Di build release (`not(debug_assertions)`), fungsi ini ter-compile out menjadi hardcoded URL resmi dan return `false` tanpa jejak kode pembaca env var sama sekali. Kalau cuma runtime check, binary release tetap bisa di-redirect lewat env var, membocorkan Authorization header (API key asli) ke server pihak ketiga. Ini bukan defense-in-depth opsional, ini penutup jalur eksfiltrasi key nyata.
- **E2E untuk Q&A pipeline WAJIB mock Groq & OpenRouter di level HTTP client (`reqwest`)** — jangan pernah hit API asli di test otomatis. Yang diverifikasi adalah wiring-nya (request terkirim dengan payload benar, event `transcript:result`/`answer:result`/`qa:error` ke-trigger sesuai skenario mock), bukan kualitas jawaban AI beneran. Non-deterministic, berbiaya nyata tiap run, dan gagal palsu kalau provider lagi down.
- **`set_content_protected` (stealth capture-exclusion) tidak bisa diverifikasi otomatis** — butuh benar-benar direkam software capture eksternal untuk dicek. Manual verification item (§8), bukan bagian test otomatis/CI.
- Lint + type-check/clippy + test wajib lolos tiap perubahan kode, di tahap manapun sedang dikerjakan. Test gagal → boleh coba perbaiki maksimal **2x percobaan**. Masih gagal setelah itu — STOP, laporkan ke user, jangan lanjut ke langkah berikutnya, jangan update `done.md`.

## 7. Kebijakan E2E Wajib untuk Fitur Penting

E2E wajib dijalankan begitu 1 fitur/mode berikut selesai penuh atau mengalami perubahan:
- Stealth capture-exclusion & window lifecycle (tray, toggle Normal/Concealed, `windowBounds` restore) — termasuk skenario registrasi `tauri-plugin-global-shortcut` gagal, baik untuk F9 (tray "Show/Hide Overlay" harus tetap jalan) **maupun untuk F8 secara independen** (Q&A dinonaktifkan dengan warning tray, Notes mode/F9/tray tetap jalan penuh — lihat TDD §4)
- Push-to-talk Q&A pipeline (F8 → STT → AI → render jawaban, termasuk state discard-old-request) — termasuk mic permission ditolak & hold di bawah threshold (~300–500ms); Groq/OpenRouter wajib di-mock, lihat §6
- Notes/file-watcher (pilih file → render → auto-reload saat file di-save)
- Config load/save — termasuk penanganan `config.json` malformed

Perubahan kosmetik murni (styling, font size button) **tidak** wajib E2E — cukup unit/component test sesuai §6.

## 8. Kebijakan Verifikasi Visual & Manual

- Review visual (tampilan overlay, tab Notes/Q&A) dilakukan user + Antigravity secara lokal (jalankan app, screenshot manual kalau perlu) — **bukan** dikirim ke sesi Claude chat terpisah.
- Review visual manual HANYA diminta 1x per fitur (di akhir) — bukan di setiap tahap kecil. Pengecualian: per-tahap kalau tahap itu menetapkan referensi visual baru yang akan dikunci, atau ada perubahan visual signifikan.
- **Manual-only verification** (tidak bisa/tidak layak diotomasi, tetap wajib dicek user sebelum fitur ditandai selesai):
  - `set_content_protected`/`SetWindowDisplayAffinity` benar-benar exclude dari capture (rekam sekali pakai OBS/tool sejenis) — **wajib diverifikasi khusus di state Normal (on-screen), bukan cuma Concealed** (off-screen tidak membuktikan apa-apa soal affinity, lihat §0)
  - Physical keypress F8/F9 sungguhan (bukan simulasi callback) berfungsi sebagai global hotkey
  - Mic capture asli menangkap audio dengan benar (bukan cuma permission-check lolos)
  - **Native file dialog (fitur Notes mode) juga tidak muncul di capture** saat fitur file-picker diimplementasi — verifikasi watcher thread (TDD §3) benar-benar menjangkau HWND dialog, bukan cuma window utama

## 9. Update done.md

Setelah 1 langkah kecil selesai, test lolos, DAN user sudah approve — tambahkan entry ke `workflow/done.md`:
```
- [x] <nama fitur/langkah> — selesai, spec: workflow/<nama>.md
  Catatan: <penyimpangan dari spec jika ada, atau "sesuai spec">
```

## 10. Konvensi Khusus Project Ini

- **Penamaan command**: `snake_case` sesuai konvensi Rust (contoh: `read_config`, `send_audio_blob`), dipanggil dari frontend lewat nama string yang sama persis.
- **Penamaan event**: pola `domain:action` (contoh: `transcript:result`, `notes:update`, `qa:error`).
- Daftar lengkap command + event + arah + payload shape ada di `docs/api-contract.md` — itu satu-satunya sumber kebenaran, jangan menebak nama sendiri.
- **Error handling seragam**: semua kegagalan (STT, AI, baca config, permission mic, registrasi hotkey gagal) tampil sebagai tray notification yang jelas — bukan gagal senyap.
- **API key/secret cuma boleh disentuh di core (Rust)** — frontend tidak pernah menerima atau meneruskan `openrouterApiKey`/`groqApiKey` dalam bentuk apapun.
- **Spec immutability**: begitu `workflow/<fitur>.md` ditulis dan Antigravity mulai kerja dari situ, jangan diedit diam-diam kalau scope berubah di tengah jalan — balik ke chat dulu buat update spec-nya.

## 11. Larangan

- JANGAN buat requirement/kode yang tidak eksplisit ada di `docs/` tanpa konfirmasi user.
- JANGAN ubah dokumen acuan (§1) atau spec `workflow/<nama_fitur>.md` yang sudah disepakati tanpa approval eksplisit user.
- JANGAN menambah dependency/crate baru tanpa alasan & konfirmasi eksplisit.
- JANGAN pakai `any` di TypeScript atau `.unwrap()`/`.expect()` sembarangan di Rust production path (§5).
- JANGAN bikin format command/event baru di luar konvensi §10.
- JANGAN simulasikan physical global keypress asli di test otomatis (§6).
- JANGAN hit Groq/OpenRouter API asli di E2E test otomatis (§6) — wajib mock.
- JANGAN biarkan bootstrap E2E meregistrasi hotkey global yang sungguhan ke OS (§6) — wajib skip via env flag, mekanismenya diverifikasi saat POC/implementasi awal.
- JANGAN mulai feature loop implementasi (command pertama, di sisi core maupun frontend) sebelum `docs/api-contract.md` selesai dibuat dan disetujui user.

## 12. Tooling Pendukung

- **rust-analyzer**: LSP wajib di editor manapun — autocomplete, inline type hint, error real-time. Tanpa ini, error baru ketauan pas `cargo build`, jauh lebih lambat buat yang baru di Rust.
- **MSVC Build Tools** (Visual Studio Installer → workload "Desktop development with C++"): prasyarat wajib untuk compile Rust di Windows — bukan fallback opsional. **Install sebelum POC pertama (§0)**, supaya tidak keblokir di tengah jalan.
- **`anyhow` + `thiserror`**: masuk dependency list dari awal (§10 TDD), bukan ditambahkan belakangan — pattern standar buat menghindari boilerplate custom error type di tiap fungsi Rust.
- **`tauri-specta`**: generate `bindings.ts` dari command & event Rust — mekanisme konkret supaya "satu sumber kebenaran tipe" di §5 benar-benar terjaga, bukan cuma niat baik. Masuk dependency list dari awal (§10 TDD), supaya `docs/api-contract.md` bisa langsung menunjuk ke file yang di-generate ini.
- **Logging, bukan Sentry**: tool ini single-user, jalan lokal — nggak perlu SaaS cloud + akun + DSN config. Log lokal, rotate harian, ditulis ke direktori data aplikasi — bisa dicek setelah stream selesai tanpa overhead cloud.
- **GitNexus** (kalau tersedia via MCP): dipakai buat impact analysis sebelum edit symbol/fungsi, blast-radius check sebelum refactor — tooling investigasi buat Antigravity, bukan dependency yang di-ship ke app.
- **GitHub Actions CI**: lint/clippy + type-check + test (§5, §6) wajib jalan otomatis tiap push, sebagai verifikasi independen di luar klaim self-report Antigravity di format Laporan (§3).
- **GitHub Dependabot**: enable native untuk flag dependency/crate yang punya vulnerability atau ketinggalan versi.
- **Gotcha dev-loop yang diketahui**: proses `tauri dev` lama yang belum ke-terminate sempurna bisa nahan port Vite (1420) dan binary `.exe` sebelumnya, bikin re-compile gagal dengan error resource lock. Kalau ini kejadian, cek & kill proses lama (PID terkait) sebelum re-run — bukan tanda bug di kode.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **overlay-screen-tools** (530 symbols, 1051 relationships, 44 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> Index stale? Run `node .gitnexus/run.cjs analyze` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? `npx gitnexus analyze` (npm 11 crash → `npm i -g gitnexus`; #1939).

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows. For regression review, compare against the default branch: `detect_changes({scope: "compare", base_ref: "main"})`.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `query({search_query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `context({name: "symbolName"})`.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method without first running `impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit changes without running `detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/overlay-screen-tools/context` | Codebase overview, check index freshness |
| `gitnexus://repo/overlay-screen-tools/clusters` | All functional areas |
| `gitnexus://repo/overlay-screen-tools/processes` | All execution flows |
| `gitnexus://repo/overlay-screen-tools/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
