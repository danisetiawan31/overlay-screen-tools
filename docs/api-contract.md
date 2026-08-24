# api-contract.md — Screen Overlay Tool

Kontrak lengkap command & event antara Tauri core (Rust) dan webview (frontend). Ini satu-satunya sumber kebenaran penamaan — lihat AGENTS.md §10 dan §11. Dokumen ini jadi acuan manusia; `tauri-specta` men-generate `bindings.ts` dari signature Rust yang sama, jadi tipe di frontend tidak pernah ditulis manual terpisah (lihat TDD §2).

Konvensi (dari AGENTS.md §10): command `snake_case`, event pola `domain:action`. Semua command dipanggil frontend → core (request-response). Semua event dikirim core → frontend (satu arah).

## 1. Commands

| Command | Payload masuk | Return | Dipanggil kapan |
|---|---|---|---|
| `get_app_state` | — | `Result<{ fontSize: number, qaAvailable: boolean }, string>` | Sekali saat frontend startup |
| `get_notes_state` | — | `Result<{ content: string \| null, error: string \| null }, string>` | Sekali saat komponen Notes mount (mengatasi race condition auto-restore startup) |
| `update_font_size` | `{ size: number }` | `Result<(), string>` | User klik tombol +/− font size (validasi range: 10–32 px, default: 14 px) |
| `pick_notes_file` | — | `Result<{ path: string } \| null, string>` (`null` = user cancel dialog) | User klik tombol pilih file di tab Notes |
| `send_audio_blob` | `{ bytes: number[] }` | `Result<(), string>` | Setelah F8 dilepas, kalau `qa:recording-ended.belowThreshold == false` |

**Catatan `get_app_state`**: `qaAvailable` ditentukan sekali saat `setup()` Rust (berhasil/gagal registrasi F8 lewat `tauri-plugin-global-shortcut`) — nilainya stabil sepanjang sesi app berjalan, tidak berubah setelah startup. Kalau `false`, frontend menonaktifkan tab Q&A secara permanen untuk sesi itu (graceful degradation, lihat TDD §4).

Semua command mengembalikan tipe `Result<T, String>` di Rust agar frontend memiliki penanganan error generik via `.catch()` jika terjadi kegagalan sistem tak terduga — di luar event-event spesifik yang sudah didefinisikan (§2).

## 2. Events

| Event | Payload | Kapan di-emit |
|---|---|---|
| `notes:update` | `{ content: string }` | Setelah `pick_notes_file` berhasil pilih file, DAN setiap kali file watcher (`notify`) deteksi file itu di-save |
| `notes:error` | `{ message: string }` | File watcher atau pembacaan file notes gagal (file terhapus, dipindah, atau permission berubah). Dikirim BERSAMAAN dengan tray notification native agar frontend menampilkan banner peringatan persisten di tab Notes |
| `qa:recording-started` | — | F8 di-tekan (`ShortcutState::Pressed`) — frontend mulai `MediaRecorder`, auto-switch ke tab Q&A |
| `qa:recording-ended` | `{ belowThreshold: boolean }` | F8 dilepas (`ShortcutState::Released`). `belowThreshold: true` kalau durasi tahan < 300–500ms (frontend diam-diam buang recording, tidak kirim apa-apa); `false` kalau valid (frontend stop `MediaRecorder`, kirim blob lewat `send_audio_blob`) |
| `transcript:result` | `{ text: string }` | STT (Groq) berhasil |
| `answer:result` | `{ text: string }` | AI (OpenRouter) berhasil |
| `qa:error` | `{ stage: "stt" \| "ai", message: string }` | STT atau AI gagal |
| `config:error` | `{ message: string }` | `config.json` tidak ada/kosong/malformed saat startup — dikirim bersamaan dengan tray notification native (§7 TDD), bukan pengganti |

**Catatan ordering (Q&A)**: kalau F8 ditekan lagi sebelum `transcript:result`/`answer:result`/`qa:error` dari request sebelumnya sempat di-emit, core WAJIB membuang hasil request lama begitu request baru mulai — frontend dijamin cuma pernah terima event dari request TERBARU. Frontend tidak perlu logic pembeda "request mana ini" — overwrite state apa adanya tiap event masuk (lihat TDD §5, state Q&A adalah single object).

Mekanisme: tiap Q&A request dapet nomor generasi (counter incrementing) di core saat F8 ditekan. Sebelum emit `transcript:result`/`answer:result`/`qa:error` manapun, core cek generasi request itu masih generasi TERKINI — kalau bukan (udah kesusul F8 baru), event dibuang di core, TIDAK di-emit sama sekali (bukan di-emit lalu diabaikan frontend).

## 3. Bentuk Data (payload shapes)

Didefinisikan sekali di Rust (`serde` + `specta::Type` derive), digenerate ke `bindings.ts` oleh `tauri-specta` — representasi di bawah ini untuk referensi manusia, bukan untuk ditulis ulang manual di frontend:

```rust
#[derive(serde::Serialize, specta::Type)]
struct AppState {
    font_size: u32,
    qa_available: bool,
}

#[derive(serde::Serialize, specta::Type)]
struct NotesState {
    content: Option<String>,
    error: Option<String>,
}

#[derive(serde::Serialize, specta::Type)]
struct PickNotesFileResponse {
    path: String,
}

#[derive(serde::Deserialize, specta::Type)]
struct SendAudioBlobArgs {
    bytes: Vec<u8>,
}

#[derive(serde::Serialize, specta::Type)]
struct NotesUpdatePayload {
    content: String,
}

#[derive(serde::Serialize, specta::Type)]
struct NotesErrorPayload {
    message: String,
}

#[derive(serde::Serialize, specta::Type)]
struct RecordingEndedPayload {
    below_threshold: bool,
}

#[derive(serde::Serialize, specta::Type)]
struct TranscriptResultPayload {
    text: String,
}

#[derive(serde::Serialize, specta::Type)]
struct AnswerResultPayload {
    text: String,
}

#[derive(serde::Serialize, specta::Type)]
struct QaErrorPayload {
    stage: String, // "stt" | "ai"
    message: String,
}
```

## 4. Yang Sengaja Tidak Ada di Kontrak Ini

- **Window/tray control (toggle visibility, Quit)**: sepenuhnya internal Rust (hotkey F9 dan klik menu tray manggil fungsi toggle yang sama) — tidak ada command/event ke frontend, karena frontend tidak perlu bereaksi terhadap perubahan Normal↔Concealed.
- **windowBounds saving (x, y, width, height)**: ditangkap dari window-move dan window-resize event native Rust, tidak lewat frontend.
- **API key (OpenRouter/Groq)**: tidak pernah ada di payload manapun ke arah frontend — dibaca dari `config.json` cuma di sisi core (lihat TDD §2, prinsip keamanan).
- **Mic permission check**: `navigator.mediaDevices.getUserMedia()` di frontend, TIDAK ada command Tauri terpisah untuk ini — dengan syarat: hasil investigasi TDD §5 soal capability Tauri v2 mengonfirmasi cukup permission prompt bawaan webview, tanpa capability tambahan. Kalau ternyata butuh capability eksplisit, itu config `src-tauri/capabilities/*.json` (bukan command/event baru), jadi baris ini di api-contract.md tetap valid — tinggal update TDD §5 begitu keputusannya konkret.
