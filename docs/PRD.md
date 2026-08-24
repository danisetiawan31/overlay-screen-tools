# Screen Overlay Tool — PRD

## 1. Overview / Problem Statement
User membuat konten YouTube (presentasi maupun live streaming) dan butuh referensi on-screen — talking points, syntax pemrograman — tanpa materi tersebut ikut muncul di screen share atau hasil rekaman. Teleprompter biasa tidak menyelesaikan masalah ini: kontennya selalu ikut ter-capture oleh apapun yang merekam layar.

Tool ini adalah overlay window desktop di Windows yang memanfaatkan screen capture exclusion di level OS (`SetWindowDisplayAffinity` dengan flag `WDA_EXCLUDEFROMCAPTURE`), sehingga konten di overlay hanya terlihat di layar fisik user — tidak pernah muncul di screen share atau recording. Selain notes statis, tool ini juga punya live Q&A mode: saat live streaming, ketika viewer bertanya, user memparafrase pertanyaan jadi bentuk yang bersih, mengucapkannya lewat push-to-talk hotkey, lalu tool mentranskripsikannya dan mengembalikan jawaban dari AI untuk direferensikan user saat menjawab di depan kamera.

Ini adalah tool single-user, personal, untuk satu orang di satu laptop Windows 11 — bukan produk multi-user. Tidak perlu akun, autentikasi, atau multi-tenancy.

## 2. Goals & Non-Goals

**Goals**
- Menampilkan konten referensi (notes, code snippet) di layar selama recording/live streaming tanpa ikut muncul di hasil capture
- Menyediakan akses cepat dan low-friction ke jawaban AI untuk pertanyaan viewer secara live, tanpa memaksa user kehilangan fokus dari layar/aplikasi utamanya
- Menjaga tool tetap ringan dan scope-nya personal use saja

**Non-Goals**
- Bukan produk multi-user atau cloud-hosted
- Tidak dirancang untuk mengatasi physical/photographic capture terhadap layar (misal kamera atau HDMI capture device yang diarahkan ke monitor) — lihat Non-Functional Requirements
- Bukan sistem automatic live-chat-reading di versi ini (lihat Out of Scope)

## 3. User Flows

### 3.1 Notes/Teleprompter Mode
1. User memilih file `.md` dari sebuah folder lewat overlay
2. Overlay me-render isi Markdown file tersebut — bullet points, headings, dan code block dengan syntax highlighting
3. Overlay auto-sync dan re-render setiap kali file yang dipilih di-save (file watcher)
4. Sebagai fallback, user bisa pakai tombol "Edit Langsung" (quick scratchpad) di overlay buat mencatat sesuatu secara ad-hoc, tanpa perlu buka file terpisah

### 3.2 Live Q&A Mode
1. Viewer bertanya saat live streaming berlangsung
2. User memparafrase pertanyaan tersebut jadi bentuk yang jelas dan ringkas
3. User menekan dan menahan global hotkey (push-to-talk), mengucapkan pertanyaan yang sudah diparafrase, lalu melepas hotkey untuk berhenti merekam
4. Menekan hotkey otomatis memindahkan tampilan overlay ke tab Q&A
5. Audio yang direkam ditranskripsi lewat speech-to-text; teks hasil transkripsi langsung tampil di overlay (non-blocking) sementara request ke AI dikirim secara paralel
6. AI (via OpenRouter) mengembalikan jawaban, yang ditampilkan di overlay
7. Hanya pasangan pertanyaan/jawaban terbaru yang ditampilkan — belum ada scrollable history di versi ini
8. Kalau STT atau request AI gagal setelah seluruh API key (primary & fallback) dicoba, overlay menampilkan pesan error yang jelas dan ringkas, bukan gagal secara senyap

Ada global hotkey terpisah untuk toggle visibility seluruh overlay (show/hide), independen dari hotkey Q&A. Overlay bersifat always-on-top secara default dan bisa di-drag untuk reposisi.

### 3.3 App Lifecycle
Overlay diakses lewat system tray icon. Klik kanan tray icon menampilkan menu dengan dua opsi: "Show/Hide Overlay" (fallback manual untuk toggle visibility, independen dari hotkey) dan "Quit" untuk keluar dari aplikasi. Tray icon juga menampilkan indikator sederhana saat sedang merekam audio (push-to-talk aktif), supaya user tahu status recording tanpa harus melihat overlay langsung.

## 4. Features & Requirements
- Overlay window desktop (Windows 11) yang dikecualikan dari screen capture/recording lewat `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`
- Dua mode tampilan yang bisa di-switch: Notes/Teleprompter dan Live Q&A
- Notes mode: file picker untuk `.md`, auto-sync berbasis file watcher, rendering Markdown (list, heading, code block dengan syntax highlighting), plus scratchpad quick-edit di dalam aplikasi
- Q&A mode: global push-to-talk hotkey, transkripsi speech-to-text (Groq), request AI via OpenRouter, fallback API key untuk resilience (detail teknis di TDD §5), tampilan non-blocking untuk teks transkripsi, tampilan jawaban AI terbaru saja
- Global hotkey terpisah untuk toggle visibility overlay
- Overlay always-on-top dan bisa di-drag ke posisi manapun di layar
- Kontrol ukuran font (tombol +/− di dalam overlay) untuk menyesuaikan keterbacaan teks Notes/Q&A
- System tray icon dengan menu Show/Hide Overlay dan Quit — satu-satunya cara menutup aplikasi (window tidak memiliki titlebar/tombol close bawaan OS), sekaligus fallback manual kalau hotkey toggle visibility gagal aktif

## 5. Non-Functional Requirements
- **Platform**: Windows 11 saja (bergantung pada `WDA_EXCLUDEFROMCAPTURE`, didukung sejak Windows 10 versi 2004+)
- **Latency**: response AI/STT diharapkan secepat mungkin secara wajar; belum ada angka target pasti — menunggu pemilihan STT engine spesifik, akan didefinisikan di TDD
- **Batasan reliability**: capture exclusion adalah fitur compositor-level di OS, bukan jaminan security/DRM. Tidak akan menyembunyikan overlay dari kamera fisik atau HDMI capture device yang diarahkan ke layar. Ini batasan yang diterima, bukan requirement yang perlu diselesaikan.
- **Error handling**: kegagalan di titik manapun (STT, AI, baca config, permission mic) wajib menampilkan pesan error yang jelas dan terlihat, bukan diam tanpa indikasi
- **Graceful degradation**: kalau modul untuk global hotkey Q&A (native keyboard hook) gagal dimuat, cukup fitur push-to-talk/Q&A yang nonaktif dengan warning jelas — Notes mode, toggle visibility, dan tray tetap harus berjalan penuh
- **Single-user**: tidak perlu autentikasi atau multi-tenancy

## 6. Out of Scope
Ditunda ke `backlog.md` (bukan dihapus — bisa dipertimbangkan lagi nanti):
- Automatic viewer-question capture dari platform live chat (misal YouTube Live Chat API), sebagai alternatif/pelengkap dari alur manual paraphrase-and-speak
- Scrollable/persistent history pasangan Q&A dalam satu sesi
- Dukungan platform selain Windows 11 (misal macOS)
- Ghost Mode (click-through overlay, mouse tembus ke aplikasi di baliknya): dipertimbangkan saat TDD tapi di-defer untuk menjaga scope tetap ringan sesuai Goals — kombinasi drag-to-reposition + toggle visibility (F9) dianggap cukup untuk versi ini. Bisa dipertimbangkan lagi kalau ternyata kurang di pemakaian nyata
- Packaging jadi installer `.exe`: dijalankan lewat perintah dev (`electron-vite dev`/`preview`) untuk versi ini; dipertimbangkan lagi kalau kebutuhan berubah (misal jadi demo portfolio)

## 7. Success Criteria
- User bisa melihat konten notes/script di overlay selama live screen share atau recording, dan overlay tidak pernah muncul di hasil capture
- User bisa memicu alur Q&A lewat hotkey selama sesi live, menerima jawaban AI untuk pertanyaan viewer yang sudah diparafrase, dan mereferensikannya tanpa overlay ikut ter-expose di stream
- Tool berjalan reliable sebagai personal utility di laptop Windows 11 milik user sendiri
