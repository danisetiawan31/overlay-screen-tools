# Live Q&A Mode

**Baca `AGENTS.md` di root project dulu sebelum mulai kerja.**

## Konteks & tujuan

Saat live streaming, user memparafrase pertanyaan viewer lalu menekan-tahan F8 (push-to-talk). Audio ditranskripsi (Groq STT), teksnya dikirim ke AI (OpenRouter), dan jawabannya tampil di overlay tanpa ikut ter-capture layar. Prasyarat: task addendum "Tray icon recording indicator" sudah selesai lebih dulu.

## Requirement

1. Command `send_audio_blob` (api-contract §1) memanggil Groq STT (`whisper-large-v3-turbo`) via `reqwest` multipart, format audio `webm` langsung tanpa transcoding.
2. Setelah transkrip didapat, core lanjut kirim teks itu ke OpenRouter untuk dapat jawaban — sekuensial (AI butuh teks transkrip), tapi non-blocking terhadap UI (`transcript:result` di-emit duluan sebelum AI selesai).
3. Event yang wajib di-emit sesuai api-contract §2: `qa:recording-started`, `qa:recording-ended`, `transcript:result`, `answer:result`, `qa:error`.
4. **Fallback API key** (Groq & OpenRouter, keduanya): baca `groqApiKeys`/`openrouterApiKeys` (array, index 0 = primary) dari config. Coba index 0 dulu; kalau gagal dengan error transient (network timeout/connection error, HTTP 401/403/429/5xx), coba index berikutnya; kalau gagal dengan error non-transient (400/404/422), STOP — jangan coba key lain, langsung propagate sebagai `qa:error`. Semua key habis & tetap gagal → `qa:error` dengan message dari percobaan terakhir.
5. **Discard stale request** (api-contract §2, catatan ordering): tiap F8 press dapat nomor generasi. Sebelum emit `transcript:result`/`answer:result`/`qa:error`, core cek generasi masih terkini — kalau tidak, event dibuang di core, tidak di-emit sama sekali.
6. Minimum hold duration (~400ms, sudah diimplementasikan di fitur hotkey existing) — tidak perlu disentuh ulang, tinggal pastikan frontend cuma invoke `send_audio_blob` kalau `qa:recording-ended.belowThreshold == false`.
7. Mic permission check: dilakukan di frontend (`getUserMedia`), BUKAN command Tauri terpisah (api-contract §4). Kalau ditolak, tampilkan error lokal di UI tanpa pernah invoke `send_audio_blob`.
8. State Q&A frontend: single object di-overwrite — `{ question: string | null, answer: string | null, status: 'idle' | 'recording' | 'transcribing' | 'answering' | 'error', error: string | null }` — bukan array/history.
9. Config schema: ganti `groqApiKey`/`openrouterApiKey` (string) jadi `groqApiKeys`/`openrouterApiKeys` (array of string). Kalau array kosong/tidak ada saat startup, tampilkan error jelas via tray notification (perluasan dari mekanisme existing TDD §7, BUKAN mengubah semantik `qaAvailable` di api-contract §1 yang tetap murni soal status F8).

## Tahapan implementasi

- **Tahap 1 (Skema & data layer)**: update `Config` struct (array key), validasi startup (key kosong → tray notif), command `send_audio_blob`, generic helper fallback-key untuk Groq & OpenRouter call, generation counter + discard-stale logic, semua event core-side (`transcript:result`, `answer:result`, `qa:error`).
- **Tahap 2 (UI)**: tab Q&A — state single-object di atas, listener tiap event, tampilan transkrip non-blocking lalu jawaban, banner error, mic permission check + local error state (tidak lewat core).
- **Tahap 3 (Test)**: lihat section Testing.

## Skema/struktur data

Lihat "Update dokumen" — `config.json`: `groqApiKeys: string[]`, `openrouterApiKeys: string[]` (index 0 = primary, dicoba pertama).

## Edge case yang perlu dihandle

- Semua key (Groq atau OpenRouter) habis dicoba, tetap gagal → `qa:error` dengan message percobaan terakhir.
- Error non-transient (400/404/422) → langsung `qa:error`, JANGAN lanjut ke key berikutnya (retry di sini cuma menunda pesan error yang benar ke user, hasilnya bakal gagal identik).
- Array key kosong untuk salah satu provider → tray notification saat startup (bukan `qaAvailable = false`).
- F8 ditekan lagi sebelum request sebelumnya selesai → request lama di-discard via generation counter, tidak di-queue.
- Hold F8 < ~400ms → `belowThreshold: true`, frontend tidak invoke `send_audio_blob` sama sekali.
- Mic permission ditolak browser → local error state di frontend, tidak pernah menyentuh core/command.

## Testing

- `cargo test`: fallback logic (mock `reqwest` — key pertama gagal 429, key kedua sukses; key pertama gagal 400 → tidak lanjut ke key kedua), parsing config array kosong/valid, generation counter discard-stale.
- Vitest + `@testing-library/react`: transisi state Q&A tab (idle → recording → transcribing → answering → idle/error), rendering banner error.
- E2E (WebdriverIO, mock Groq/OpenRouter di level `reqwest` — JANGAN hit API asli, lihat AGENTS §6): full pipeline F8→transcript→answer, skenario discard-stale-request (F8 ditekan 2x cepat), skenario hold di bawah threshold, skenario mic permission ditolak, skenario semua fallback key habis.

## Kriteria selesai

Semua tahap di atas selesai, test lolos (`cargo clippy`, `tsc --noEmit`, `cargo test`, Vitest, E2E), `done.md` diupdate SATU entry setelah seluruh fitur selesai (bukan per tahap), dan user sudah verifikasi manual: physical F8 asli berfungsi, mic capture asli menangkap audio, jawaban AI tampil benar di overlay saat live testing.
