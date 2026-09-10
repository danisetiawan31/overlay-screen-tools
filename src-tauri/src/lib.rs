pub mod clipboard;
pub mod commands;
pub mod config;
pub mod screen;
pub mod vault;

use config::{Config, ConfigErrorPayload};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, PhysicalPosition};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use tauri_specta::{collect_commands, collect_events, Builder};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, EnumChildWindows, GetWindowLongPtrW, IsWindow, SetWindowDisplayAffinity,
    SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_ASYNCWINDOWPOS, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;

/// Identifier unik untuk system tray icon utama aplikasi.
pub const MAIN_TRAY_ID: &str = "main-tray";
/// Tooltip default untuk tray icon saat status idle / normal.
pub const DEFAULT_TRAY_TOOLTIP: &str = "Screen Overlay Tool";
/// Tooltip tray icon saat hotkey push-to-talk F8 sedang merekam audio (PRD §3.3).
pub const RECORDING_TRAY_TOOLTIP: &str = "Screen Overlay Tool \u{2014} Recording...";

/// Memperbarui tooltip tray icon untuk menandakan status recording audio secara visual (PRD §3.3).
pub fn update_tray_recording_state(app: &tauri::AppHandle, is_recording: bool) {
    if let Some(tray) = app.tray_by_id(MAIN_TRAY_ID) {
        let tooltip = if is_recording {
            RECORDING_TRAY_TOOLTIP
        } else {
            DEFAULT_TRAY_TOOLTIP
        };
        if let Err(err) = tray.set_tooltip(Some(tooltip)) {
            eprintln!("[TRAY ERROR] Gagal mengupdate tooltip tray: {}", err);
        }
    }
}

/// Ambang batas durasi tahan minimum F8 (ms) untuk membedakan antara klik tak sengaja dan push-to-talk yang valid.
pub const PUSH_TO_TALK_MIN_HOLD_MS: u64 = 400;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "qa:recording-ended")]
pub struct RecordingEndedPayload {
    pub below_threshold: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "notes:update")]
pub struct NotesUpdatePayload {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "notes:error")]
pub struct NotesErrorPayload {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "transcript:result")]
pub struct TranscriptResultPayload {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "answer:result")]
pub struct AnswerResultPayload {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "qa:error")]
pub struct QaErrorPayload {
    pub stage: String,
    pub message: String,
}

unsafe extern "system" fn enum_child_proc(hwnd: HWND, _lparam: LPARAM) -> i32 {
    // SAFETY: HWND di-pass langsung oleh sistem Windows saat EnumChildWindows berjalan.
    unsafe {
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    }
    1
}

const GWLP_HWNDPARENT: i32 = -8;
static DUMMY_OWNER_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

fn get_or_create_dummy_owner() -> HWND {
    let current = DUMMY_OWNER_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if current != 0 {
        return current as HWND;
    }

    // SAFETY: STATIC window class valid bawaan Windows Win32 API, dibuat tersembunyi sebagai owner HWND.
    unsafe {
        let class_name: [u16; 7] = [
            'S' as u16, 'T' as u16, 'A' as u16, 'T' as u16, 'I' as u16, 'C' as u16, 0,
        ];
        let dummy = CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            0 as HWND,
            0 as _,
            0 as _,
            std::ptr::null(),
        );
        DUMMY_OWNER_HWND.store(dummy as isize, std::sync::atomic::Ordering::SeqCst);
        dummy
    }
}

fn apply_stealth(hwnd_val: isize) {
    let hwnd = hwnd_val as HWND;
    // SAFETY: HWND valid didapatkan dari WebViewWindow aktif di Tauri.
    unsafe {
        let res = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        println!(
            "[STEALTH] SetWindowDisplayAffinity main HWND ({:?}): result={}",
            hwnd, res
        );
        EnumChildWindows(hwnd, Some(enum_child_proc), 0);
        println!("[STEALTH] Applied WDA_EXCLUDEFROMCAPTURE to all child WebViews");

        // Jadikan dummy hidden window sebagai owner agar tersembunyi dari Alt+Tab
        // tanpa memakai WS_EX_TOOLWINDOW (yang membuat tombol 'X' dan titlebar mengecil)
        let dummy = get_or_create_dummy_owner();
        if dummy != (0 as HWND) {
            SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, dummy as isize);
        }

        // Hapus WS_EX_TOOLWINDOW (mengembalikan ukuran tombol X dan titlebar normal),
        // hapus WS_EX_APPWINDOW (memastikan tidak muncul di taskbar atau Alt+Tab),
        // dan pasang WS_EX_NOACTIVATE (mencegah klik mouse merebut fokus dari browser)
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let target_ex_style = (ex_style | (WS_EX_NOACTIVATE as isize))
            & !(WS_EX_TOOLWINDOW as isize)
            & !(WS_EX_APPWINDOW as isize);
        if ex_style != target_ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, target_ex_style);
            SetWindowPos(
                hwnd,
                0 as _,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
            println!("[STEALTH] Restored normal caption/close button, hidden from Alt+Tab via owner window, and applied WS_EX_NOACTIVATE");
        }
    }
}

pub struct OverlayState {
    pub is_concealed: bool,
    pub last_normal_position: PhysicalPosition<i32>,
    pub hwnd: isize,
    pub config: Option<Config>,
    pub qa_available: bool,
    pub is_recording: bool,
    pub recording_start_instant: Option<std::time::Instant>,
    pub notes_watcher: Option<notify::RecommendedWatcher>,
    pub watched_directories: std::collections::HashSet<PathBuf>,
    pub opened_notes: std::collections::HashMap<String, String>,
    pub active_notes_path: Option<String>,
    pub last_notes_content: Option<String>,
    pub last_notes_error: Option<String>,
    pub qa_generation: u64,
    pub active_dialog_hwnd: Option<isize>,
}

impl OverlayState {
    /// Membalikkan state `is_concealed` (Normal <-> Concealed) dan mengembalikan state baru.
    pub fn toggle_concealed(&mut self) -> bool {
        self.is_concealed = !self.is_concealed;
        self.is_concealed
    }
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            is_concealed: false,
            last_normal_position: PhysicalPosition::new(100, 100),
            hwnd: 0,
            config: None,
            qa_available: true,
            is_recording: false,
            recording_start_instant: None,
            notes_watcher: None,
            watched_directories: std::collections::HashSet::new(),
            opened_notes: std::collections::HashMap::new(),
            active_notes_path: None,
            last_notes_content: None,
            last_notes_error: None,
            qa_generation: 0,
            active_dialog_hwnd: None,
        }
    }
}

/// Helper murni untuk mengecek string env var E2E_TEST_MODE (hanya "true" case-insensitive).
pub fn parse_e2e_test_mode_from_str(val: Option<&str>) -> bool {
    val.map(|v| v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Memeriksa apakah aplikasi dijalankan dalam mode pengujian E2E (`E2E_TEST_MODE=true`).
/// WAJIB di-gate #[cfg(debug_assertions)]: pada release build selalu return false hardcoded.
#[cfg(debug_assertions)]
pub fn is_e2e_test_mode() -> bool {
    let env_val = std::env::var("E2E_TEST_MODE").ok();
    if parse_e2e_test_mode_from_str(env_val.as_deref()) {
        return true;
    }
    std::env::args().any(|arg| arg == "--e2e-test-mode" || arg == "--e2e")
}

#[cfg(not(debug_assertions))]
pub fn is_e2e_test_mode() -> bool {
    false
}

/// Mendapatkan endpoint Groq Whisper STT.
/// Pada debug_assertions & E2E_TEST_MODE=true: membaca GROQ_API_URL atau CLI arg untuk mock HTTP server.
/// Pada release build: selalu mengembalikan URL resmi production hardcoded tanpa jejak env var.
#[cfg(debug_assertions)]
pub fn get_groq_endpoint() -> String {
    if is_e2e_test_mode() {
        if let Ok(val) = std::env::var("GROQ_API_URL") {
            return val;
        }
        for arg in std::env::args() {
            if let Some(val) = arg.strip_prefix("--groq-url=") {
                return val.to_string();
            }
        }
        "http://127.0.0.1:4545/groq".to_string()
    } else {
        "https://api.groq.com/openai/v1/audio/transcriptions".to_string()
    }
}

#[cfg(not(debug_assertions))]
pub fn get_groq_endpoint() -> String {
    "https://api.groq.com/openai/v1/audio/transcriptions".to_string()
}

/// Mendapatkan endpoint OpenRouter Chat Completion.
/// Pada debug_assertions & E2E_TEST_MODE=true: membaca OPENROUTER_API_URL atau CLI arg untuk mock HTTP server.
/// Pada release build: selalu mengembalikan URL resmi production hardcoded tanpa jejak env var.
#[cfg(debug_assertions)]
pub fn get_openrouter_endpoint() -> String {
    if is_e2e_test_mode() {
        if let Ok(val) = std::env::var("OPENROUTER_API_URL") {
            return val;
        }
        for arg in std::env::args() {
            if let Some(val) = arg.strip_prefix("--openrouter-url=") {
                return val.to_string();
            }
        }
        "http://127.0.0.1:4545/openrouter".to_string()
    } else {
        "https://openrouter.ai/api/v1/chat/completions".to_string()
    }
}

#[cfg(not(debug_assertions))]
pub fn get_openrouter_endpoint() -> String {
    "https://openrouter.ai/api/v1/chat/completions".to_string()
}

/// Memeriksa apakah durasi hold berada di bawah ambang batas minimum push-to-talk.
pub fn is_hold_duration_below_threshold(duration: std::time::Duration) -> bool {
    duration.as_millis() < PUSH_TO_TALK_MIN_HOLD_MS as u128
}

/// Logika keputusan murni saat tombol F8 ditekan (Pressed).
///
/// Mengembalikan `true` jika event start harus di-emit (tekanan pertama), serta menaikkan nomor `qa_generation`.
/// Mengembalikan `false` jika sedang recording (mengabaikan repeated Pressed akibat key-repeat OS).
pub fn handle_f8_press(
    is_recording: &mut bool,
    start_instant: &mut Option<std::time::Instant>,
    qa_generation: &mut u64,
    now: std::time::Instant,
) -> bool {
    if !*is_recording {
        *is_recording = true;
        *start_instant = Some(now);
        *qa_generation = qa_generation.wrapping_add(1);
        true
    } else {
        false
    }
}

/// Logika keputusan murni saat tombol F8 dilepas (Released).
///
/// Mengembalikan `Some(below_threshold)` jika sebelumnya sedang recording.
/// Mengembalikan `None` jika dilepas tanpa ada status recording sebelumnya (orphaned Released).
pub fn handle_f8_release(
    is_recording: &mut bool,
    start_instant: &mut Option<std::time::Instant>,
    now: std::time::Instant,
) -> Option<bool> {
    if *is_recording {
        let start = start_instant.take().unwrap_or(now);
        *is_recording = false;
        let duration = now.saturating_duration_since(start);
        Some(is_hold_duration_below_threshold(duration))
    } else {
        None
    }
}

/// Memeriksa apakah nomor generasi request Q&A masih merupakan generasi terbaru.
pub fn is_qa_generation_current(app: &tauri::AppHandle, expected_generation: u64) -> bool {
    let state = app.state::<Mutex<OverlayState>>();
    let state_guard = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    state_guard.qa_generation == expected_generation
}

/// Klasifikasi aksi penanganan error HTTP untuk fallback API key (TDD §5).
#[derive(Debug, PartialEq)]
pub enum RetryAction {
    RetryNextKey(String),
    StopNonTransient(String),
}

/// Mengklasifikasi status error HTTP:
/// - 400 / 404 / 422: Non-transient (Stop / fail-fast)
/// - 401 / 403 / 429 / 5xx / Network timeout: Transient / key-related (Retry next key)
pub fn classify_http_error(status: Option<reqwest::StatusCode>, err_text: &str) -> RetryAction {
    match status {
        Some(s) if s.as_u16() == 400 || s.as_u16() == 404 || s.as_u16() == 422 => {
            RetryAction::StopNonTransient(format!("HTTP {}: {}", s, err_text))
        }
        Some(s) => RetryAction::RetryNextKey(format!("HTTP {}: {}", s, err_text)),
        None => RetryAction::RetryNextKey(err_text.to_string()),
    }
}

/// Eksekusi generic loop fallback API key untuk Groq & OpenRouter (TDD §5).
pub async fn execute_fallback_keys<T, F, Fut>(
    keys: &[String],
    mut operation: F,
) -> Result<T, String>
where
    F: FnMut(&str) -> Fut,
    Fut: std::future::Future<Output = Result<T, (Option<reqwest::StatusCode>, String)>>,
{
    let valid_keys: Vec<&str> = keys
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if valid_keys.is_empty() {
        return Err("Tidak ada API key yang valid di config.json".to_string());
    }

    let mut last_error = String::from("Semua API key gagal");
    println!("[FALLBACK] Memulai percobaan fallback dengan {} API key terkonfigurasi.", valid_keys.len());
    for (idx, key) in valid_keys.iter().enumerate() {
        let masked = if key.len() >= 14 {
            format!("{}...{}", &key[..8], &key[key.len() - 4..])
        } else {
            "***".to_string()
        };
        println!("[FALLBACK] Mencoba key [{}/{}] ({})", idx + 1, valid_keys.len(), masked);
        match operation(key).await {
            Ok(val) => {
                println!("[FALLBACK] Key [{}/{}] ({}) BERHASIL!", idx + 1, valid_keys.len(), masked);
                return Ok(val);
            }
            Err((status, msg)) => match classify_http_error(status, &msg) {
                RetryAction::StopNonTransient(err) => {
                    eprintln!(
                        "[FALLBACK] Key [{}/{}] non-transient error: {}. Hentikan fallback.",
                        idx + 1, valid_keys.len(), err
                    );
                    return Err(err);
                }
                RetryAction::RetryNextKey(err) => {
                    eprintln!(
                        "[FALLBACK] Key [{}/{}] gagal ({:?}): {}. Beralih ke key berikutnya...",
                        idx + 1, valid_keys.len(), status, err
                    );
                    last_error = err;
                }
            },
        }
    }

    if valid_keys.len() > 1 {
        Err(format!(
            "Semua {} API key fallback gagal. Error percobaan terakhir: {}",
            valid_keys.len(),
            last_error
        ))
    } else {
        Err(last_error)
    }
}

/// Helper untuk memanggil Groq Whisper STT API via multipart form-data.
pub async fn call_groq_stt(
    client: &reqwest::Client,
    key: &str,
    audio_bytes: &[u8],
) -> Result<String, (Option<reqwest::StatusCode>, String)> {
    let part = reqwest::multipart::Part::bytes(audio_bytes.to_vec())
        .file_name("audio.webm")
        .mime_str("audio/webm")
        .map_err(|e| (None, format!("Gagal membuat form audio: {}", e)))?;

    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", "whisper-large-v3-turbo");

    let response = client
        .post(get_groq_endpoint())
        .bearer_auth(key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| (e.status(), format!("Network error Groq: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Gagal membaca body error".to_string());
        return Err((Some(status), err_body));
    }

    #[derive(Deserialize)]
    struct GroqResponse {
        text: String,
    }

    let parsed = response
        .json::<GroqResponse>()
        .await
        .map_err(|e| (Some(status), format!("Gagal parse JSON Groq: {}", e)))?;

    Ok(parsed.text)
}

/// Helper untuk memanggil OpenRouter Chat Completion API dengan dukungan konteks Obsidian Vault opsional.
pub async fn call_openrouter_ai(
    client: &reqwest::Client,
    key: &str,
    transcript: &str,
    vault_context: Option<&str>,
) -> Result<String, (Option<reqwest::StatusCode>, String)> {
    #[derive(Serialize)]
    struct Message {
        role: String,
        content: String,
    }

    #[derive(Serialize)]
    struct OpenRouterRequest {
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        models: Option<Vec<String>>,
        messages: Vec<Message>,
    }

    let core_profile = "\
### CANDIDATE CORE IDENTITY & BACKGROUND:
- **Name**: Ahmad Dhani Setiawan
- **Role**: Fullstack Software Engineer
- **Core Tech Stack**: TypeScript, JavaScript, Go, Python, PHP, Next.js, React, Angular, NestJS, Laravel, FastAPI, PostgreSQL, MySQL, Redis, Docker, Vitest, Playwright.
- **Flagship Projects**:
  1. **Klinik RME** (Go, sqlc, Angular 21, PostgreSQL, WebSocket) — Electronic Medical Record & Queue System with pessimistic row locking (`FOR UPDATE SKIP LOCKED`) and SHA-256 tamper-evident audit trail.
  2. **Attendance & Workforce Management** (NestJS, Python FastAPI, PostgreSQL) — Spoof-resistant attendance with DeepFace AI facial verification and Haversine GPS geofencing.
  3. **Vehicle Booking Management** (Laravel 11, PostgreSQL) — Fleet reservation with 2-tier state machine approval and schedule collision prevention.
  4. **Fishing Pond POS** (CodeIgniter 4, MySQL) — Point of Sale with dynamic duration billing and ESC/POS thermal printing.
- **Communication Style**: First-person ('Saya' / 'I'), concise, confident, structured (2-4 bullet points or short paragraphs), grounded in real technical facts.";

    let prompt_rules = "\
### CRITICAL ADAPTIVE RULES (DETECT QUESTION TYPE FIRST):

1. **TYPE 1: MULTIPLE CHOICE QUESTIONS (PILIHAN GANDA) — Matematika, Logika, CS Theory, Aptitude (A, B, C, D, dsb.):**
   - **LINE 1 MUST BE THE CORRECT OPTION**: Directly state the final chosen answer in bold on the very first line:
     **Jawaban: [Option Letter]. [Option Text/Value]** (contoh: **Jawaban: B. 42** atau **Jawaban: C. O(n log n)**).
   - **CONCISE CALCULATION / REASONING**: Underneath the first line, provide 2-4 brief, clear steps explaining the math calculation, formula, or logic.
   - **STRICTLY NO PROGRAMMING CODE**: DO NOT output Python/JavaScript/any programming code for math or conceptual MCQs! Give direct math steps and the answer option.

2. **TYPE 2: HANDS-ON CODING & ALGORITHM PROBLEMS (Membuat Fungsi, Implementasi, Coding Test):**
   - **CODE FIRST & END-TO-END COMPLETE**: Output the complete, working, optimal solution CODE IMMEDIATELY at the very top in a standard markdown code block.
   - **MUST INCLUDE TEST/DRIVER EXECUTION**: The code block MUST be completely end-to-end:
     * Full function / class implementation.
     * Sample input variable declaration (matching the question's example).
     * Function call and print statement (e.g. `console.log(...)`, `print(...)`, `fmt.Println(...)`) displaying the expected output so the candidate can run/verify immediately.
   - **NO CONVERSATIONAL FLUFF**: Do NOT start with phrases like 'Saya akan...', 'Berikut adalah solusinya...'. Start directly with the code block.
   - **BRIEF BULLETS BELOW**: Underneath the code block, provide at most 2-3 concise bullet points explaining key algorithm logic and time/space complexity O(...).

3. **TYPE 3: CONCEPTUAL, SYSTEM DESIGN, OR BEHAVIORAL INTERVIEW QUESTIONS:**
   - Speak in the first person ('Saya' / 'I') as the candidate using the candidate background below.
   - Provide 2-4 high-density, concise bullet points or structured explanations.

4. **LANGUAGE**: Always match the question's language (Bahasa Indonesia if in Indonesian, English if in English).";

    let system_content = if let Some(context) = vault_context {
        format!(
            "You are an expert technical copilot and problem solver assisting a candidate in a live assessment, technical test, or interview.\n\
            \n\
            {}\n\
            \n\
            {}\n\
            \n\
            --- RELEVANT NOTES FROM YOUR PERSONAL KNOWLEDGE VAULT ---\n\
            {}\n\
            --------------------------------------------------------",
            prompt_rules, core_profile, context
        )
    } else {
        format!(
            "You are an expert technical copilot and problem solver assisting a candidate in a live assessment, technical test, or interview.\n\
            \n\
            {}\n\
            \n\
            {}",
            prompt_rules, core_profile
        )
    };

    let request_payload = OpenRouterRequest {
        model: None,
        models: Some(vec![
            "nvidia/nemotron-3-super-120b-a12b:free".to_string(),
            "dots-studio/dots-3-note-preview:free".to_string(),
            "cohere/north-mini-code:free".to_string(),
        ]),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_content,
            },
            Message {
                role: "user".to_string(),
                content: transcript.to_string(),
            },
        ],
    };

    let response = client
        .post(get_openrouter_endpoint())
        .bearer_auth(key)
        .header(
            "HTTP-Referer",
            "https://github.com/danisetiawan31/overlay-screen-tools",
        )
        .header("X-Title", "Screen Overlay Tool")
        .json(&request_payload)
        .send()
        .await
        .map_err(|e| (e.status(), format!("Network error OpenRouter: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Gagal membaca body error".to_string());
        return Err((Some(status), err_body));
    }

    let raw_text = response.text().await.map_err(|e| {
        (
            Some(status),
            format!("Gagal membaca body response OpenRouter: {}", e),
        )
    })?;

    let json_val: serde_json::Value = serde_json::from_str(&raw_text)
        .map_err(|e| (Some(status), format!("Gagal parse JSON OpenRouter: {}", e)))?;

    if let Some(content) = json_val
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
    {
        Ok(content.to_string())
    } else if let Some(err_msg) = json_val
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        Err((Some(status), format!("OpenRouter error: {}", err_msg)))
    } else {
        Err((
            Some(status),
            format!(
                "OpenRouter mengembalikan response tidak terduga: {}",
                raw_text
            ),
        ))
    }
}

/// Helper untuk memanggil OpenRouter Vision AI dengan tangkapan layar (multimodal payload).
pub async fn call_openrouter_vision_ai(
    client: &reqwest::Client,
    key: &str,
    image_data_url: &str,
    vault_context: Option<&str>,
) -> Result<String, (Option<reqwest::StatusCode>, String)> {
    #[derive(Serialize)]
    #[serde(tag = "type")]
    enum ContentPart {
        #[serde(rename = "text")]
        Text { text: String },
        #[serde(rename = "image_url")]
        ImageUrl { image_url: ImageUrlObj },
    }

    #[derive(Serialize)]
    struct ImageUrlObj {
        url: String,
    }

    #[derive(Serialize)]
    #[serde(untagged)]
    enum MessageContent {
        Text(String),
        Parts(Vec<ContentPart>),
    }

    #[derive(Serialize)]
    struct VisionMessage {
        role: String,
        content: MessageContent,
    }

    #[derive(Serialize)]
    struct OpenRouterVisionRequest {
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        models: Option<Vec<String>>,
        messages: Vec<VisionMessage>,
    }

    let core_profile = "\
### CANDIDATE CORE IDENTITY & BACKGROUND:
- **Name**: Ahmad Dhani Setiawan
- **Role**: Fullstack Software Engineer
- **Core Tech Stack**: TypeScript, JavaScript, Go, Python, PHP, Next.js, React, Angular, NestJS, Laravel, FastAPI, PostgreSQL, MySQL, Redis, Docker, Vitest, Playwright.
- **Flagship Projects**:
  1. **Klinik RME** (Go, sqlc, Angular 21, PostgreSQL, WebSocket) — Electronic Medical Record & Queue System with pessimistic row locking (`FOR UPDATE SKIP LOCKED`) and SHA-256 tamper-evident audit trail.
  2. **Attendance & Workforce Management** (NestJS, Python FastAPI, PostgreSQL) — Spoof-resistant attendance with DeepFace AI facial verification and Haversine GPS geofencing.
  3. **Vehicle Booking Management** (Laravel 11, PostgreSQL) — Fleet reservation with 2-tier state machine approval and schedule collision prevention.
  4. **Fishing Pond POS** (CodeIgniter 4, MySQL) — Point of Sale with dynamic duration billing and ESC/POS thermal printing.
- **Communication Style**: First-person ('Saya' / 'I'), concise, confident, structured (2-4 bullet points or short paragraphs), grounded in real technical facts.";

    let prompt_rules = "\
### CRITICAL ADAPTIVE RULES (DETECT QUESTION TYPE FROM SCREEN FIRST):

1. **TYPE 1: MULTIPLE CHOICE QUESTIONS (PILIHAN GANDA) — Matematika, Logika, CS Theory, Aptitude (A, B, C, D, dsb.):**
   - **LINE 1 MUST BE THE CORRECT OPTION**: Directly state the final chosen answer in bold on the very first line:
     **Jawaban: [Option Letter]. [Option Text/Value]** (contoh: **Jawaban: B. 42** atau **Jawaban: C. O(n log n)**).
   - **CONCISE CALCULATION / REASONING**: Underneath the first line, provide 2-4 brief, clear steps explaining the math calculation, formula, or logic.
   - **STRICTLY NO PROGRAMMING CODE**: DO NOT output Python/JavaScript/any programming code for math or conceptual MCQs! Give direct math steps and the answer option.

2. **TYPE 2: HANDS-ON CODING & ALGORITHM PROBLEMS (Membuat Fungsi, Implementasi, Coding Test):**
   - **CODE FIRST & END-TO-END COMPLETE**: Output the complete, optimal, bug-free solution CODE IMMEDIATELY at the very top in a standard markdown code block.
   - **MUST INCLUDE TEST/DRIVER EXECUTION**: The code block MUST be completely end-to-end:
     * Full function / class implementation.
     * Sample input variable declaration (matching the question's example).
     * Function call and print statement (e.g. `console.log(...)`, `print(...)`, `fmt.Println(...)`) displaying the expected output so the candidate can run/verify immediately.
   - **NO CONVERSATIONAL FLUFF**: Do NOT start with phrases like 'Saya akan...', 'Berikut adalah kodenya...'. Start directly with the code block.
   - **BRIEF BULLETS BELOW**: Underneath the code block, provide 2-3 concise bullet points explaining key algorithm logic, edge cases, and time/space complexity.

3. **TYPE 3: CONCEPTUAL, SYSTEM DESIGN, OR BEHAVIORAL INTERVIEW QUESTIONS:**
   - Speak in the first person ('Saya' / 'I') as the candidate using the candidate background below.
   - Provide 2-4 high-density, concise bullet points or structured explanations.

4. **LANGUAGE**: Always match the question's language (Bahasa Indonesia if in Indonesian, English if in English).";

    let system_content = if let Some(context) = vault_context {
        format!(
            "You are an expert technical copilot and problem solver assisting a candidate in a live assessment or exam.\n\
            Carefully inspect the screen capture, detect the exact question type, and solve it accurately.\n\
            \n\
            {}\n\
            \n\
            {}\n\
            \n\
            --- RELEVANT NOTES FROM YOUR PERSONAL KNOWLEDGE VAULT ---\n\
            {}\n\
            --------------------------------------------------------",
            prompt_rules, core_profile, context
        )
    } else {
        format!(
            "You are an expert technical copilot and problem solver assisting a candidate in a live assessment or exam.\n\
            Carefully inspect the screen capture, detect the exact question type, and solve it accurately.\n\
            \n\
            {}\n\
            \n\
            {}",
            prompt_rules, core_profile
        )
    };

    let request_payload = OpenRouterVisionRequest {
        model: None,
        models: Some(vec![
            "nex-agi/nex-n2.5-pro:free".to_string(),
            "google/gemma-4-26b-a4b-it:free".to_string(),
            "dots-studio/dots-3-note-preview:free".to_string(),
        ]),
        messages: vec![
            VisionMessage {
                role: "system".to_string(),
                content: MessageContent::Text(system_content),
            },
            VisionMessage {
                role: "user".to_string(),
                content: MessageContent::Parts(vec![
                    ContentPart::Text {
                        text: "Tolong analisis dan selesaikan soal pada tangkapan layar ini. Jika soal pilihan ganda (MCQ) seperti matematika atau teori, langsung tentukan opsi jawaban yang benar (contoh: **Jawaban: B. 42**) beserta langkah singkatnya. Jika soal coding/pemrograman, berikan kode solusi lengkap end-to-end.".to_string(),
                    },
                    ContentPart::ImageUrl {
                        image_url: ImageUrlObj {
                            url: image_data_url.to_string(),
                        },
                    },
                ]),
            },
        ],
    };

    let response = client
        .post(get_openrouter_endpoint())
        .bearer_auth(key)
        .header(
            "HTTP-Referer",
            "https://github.com/danisetiawan31/overlay-screen-tools",
        )
        .header("X-Title", "Screen Overlay Tool")
        .json(&request_payload)
        .send()
        .await
        .map_err(|e| (e.status(), format!("Network error OpenRouter Vision: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Gagal membaca body error".to_string());
        return Err((Some(status), err_body));
    }

    let raw_text = response.text().await.map_err(|e| {
        let msg = if e.is_timeout() {
            "Waktu tunggu AI habis (>90 detik). Server model Vision OpenRouter sedang sibuk atau antre.".to_string()
        } else {
            format!("Gagal membaca body response OpenRouter Vision: {}", e)
        };
        (Some(status), msg)
    })?;

    let json_val: serde_json::Value = serde_json::from_str(&raw_text)
        .map_err(|e| (Some(status), format!("Gagal parse JSON OpenRouter Vision: {}", e)))?;

    if let Some(content) = json_val
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
    {
        Ok(content.to_string())
    } else if let Some(err_msg) = json_val
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
        Err((Some(status), format!("OpenRouter Vision error: {}", err_msg)))
    } else {
        Err((
            Some(status),
            format!(
                "OpenRouter Vision mengembalikan response tidak terduga: {}",
                raw_text
            ),
        ))
    }
}

/// Fungsi terpusat untuk memicu tangkapan layar senyap dan analisis OpenRouter Vision AI.
pub async fn trigger_screen_qa(app: &tauri::AppHandle) -> Result<(), String> {
    let (openrouter_keys, current_gen, obsidian_vault_path) = {
        let state = app.state::<Mutex<OverlayState>>();
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Ok(app_data_dir) = app.path().app_data_dir() {
            if let Ok(fresh_cfg) = config::load_config(&app_data_dir) {
                state_guard.config = Some(fresh_cfg);
            }
        }
        state_guard.qa_generation += 1;
        let gen = state_guard.qa_generation;
        let cfg = state_guard
            .config
            .clone()
            .ok_or_else(|| "Config belum dimuat di memori aplikasi".to_string())?;
        (
            cfg.openrouter_api_keys,
            gen,
            cfg.obsidian_vault_path,
        )
    };

    println!("[SCREEN QA] Memicu silent screen capture & Vision AI (generasi: {})", current_gen);

    // 1. Pindahkan tab UI ke Q&A dan tampilkan indikator proses
    let _ = app.emit("qa:recording-started", ());
    let _ = app.emit(
        "transcript:result",
        TranscriptResultPayload {
            text: "📸 Menganalisis tangkapan layar...".to_string(),
        },
    );

    // 2. Ambil screenshot layar di thread background agar tidak blocking
    let jpeg_data_url = match tauri::async_runtime::spawn_blocking(move || {
        screen::capture_screen_as_jpeg_data_url()
    })
    .await
    {
        Ok(Ok(url)) => url,
        Ok(Err(err)) => {
            eprintln!("[SCREEN QA ERROR] {}", err);
            let _ = app.emit(
                "qa:error",
                QaErrorPayload {
                    stage: "screen_capture".to_string(),
                    message: err.clone(),
                },
            );
            return Err(err);
        }
        Err(join_err) => {
            let err_msg = format!("Task capture panic/join error: {}", join_err);
            eprintln!("[SCREEN QA ERROR] {}", err_msg);
            let _ = app.emit(
                "qa:error",
                QaErrorPayload {
                    stage: "screen_capture".to_string(),
                    message: err_msg.clone(),
                },
            );
            return Err(err_msg);
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|e| format!("Gagal inisialisasi HTTP client: {}", e))?;

    let vault_context = if let Some(ref path_str) = obsidian_vault_path {
        let path = std::path::Path::new(path_str);
        match vault::scan_vault_markdown_files(path) {
            Ok(docs) => vault::extract_relevant_context(&docs, "coding problem algorithm solution", 3000),
            Err(_) => None,
        }
    } else {
        None
    };

    // 3. Eksekusi request OpenRouter Vision dengan fallback keys
    let answer_res = execute_fallback_keys(&openrouter_keys, |key| {
        let key = key.to_string();
        let client = client.clone();
        let jpeg_data_url = jpeg_data_url.clone();
        let vault_context = vault_context.clone();
        async move {
            call_openrouter_vision_ai(&client, &key, &jpeg_data_url, vault_context.as_deref()).await
        }
    })
    .await;

    // Discard jika ada request baru
    if !is_qa_generation_current(app, current_gen) {
        println!(
            "[SCREEN QA] Request generasi {} sudah stale setelah Vision AI, discard answer.",
            current_gen
        );
        return Ok(());
    }

    match answer_res {
        Ok(ans) => {
            let _ = app.emit("answer:result", AnswerResultPayload { text: ans });
            Ok(())
        }
        Err(err_msg) => {
            eprintln!("[SCREEN QA AI ERROR] {}", err_msg);
            let _ = app.emit(
                "qa:error",
                QaErrorPayload {
                    stage: "ai".to_string(),
                    message: err_msg.clone(),
                },
            );
            Err(err_msg)
        }
    }
}

/// Fungsi terpusat untuk memicu penyalinan teks seleksi secara senyap dan analisis OpenRouter Text AI (3 Model Utama).
pub async fn trigger_selected_text_qa(app: &tauri::AppHandle) -> Result<(), String> {
    let (openrouter_keys, current_gen, obsidian_vault_path) = {
        let state = app.state::<Mutex<OverlayState>>();
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Ok(app_data_dir) = app.path().app_data_dir() {
            if let Ok(fresh_cfg) = config::load_config(&app_data_dir) {
                state_guard.config = Some(fresh_cfg);
            }
        }
        state_guard.qa_generation += 1;
        let gen = state_guard.qa_generation;
        let cfg = state_guard
            .config
            .clone()
            .ok_or_else(|| "Config belum dimuat di memori aplikasi".to_string())?;
        (
            cfg.openrouter_api_keys,
            gen,
            cfg.obsidian_vault_path,
        )
    };

    println!(
        "[SELECTED TEXT QA] Memicu silent highlight text capture & Text AI (generasi: {})",
        current_gen
    );

    // 1. Pindahkan tab UI ke Q&A dan tampilkan indikator proses awal
    let _ = app.emit("qa:recording-started", ());
    let _ = app.emit(
        "transcript:result",
        TranscriptResultPayload {
            text: "📋 Membaca teks yang disorot...".to_string(),
        },
    );

    // 2. Ambil teks yang diseleksi di thread blocking agar tidak memblokir async runtime
    let captured_text = match tauri::async_runtime::spawn_blocking(move || {
        clipboard::capture_selected_text_silently()
    })
    .await
    {
        Ok(Ok(text)) => text,
        Ok(Err(err)) => {
            eprintln!("[SELECTED TEXT QA ERROR] {}", err);
            let _ = app.emit(
                "qa:error",
                QaErrorPayload {
                    stage: "clipboard".to_string(),
                    message: err.clone(),
                },
            );
            return Err(err);
        }
        Err(join_err) => {
            let err_msg = format!("Task capture panic/join error: {}", join_err);
            eprintln!("[SELECTED TEXT QA ERROR] {}", err_msg);
            let _ = app.emit(
                "qa:error",
                QaErrorPayload {
                    stage: "clipboard".to_string(),
                    message: err_msg.clone(),
                },
            );
            return Err(err_msg);
        }
    };

    // 3. Tampilkan teks pertanyaan asli yang berhasil disorot di panel Q&A
    let _ = app.emit(
        "transcript:result",
        TranscriptResultPayload {
            text: captured_text.clone(),
        },
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Gagal inisialisasi HTTP client: {}", e))?;

    let vault_context = if let Some(ref path_str) = obsidian_vault_path {
        let path = std::path::Path::new(path_str);
        match vault::scan_vault_markdown_files(path) {
            Ok(docs) => vault::extract_relevant_context(&docs, &captured_text, 3000),
            Err(_) => None,
        }
    } else {
        None
    };

    // 4. Eksekusi request ke 3 model teks utama via execute_fallback_keys & call_openrouter_ai
    let answer_res = execute_fallback_keys(&openrouter_keys, |key| {
        let key = key.to_string();
        let client = client.clone();
        let prompt_text = captured_text.clone();
        let vault_context = vault_context.clone();
        async move {
            call_openrouter_ai(&client, &key, &prompt_text, vault_context.as_deref()).await
        }
    })
    .await;

    // Discard jika ada request baru
    if !is_qa_generation_current(app, current_gen) {
        println!(
            "[SELECTED TEXT QA] Request generasi {} sudah stale setelah AI, discard answer.",
            current_gen
        );
        return Ok(());
    }

    match answer_res {
        Ok(ans) => {
            let _ = app.emit("answer:result", AnswerResultPayload { text: ans });
            Ok(())
        }
        Err(err_msg) => {
            eprintln!("[SELECTED TEXT QA AI ERROR] {}", err_msg);
            let _ = app.emit(
                "qa:error",
                QaErrorPayload {
                    stage: "ai".to_string(),
                    message: err_msg.clone(),
                },
            );
            Err(err_msg)
        }
    }
}

/// Handler terpusat saat F8 ditekan (dipakai bersama oleh shortcut fisik dan simulasi E2E).
pub fn handle_f8_pressed_event(app: &tauri::AppHandle) {
    let state = app.state::<Mutex<OverlayState>>();
    let should_emit = {
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let guard = &mut *state_guard;
        handle_f8_press(
            &mut guard.is_recording,
            &mut guard.recording_start_instant,
            &mut guard.qa_generation,
            std::time::Instant::now(),
        )
    };

    if should_emit {
        println!("[HOTKEY] F8 Pressed: recording started");
        update_tray_recording_state(app, true);
        let _ = app.emit("qa:recording-started", ());
    }
}

/// Handler terpusat saat F8 dilepas (dipakai bersama oleh shortcut fisik dan simulasi E2E).
pub fn handle_f8_released_event(app: &tauri::AppHandle) {
    let state = app.state::<Mutex<OverlayState>>();
    let below_threshold_opt = {
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let guard = &mut *state_guard;
        handle_f8_release(
            &mut guard.is_recording,
            &mut guard.recording_start_instant,
            std::time::Instant::now(),
        )
    };

    if let Some(below_threshold) = below_threshold_opt {
        println!(
            "[HOTKEY] F8 Released: recording ended (belowThreshold={})",
            below_threshold
        );
        update_tray_recording_state(app, false);
        let _ = app.emit(
            "qa:recording-ended",
            RecordingEndedPayload { below_threshold },
        );
    }
}

/// Menghasilkan pesan notifikasi peringatan jika registrasi shortcut (F8/F9) gagal.
pub fn get_hotkey_registration_error_notification(
    hotkey_name: &str,
    result: Result<(), impl std::fmt::Display>,
) -> Option<String> {
    match result {
        Ok(()) => None,
        Err(err) => Some(format!(
            "{} hotkey gagal didaftarkan ({}) - kemungkinan dipakai app lain. {}",
            hotkey_name,
            err,
            if hotkey_name == "F9" {
                "Gunakan menu tray 'Show/Hide Overlay' sebagai alternatif."
            } else {
                "Fitur Live Q&A dinonaktifkan untuk sesi ini."
            }
        )),
    }
}

/// Fungsi standalone untuk toggle visibilitas overlay (Normal <-> Concealed).
///
/// Dipakai bersama oleh shortcut F9 dan context menu Tray "Show/Hide Overlay".
pub fn toggle_overlay_visibility(app: &tauri::AppHandle) {
    let state = app.state::<Mutex<OverlayState>>();
    let (hwnd, is_concealed_before, pos_to_restore) = {
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        // Proteksi modal dialog: Jika native modal file dialog sedang aktif di layar,
        // abaikan shortcut F9 agar tidak terjadi deadlock input queue Win32.
        if let Some(dlg_hwnd_val) = state_guard.active_dialog_hwnd {
            let dlg_hwnd = dlg_hwnd_val as HWND;
            // SAFETY: IsWindow hanya memvalidasi apakah handle window masih aktif di OS.
            let is_alive = unsafe { IsWindow(dlg_hwnd) != 0 };
            if is_alive {
                println!("[HOTKEY] F9 diabaikan demi keamanan karena modal dialog sedang aktif.");
                return;
            } else {
                state_guard.active_dialog_hwnd = None;
            }
        }

        let hwnd = state_guard.hwnd;
        let is_concealed = state_guard.is_concealed;
        state_guard.is_concealed = !is_concealed;

        let mut pos = state_guard.last_normal_position;
        if is_concealed && (pos.x < 0 || pos.y < 0) {
            let (def_x, def_y) = state_guard
                .config
                .as_ref()
                .map(|c| (c.window_bounds.x, c.window_bounds.y))
                .unwrap_or((100, 100));
            pos = PhysicalPosition::new(def_x, def_y);
            state_guard.last_normal_position = pos;
        }

        (hwnd, is_concealed, pos)
    };

    let hwnd_raw = hwnd as HWND;

    if is_concealed_before {
        // Concealed -> Normal
        // SAFETY: HWND adalah handle window utama yang valid dan tersimpan di OverlayState.
        unsafe {
            SetWindowPos(
                hwnd_raw,
                0 as _,
                pos_to_restore.x,
                pos_to_restore.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
            );
        }
        // Re-apply stealth saat kembali ke Normal state
        apply_stealth(hwnd);
        println!(
            "[VISIBILITY] Normal: posisi ({}, {}) & stealth re-applied",
            pos_to_restore.x, pos_to_restore.y
        );
    } else {
        // Normal -> Concealed
        // SAFETY: HWND adalah handle window utama yang valid dan dipindahkan ke koordinat offscreen (-9999, -9999).
        unsafe {
            SetWindowPos(
                hwnd_raw,
                0 as _,
                -9999,
                -9999,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
            );
        }
        println!("[VISIBILITY] Concealed: dipindahkan ke (-9999, -9999)");
    }
}

/// Helper murni untuk mengecek apakah path event dari watcher cocok dengan target file path.
pub fn is_target_file_event(event_paths: &[PathBuf], target_path: &Path) -> bool {
    let target_canonical = target_path.canonicalize().ok();
    let target_file_name = target_path.file_name();

    event_paths.iter().any(|p| {
        if p == target_path {
            return true;
        }
        if let (Some(ref tc), Ok(pc)) = (&target_canonical, p.canonicalize()) {
            if tc == &pc {
                return true;
            }
        }
        if let (Some(ev_name), Some(tgt_name)) = (p.file_name(), target_file_name) {
            if ev_name == tgt_name {
                return true;
            }
        }
        false
    })
}

/// Menampilkan notifikasi desktop native lewat tauri-plugin-notification.
pub fn show_tray_notification(app: &tauri::AppHandle, message: &str) {
    if let Err(notify_err) = app
        .notification()
        .builder()
        .title("Screen Overlay Tool")
        .body(message)
        .show()
    {
        eprintln!(
            "[NOTIFICATION ERROR] Gagal menampilkan notifikasi: {}",
            notify_err
        );
    }
}

/// Memancarkan event `notes:error` ke frontend sekaligus menampilkan notifikasi desktop native.
pub fn trigger_notes_error(app: &tauri::AppHandle, message: &str) {
    eprintln!("[NOTES ERROR] {}", message);
    let state = app.state::<Mutex<OverlayState>>();
    {
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state_guard.last_notes_error = Some(message.to_string());
        state_guard.last_notes_content = None;
    }
    let _ = app.emit(
        "notes:error",
        NotesErrorPayload {
            message: message.to_string(),
        },
    );
    show_tray_notification(app, message);
}

/// Membaca file notes dan melakukan write-through update `last_opened_notes_path` ke `config.json` di disk.
/// Mengembalikan tuple `(content, updated_config)` jika sukses.
pub fn process_notes_file_load(
    notes_path: &Path,
    app_data_dir: &Path,
) -> Result<(String, Config), String> {
    let content = fs::read_to_string(notes_path).map_err(|err| {
        format!(
            "Gagal membaca file notes '{}': {}",
            notes_path.display(),
            err
        )
    })?;

    let path_str = notes_path.to_string_lossy().to_string();
    let updated_config =
        config::update_config_last_opened_notes_path(app_data_dir, Some(path_str))?;

    Ok((content, updated_config))
}

/// Membaca file notes, memancarkan event `notes:update`, menyimpan path ke config (write-through),
/// dan memulai pemantauan file melalui parent directory watcher non-rekursif.
/// Mendaftarkan parent directory ke file watcher jika belum pernah dipantau.
pub fn setup_or_add_watcher(app: &tauri::AppHandle, parent_dir: PathBuf) -> Result<(), String> {
    use notify::Watcher;

    let state = app.state::<Mutex<OverlayState>>();
    let mut state_guard = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if state_guard.watched_directories.contains(&parent_dir) {
        return Ok(());
    }

    if state_guard.notes_watcher.is_none() {
        let app_handle = app.clone();
        let watcher = notify::recommended_watcher(
            move |res: Result<notify::Event, notify::Error>| match res {
                Ok(event) => {
                    let state = app_handle.state::<Mutex<OverlayState>>();
                    let (opened_paths, active_path) = {
                        let guard = match state.lock() {
                            Ok(g) => g,
                            Err(p) => p.into_inner(),
                        };
                        let paths: Vec<String> = guard.opened_notes.keys().cloned().collect();
                        (paths, guard.active_notes_path.clone())
                    };

                    for path_str in opened_paths {
                        let target_path = PathBuf::from(&path_str);
                        if is_target_file_event(&event.paths, &target_path) {
                            if target_path.exists() {
                                match fs::read_to_string(&target_path) {
                                    Ok(new_content) => {
                                        println!(
                                            "[WATCHER] File notes di-update: {}",
                                            target_path.display()
                                        );
                                        {
                                            let mut guard = match state.lock() {
                                                Ok(g) => g,
                                                Err(p) => p.into_inner(),
                                            };
                                            guard
                                                .opened_notes
                                                .insert(path_str.clone(), new_content.clone());
                                            if active_path.as_deref() == Some(&path_str) {
                                                guard.last_notes_content =
                                                    Some(new_content.clone());
                                            }
                                            guard.last_notes_error = None;
                                        }
                                        let _ = app_handle.emit(
                                            "notes:update",
                                            NotesUpdatePayload {
                                                path: path_str.clone(),
                                                content: new_content,
                                            },
                                        );
                                    }
                                    Err(err) => {
                                        let msg = format!(
                                            "Gagal membaca ulang file notes '{}': {}",
                                            target_path.display(),
                                            err
                                        );
                                        trigger_notes_error(&app_handle, &msg);
                                    }
                                }
                            } else {
                                let msg = format!(
                                    "File notes '{}' telah dihapus atau dipindahkan.",
                                    target_path.display()
                                );
                                trigger_notes_error(&app_handle, &msg);
                            }
                        }
                    }
                }
                Err(err) => {
                    let msg = format!("Terjadi kesalahan pada file watcher: {}", err);
                    trigger_notes_error(&app_handle, &msg);
                }
            },
        )
        .map_err(|err| format!("Gagal menginisialisasi file watcher: {}", err))?;

        state_guard.notes_watcher = Some(watcher);
    }

    if let Some(ref mut watcher) = state_guard.notes_watcher {
        watcher
            .watch(&parent_dir, notify::RecursiveMode::NonRecursive)
            .map_err(|err| {
                format!(
                    "Gagal memantau direktori '{}': {}",
                    parent_dir.display(),
                    err
                )
            })?;
        state_guard.watched_directories.insert(parent_dir);
    }

    Ok(())
}

/// Membaca file notes, memancarkan event `notes:update`, menyimpan path ke config (write-through),
/// memperbarui in-memory OverlayState (opened_notes), dan memulai pemantauan file.
pub fn start_watching_notes_file(
    app: &tauri::AppHandle,
    notes_path: PathBuf,
) -> Result<String, String> {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(err) => {
            let msg = format!("Gagal mengakses direktori data aplikasi: {}", err);
            trigger_notes_error(app, &msg);
            return Err(msg);
        }
    };

    let content = fs::read_to_string(&notes_path).map_err(|err| {
        let msg = format!(
            "Gagal membaca file notes '{}': {}",
            notes_path.display(),
            err
        );
        trigger_notes_error(app, &msg);
        msg
    })?;

    let path_str = notes_path.to_string_lossy().to_string();

    // 1. Update in-memory state & persistensi write-through ke config.json
    {
        let state = app.state::<Mutex<OverlayState>>();
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        let mut opened_paths = state_guard
            .config
            .as_ref()
            .map(|c| c.get_effective_opened_notes_paths())
            .unwrap_or_default();

        if !opened_paths.contains(&path_str) {
            opened_paths.push(path_str.clone());
        }

        let active_path = Some(path_str.clone());
        let updated_cfg =
            config::update_config_notes_tabs(&app_data_dir, opened_paths, active_path)?;

        state_guard.config = Some(updated_cfg);
        state_guard
            .opened_notes
            .insert(path_str.clone(), content.clone());
        state_guard.active_notes_path = Some(path_str.clone());
        state_guard.last_notes_content = Some(content.clone());
        state_guard.last_notes_error = None;
    }

    // 2. Emit notes:update { path, content }
    let _ = app.emit(
        "notes:update",
        NotesUpdatePayload {
            path: path_str.clone(),
            content: content.clone(),
        },
    );

    // 3. Daftarkan parent directory ke watcher
    let parent_dir = notes_path
        .parent()
        .ok_or_else(|| "Parent directory tidak valid untuk file notes".to_string())?
        .to_path_buf();

    setup_or_add_watcher(app, parent_dir)?;

    println!(
        "[WATCHER] Memulai pemantauan file notes: {}",
        notes_path.display()
    );

    Ok(content)
}

/// Menutup tab dokumen notes tertentu dan memperbarui state serta config.json.
pub fn close_notes_file_core(
    app: &tauri::AppHandle,
    path_to_close: &str,
) -> Result<Config, String> {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(err) => {
            return Err(format!("Gagal mengakses direktori data aplikasi: {}", err));
        }
    };

    let state = app.state::<Mutex<OverlayState>>();
    let mut state_guard = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    state_guard.opened_notes.remove(path_to_close);

    let mut opened_paths = state_guard
        .config
        .as_ref()
        .map(|c| c.get_effective_opened_notes_paths())
        .unwrap_or_default();

    opened_paths.retain(|p| p != path_to_close);

    let new_active = if state_guard.active_notes_path.as_deref() == Some(path_to_close) {
        opened_paths.last().cloned()
    } else {
        state_guard.active_notes_path.clone()
    };

    state_guard.active_notes_path = new_active.clone();
    state_guard.last_notes_content = new_active
        .as_ref()
        .and_then(|p| state_guard.opened_notes.get(p).cloned());

    let updated_config = config::update_config_notes_tabs(&app_data_dir, opened_paths, new_active)?;
    state_guard.config = Some(updated_config.clone());

    Ok(updated_config)
}

/// Mengubah tab dokumen aktif dan memperbarui state serta config.json.
pub fn set_active_notes_file_core(
    app: &tauri::AppHandle,
    active_path: &str,
) -> Result<Config, String> {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(err) => {
            return Err(format!("Gagal mengakses direktori data aplikasi: {}", err));
        }
    };

    let state = app.state::<Mutex<OverlayState>>();
    let mut state_guard = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    state_guard.active_notes_path = Some(active_path.to_string());
    state_guard.last_notes_content = state_guard.opened_notes.get(active_path).cloned();

    let opened_paths = state_guard
        .config
        .as_ref()
        .map(|c| c.get_effective_opened_notes_paths())
        .unwrap_or_default();

    let updated_config = config::update_config_notes_tabs(
        &app_data_dir,
        opened_paths,
        Some(active_path.to_string()),
    )?;
    state_guard.config = Some(updated_config.clone());

    Ok(updated_config)
}

/// Menangani auto-restore seluruh tab dokumen notes saat startup aplikasi.
pub fn handle_startup_notes_restore(app: &tauri::AppHandle, config: Option<&Config>) {
    let cfg = match config {
        Some(c) => c,
        None => return,
    };

    let paths = cfg.get_effective_opened_notes_paths();
    if paths.is_empty() {
        return;
    }

    let active_path = cfg.get_effective_active_notes_path();

    for path_str in paths {
        let notes_path = PathBuf::from(&path_str);
        if notes_path.exists() {
            if let Err(err) = start_watching_notes_file(app, notes_path) {
                eprintln!("[NOTES RESTORE ERROR] {}", err);
            }
        } else {
            let msg = format!(
                "File notes sebelumnya '{}' tidak ditemukan atau telah dipindahkan.",
                path_str
            );
            trigger_notes_error(app, &msg);
        }
    }

    if let Some(active) = active_path {
        let state = app.state::<Mutex<OverlayState>>();
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state_guard.active_notes_path = Some(active.clone());
        if let Some(content) = state_guard.opened_notes.get(&active) {
            state_guard.last_notes_content = Some(content.clone());
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(debug_assertions)]
    let builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::update_font_size,
            commands::get_app_state,
            commands::get_notes_state,
            commands::pick_notes_file,
            commands::set_active_notes_file,
            commands::close_notes_file,
            commands::send_audio_blob,
            commands::ask_ai_text,
            commands::ask_ai_screen,
            commands::ask_ai_selected_text,
            commands::copy_to_clipboard,
            commands::test_trigger_hotkey
        ])
        .events(collect_events![
            ConfigErrorPayload,
            RecordingEndedPayload,
            NotesUpdatePayload,
            NotesErrorPayload,
            TranscriptResultPayload,
            AnswerResultPayload,
            QaErrorPayload
        ]);

    #[cfg(not(debug_assertions))]
    let builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::update_font_size,
            commands::get_app_state,
            commands::get_notes_state,
            commands::pick_notes_file,
            commands::set_active_notes_file,
            commands::close_notes_file,
            commands::send_audio_blob,
            commands::ask_ai_text,
            commands::ask_ai_screen,
            commands::ask_ai_selected_text,
            commands::copy_to_clipboard
        ])
        .events(collect_events![
            ConfigErrorPayload,
            RecordingEndedPayload,
            NotesUpdatePayload,
            NotesErrorPayload,
            TranscriptResultPayload,
            AnswerResultPayload,
            QaErrorPayload
        ]);

    #[cfg(debug_assertions)]
    if let Err(err) = builder.export(
        specta_typescript::Typescript::default(),
        "../src/bindings.ts",
    ) {
        eprintln!("[SPECTA] Failed to export typescript bindings: {}", err);
    }

    let f8_shortcut = Shortcut::new(None, Code::F8);
    let f9_shortcut = Shortcut::new(None, Code::F9);
    let f7_shortcut = Shortcut::new(None, Code::F7);
    let ctrl_f8_shortcut = Shortcut::new(Some(Modifiers::CONTROL), Code::F8);
    let ctrl_f7_shortcut = Shortcut::new(Some(Modifiers::CONTROL), Code::F7);
    let f6_shortcut = Shortcut::new(None, Code::F6);

    if let Err(err) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &f9_shortcut && event.state() == ShortcutState::Pressed {
                        toggle_overlay_visibility(app);
                    } else if shortcut == &f6_shortcut && event.state() == ShortcutState::Pressed {
                        let _ = app.emit("tab:toggle", ());
                    } else if (shortcut == &f7_shortcut || shortcut == &ctrl_f8_shortcut)
                        && event.state() == ShortcutState::Pressed
                    {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(err) = trigger_screen_qa(&app_handle).await {
                                eprintln!("[SCREEN QA ERROR] {}", err);
                            }
                        });
                    } else if shortcut == &ctrl_f7_shortcut
                        && event.state() == ShortcutState::Pressed
                    {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(err) = trigger_selected_text_qa(&app_handle).await {
                                eprintln!("[SELECTED TEXT QA ERROR] {}", err);
                            }
                        });
                    } else if shortcut == &f8_shortcut {
                        match event.state() {
                            ShortcutState::Pressed => {
                                handle_f8_pressed_event(app);
                            }
                            ShortcutState::Released => {
                                handle_f8_released_event(app);
                            }
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(builder.invoke_handler())
        .manage(Mutex::new(OverlayState::default()))
        .setup(move |app| {
            builder.mount_events(app);

            // 1. Load config dari app_data_dir
            let app_data_dir = match app.path().app_data_dir() {
                Ok(dir) => dir,
                Err(err) => {
                    let err_msg = format!("Gagal mengakses direktori data aplikasi: {}", err);
                    eprintln!("[CONFIG ERROR] {}", err_msg);
                    let _ = app.emit(
                        "config:error",
                        ConfigErrorPayload {
                            message: err_msg.clone(),
                        },
                    );
                    if let Err(notify_err) = app
                        .notification()
                        .builder()
                        .title("Screen Overlay Tool")
                        .body(&err_msg)
                        .show()
                    {
                        eprintln!(
                            "[NOTIFICATION ERROR] Gagal menampilkan notifikasi: {}",
                            notify_err
                        );
                    }
                    std::path::PathBuf::from(".")
                }
            };

            let loaded_config_opt = match config::load_config(&app_data_dir) {
                Ok(loaded_cfg) => {
                    println!("[CONFIG] Berhasil memuat config.json");
                    let state = app.state::<Mutex<OverlayState>>();
                    let mut state_guard = match state.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    state_guard.config = Some(loaded_cfg.clone());
                    Some(loaded_cfg)
                }
                Err(err_msg) => {
                    #[cfg(debug_assertions)]
                    if is_e2e_test_mode() {
                        println!("[E2E] Config tidak ditemukan, menggunakan mock config untuk E2E test");
                        let mock_cfg = config::Config {
                            openrouter_api_keys: vec!["mock-openrouter-key".to_string()],
                            groq_api_keys: vec!["mock-groq-key".to_string()],
                            window_bounds: config::WindowBounds::default(),
                            font_size: config::DEFAULT_FONT_SIZE,
                            opened_notes_paths: Vec::new(),
                            active_notes_path: None,
                            last_opened_notes_path: None,
                            obsidian_vault_path: None,
                            hotkeys: config::HotkeysConfig::default(),
                        };
                        let state = app.state::<Mutex<OverlayState>>();
                        let mut state_guard = match state.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        state_guard.config = Some(mock_cfg.clone());
                        Some(mock_cfg)
                    } else {
                        eprintln!("[CONFIG ERROR] {}", err_msg);
                        let _ = app.emit(
                            "config:error",
                            ConfigErrorPayload {
                                message: err_msg.clone(),
                            },
                        );
                        if let Err(notify_err) = app
                            .notification()
                            .builder()
                            .title("Screen Overlay Tool")
                            .body(&err_msg)
                            .show()
                        {
                            eprintln!(
                                "[NOTIFICATION ERROR] Gagal menampilkan notifikasi: {}",
                                notify_err
                            );
                        }
                        None
                    }
                    #[cfg(not(debug_assertions))]
                    {
                        eprintln!("[CONFIG ERROR] {}", err_msg);
                        let _ = app.emit(
                            "config:error",
                            ConfigErrorPayload {
                                message: err_msg.clone(),
                            },
                        );
                        if let Err(notify_err) = app
                            .notification()
                            .builder()
                            .title("Screen Overlay Tool")
                            .body(&err_msg)
                            .show()
                        {
                            eprintln!(
                                "[NOTIFICATION ERROR] Gagal menampilkan notifikasi: {}",
                                notify_err
                            );
                        }
                        None
                    }
                }
            };

            let window = match app.get_webview_window("main") {
                Some(w) => w,
                None => {
                    eprintln!("[ERROR] Main window tidak ditemukan saat startup.");
                    return Ok(());
                }
            };

            // 2. Validasi windowBounds terhadap monitor aktif
            let monitor_rects: Vec<config::MonitorRect> = match window.available_monitors() {
                Ok(monitors) => monitors
                    .into_iter()
                    .map(|m| {
                        let pos = m.position();
                        let size = m.size();
                        config::MonitorRect::new(pos.x, pos.y, size.width, size.height)
                    })
                    .collect(),
                Err(err) => {
                    eprintln!("[WARN] Gagal membaca monitor aktif: {}", err);
                    Vec::new()
                }
            };

            let default_bounds = config::WindowBounds::default();
            let initial_bounds = match &loaded_config_opt {
                Some(cfg) => config::resolve_initial_window_bounds(
                    &cfg.window_bounds,
                    &monitor_rects,
                    &default_bounds,
                ),
                None => default_bounds,
            };

            let _ = window.set_size(tauri::PhysicalSize::new(
                initial_bounds.width,
                initial_bounds.height,
            ));
            let _ = window.set_position(tauri::PhysicalPosition::new(
                initial_bounds.x,
                initial_bounds.y,
            ));

            let _ = window.set_always_on_top(true);
            let _ = window.show();

            let hwnd_raw = match window.hwnd() {
                Ok(h) => h.0 as isize,
                Err(err) => {
                    eprintln!("[ERROR] Gagal mendapatkan HWND window: {}", err);
                    0
                }
            };
            println!("[STEALTH] HWND didapatkan: {:?}", hwnd_raw);

            if hwnd_raw != 0 {
                // 1. Terapkan stealth ke main HWND dan seluruh child HWND (WebView2)
                apply_stealth(hwnd_raw);
            }

            // Simpan posisi awal & HWND ke OverlayState
            {
                let state = app.state::<Mutex<OverlayState>>();
                let mut state_guard = match state.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                state_guard.last_normal_position =
                    PhysicalPosition::new(initial_bounds.x, initial_bounds.y);
                state_guard.hwnd = hwnd_raw;
            }

            // 3. Setup Debounced Window Bounds Persistence Worker
            let (tx_bounds, rx_bounds) = std::sync::mpsc::channel::<config::WindowBounds>();
            let app_data_dir_for_debounce = app_data_dir.clone();
            let app_handle_for_debounce = app.handle().clone();

            std::thread::spawn(move || {
                while let Ok(mut latest_bounds) = rx_bounds.recv() {
                    let debounce_duration = std::time::Duration::from_millis(500);
                    let start = std::time::Instant::now();
                    loop {
                        let elapsed = start.elapsed();
                        if elapsed >= debounce_duration {
                            break;
                        }
                        let remaining = debounce_duration - elapsed;
                        match rx_bounds.recv_timeout(remaining) {
                            Ok(new_bounds) => {
                                latest_bounds = new_bounds;
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    }

                    // Write-through: Simpan ke disk dulu
                    if let Err(err) = config::update_config_window_bounds(
                        &app_data_dir_for_debounce,
                        latest_bounds.clone(),
                    ) {
                        eprintln!(
                            "[CONFIG ERROR] Gagal mendebounce simpan window bounds: {}",
                            err
                        );
                    } else {
                        // Jika penulisan disk sukses, perbarui in-memory state
                        let state = app_handle_for_debounce.state::<Mutex<OverlayState>>();
                        let mut state_guard = match state.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        if let Some(ref mut cfg) = state_guard.config {
                            cfg.window_bounds = latest_bounds;
                        }
                    }
                }
            });

            // 4. Webview & Window event listener
            let hwnd_for_webview = hwnd_raw;
            window.on_webview_event(move |_event| {
                if hwnd_for_webview != 0 {
                    apply_stealth(hwnd_for_webview);
                    println!("[STEALTH] Re-applied pada WebviewEvent");
                }
            });

            let hwnd_for_window = hwnd_raw;
            let tx_for_window = tx_bounds.clone();
            let app_handle_for_window = app.handle().clone();
            let app_data_dir_for_window = app_data_dir.clone();
            let window_for_event = window.clone();

            window.on_window_event(move |event| {
                match event {
                    tauri::WindowEvent::Focused(true) => {
                        if hwnd_for_window != 0 {
                            apply_stealth(hwnd_for_window);
                            println!("[STEALTH] Re-applied pada WindowEvent::Focused");
                        }
                    }
                    tauri::WindowEvent::Moved(pos) => {
                        if pos.x >= 0 && pos.y >= 0 {
                            let state = app_handle_for_window.state::<Mutex<OverlayState>>();
                            let mut state_guard = match state.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            if !state_guard.is_concealed {
                                state_guard.last_normal_position = *pos;
                                if let Ok(size) = window_for_event.outer_size() {
                                    let _ = tx_for_window.send(config::WindowBounds {
                                        x: pos.x,
                                        y: pos.y,
                                        width: size.width,
                                        height: size.height,
                                    });
                                }
                            }
                        }
                    }
                    tauri::WindowEvent::Resized(size) => {
                        if hwnd_for_window != 0 {
                            apply_stealth(hwnd_for_window);
                        }

                        let state = app_handle_for_window.state::<Mutex<OverlayState>>();
                        let (is_concealed, pos) = {
                            let state_guard = match state.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            (state_guard.is_concealed, state_guard.last_normal_position)
                        };

                        if !is_concealed && pos.x >= 0 && pos.y >= 0 {
                            let _ = tx_for_window.send(config::WindowBounds {
                                x: pos.x,
                                y: pos.y,
                                width: size.width,
                                height: size.height,
                            });
                        }
                    }
                    tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed => {
                        // Flush-on-exit: Pastikan posisi window terakhir ter-commit ke disk jika app ditutup dalam rentang debounce < 500ms
                        let state = app_handle_for_window.state::<Mutex<OverlayState>>();
                        let (is_concealed, last_pos) = {
                            let state_guard = match state.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            (state_guard.is_concealed, state_guard.last_normal_position)
                        };

                        if !is_concealed {
                            if let Ok(size) = window_for_event.outer_size() {
                                let bounds = config::WindowBounds {
                                    x: last_pos.x,
                                    y: last_pos.y,
                                    width: size.width,
                                    height: size.height,
                                };
                                if let Err(err) = config::update_config_window_bounds(
                                    &app_data_dir_for_window,
                                    bounds,
                                ) {
                                    eprintln!(
                                        "[CONFIG ERROR] Gagal flush window bounds saat close: {}",
                                        err
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            });

            // 5. Setup Tray Icon & Context Menu (TDD §9)
            let toggle_menu_item =
                MenuItemBuilder::with_id("toggle", "Show/Hide Overlay").build(app)?;
            let quit_menu_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .items(&[&toggle_menu_item, &quit_menu_item])
                .build()?;

            let tray_builder = TrayIconBuilder::with_id(MAIN_TRAY_ID)
                .tooltip(DEFAULT_TRAY_TOOLTIP)
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "toggle" => {
                        toggle_overlay_visibility(app);
                    }
                    "quit" => {
                        // ATURAN WAJIB (Scope #3): Melalui jalur close window biasa
                        // agar WindowEvent::CloseRequested/Destroyed ter-trigger dan
                        // flush pending windowBounds write tetap jalan.
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.close();
                        }
                    }
                    _ => {}
                });

            if let Some(icon) = app.default_window_icon() {
                let _ = tray_builder.icon(icon.clone()).build(app);
            } else {
                let _ = tray_builder.build(app);
            }

            // 6. Registrasi Shortcut F8 & F9 (dengan E2E_TEST_MODE check & graceful degradation)
            if is_e2e_test_mode() {
                println!("[E2E] E2E_TEST_MODE aktif: Registrasi global hotkey fisik (F8 & F9) ke OS dilewati.");
                let state = app.state::<Mutex<OverlayState>>();
                let mut state_guard = match state.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                state_guard.qa_available = true;
            } else {
                // Registrasi F9 (Toggle visibility)
                let f9_reg_res = app.global_shortcut().register(f9_shortcut);
                if let Some(error_msg) =
                    get_hotkey_registration_error_notification("F9", f9_reg_res)
                {
                    eprintln!("[ERROR] {}", error_msg);
                    if let Err(notify_err) = app
                        .notification()
                        .builder()
                        .title("Screen Overlay Tool")
                        .body(&error_msg)
                        .show()
                    {
                        eprintln!(
                            "[NOTIFICATION ERROR] Gagal menampilkan notifikasi: {}",
                            notify_err
                        );
                    }
                } else {
                    println!("[HOTKEY] F9 shortcut terdaftar - siap digunakan");
                }

                // Registrasi F8 (Live Q&A push-to-talk)
                let f8_reg_res = app.global_shortcut().register(f8_shortcut);
                let state = app.state::<Mutex<OverlayState>>();
                let mut state_guard = match state.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };

                if let Some(error_msg) =
                    get_hotkey_registration_error_notification("F8", f8_reg_res)
                {
                    eprintln!("[ERROR] {}", error_msg);
                    state_guard.qa_available = false;
                    if let Err(notify_err) = app
                        .notification()
                        .builder()
                        .title("Screen Overlay Tool")
                        .body(&error_msg)
                        .show()
                    {
                        eprintln!(
                            "[NOTIFICATION ERROR] Gagal menampilkan notifikasi: {}",
                            notify_err
                        );
                    }
                } else {
                    state_guard.qa_available = true;
                    println!("[HOTKEY] F8 shortcut terdaftar - siap digunakan");
                }

                // Registrasi F7 & Ctrl+F8 (Silent Screen Capture to Vision AI)
                let _ = app.global_shortcut().register(f7_shortcut);
                let _ = app.global_shortcut().register(ctrl_f8_shortcut);
                println!("[HOTKEY] F7 & Ctrl+F8 (Silent Screen QA) shortcuts terdaftar - siap digunakan");

                // Registrasi Ctrl+F7 (Silent Selected Text QA ke 3 Model Teks Utama)
                let _ = app.global_shortcut().register(ctrl_f7_shortcut);
                println!("[HOTKEY] Ctrl+F7 (Silent Selected Text QA) shortcut terdaftar - siap digunakan");

                // Registrasi F6 (Toggle Tab Notes <-> Live Q&A)
                let _ = app.global_shortcut().register(f6_shortcut);
                println!("[HOTKEY] F6 (Toggle Tab) shortcut terdaftar - siap digunakan");
            }

            // 7. Auto-restore notes file jika tersimpan di config
            handle_startup_notes_restore(app.handle(), loaded_config_opt.as_ref());

            Ok(())
        })
        .run(tauri::generate_context!())
    {
        eprintln!("Error while running tauri application: {}", err);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_state_toggle_concealed_cycle() {
        let mut state = OverlayState::default();
        // Awal mula: Normal state (is_concealed == false)
        assert!(!state.is_concealed);

        // Panggilan ke-1: berpindah ke Concealed (is_concealed == true)
        let is_concealed_1 = state.toggle_concealed();
        assert!(is_concealed_1);
        assert!(state.is_concealed);

        // Panggilan ke-2: kembali ke Normal (is_concealed == false)
        let is_concealed_2 = state.toggle_concealed();
        assert!(!is_concealed_2);
        assert!(!state.is_concealed);

        // Panggilan ke-3: berpindah ke Concealed lagi (is_concealed == true)
        let is_concealed_3 = state.toggle_concealed();
        assert!(is_concealed_3);
        assert!(state.is_concealed);
    }

    #[test]
    fn test_toggle_visibility_position_protection_fallback() {
        let mut state = OverlayState::default();
        state.last_normal_position = PhysicalPosition::new(-9999, -9999);
        state.is_concealed = true;

        let mut pos = state.last_normal_position;
        if pos.x <= -5000 || pos.y <= -5000 {
            pos = PhysicalPosition::new(100, 100);
            state.last_normal_position = pos;
        }

        assert_eq!(state.last_normal_position.x, 100);
        assert_eq!(state.last_normal_position.y, 100);
    }

    #[test]
    fn test_get_hotkey_registration_error_notification_ok_returns_none() {
        let res_f9: Result<(), String> = Ok(());
        assert_eq!(
            get_hotkey_registration_error_notification("F9", res_f9),
            None
        );

        let res_f8: Result<(), String> = Ok(());
        assert_eq!(
            get_hotkey_registration_error_notification("F8", res_f8),
            None
        );
    }

    #[test]
    fn test_get_hotkey_registration_error_notification_f9_err() {
        let res: Result<(), &str> = Err("hotkey conflict F9");
        let notif = get_hotkey_registration_error_notification("F9", res);
        assert!(notif.is_some());
        let msg = notif.unwrap();
        assert!(msg.contains("F9 hotkey gagal didaftarkan"));
        assert!(msg.contains("Show/Hide Overlay"));
        assert!(msg.contains("hotkey conflict F9"));
    }

    #[test]
    fn test_get_hotkey_registration_error_notification_f8_err() {
        let res: Result<(), &str> = Err("hotkey conflict F8");
        let notif = get_hotkey_registration_error_notification("F8", res);
        assert!(notif.is_some());
        let msg = notif.unwrap();
        assert!(msg.contains("F8 hotkey gagal didaftarkan"));
        assert!(msg.contains("Live Q&A dinonaktifkan"));
        assert!(msg.contains("hotkey conflict F8"));
    }

    #[test]
    fn test_overlay_state_default_qa_available_is_true() {
        let state = OverlayState::default();
        assert!(state.qa_available);
    }

    #[test]
    fn test_overlay_state_qa_available_reflects_f8_registration_failure() {
        let mut state = OverlayState::default();
        assert!(state.qa_available);

        // Simulasi path registrasi F8 gagal di setup()
        let f8_reg_res: Result<(), &str> = Err("hotkey F8 occupied");
        if get_hotkey_registration_error_notification("F8", f8_reg_res).is_some() {
            state.qa_available = false;
        }

        assert!(!state.qa_available);
    }

    #[test]
    fn test_parse_e2e_test_mode_from_str_matrix() {
        // Harus bernilai true hanya jika string "true" (case-insensitive)
        assert!(parse_e2e_test_mode_from_str(Some("true")));
        assert!(parse_e2e_test_mode_from_str(Some("TRUE")));
        assert!(parse_e2e_test_mode_from_str(Some("True")));
        assert!(parse_e2e_test_mode_from_str(Some("  true  ")));

        // Nilai lain harus dievaluasi sebagai false
        assert!(!parse_e2e_test_mode_from_str(Some("false")));
        assert!(!parse_e2e_test_mode_from_str(Some("1")));
        assert!(!parse_e2e_test_mode_from_str(Some("0")));
        assert!(!parse_e2e_test_mode_from_str(Some("e2e")));
        assert!(!parse_e2e_test_mode_from_str(Some("")));
        assert!(!parse_e2e_test_mode_from_str(None));
    }

    #[test]
    fn test_is_hold_duration_below_threshold() {
        assert!(is_hold_duration_below_threshold(
            std::time::Duration::from_millis(0)
        ));
        assert!(is_hold_duration_below_threshold(
            std::time::Duration::from_millis(200)
        ));
        assert!(is_hold_duration_below_threshold(
            std::time::Duration::from_millis(399)
        ));

        // 400ms ke atas dianggap hold valid (below_threshold = false)
        assert!(!is_hold_duration_below_threshold(
            std::time::Duration::from_millis(400)
        ));
        assert!(!is_hold_duration_below_threshold(
            std::time::Duration::from_millis(500)
        ));
        assert!(!is_hold_duration_below_threshold(
            std::time::Duration::from_millis(1500)
        ));
    }

    #[test]
    fn test_handle_f8_press_first_time_returns_true_and_sets_recording() {
        let mut is_recording = false;
        let mut start_instant = None;
        let mut qa_generation = 0;
        let now = std::time::Instant::now();

        let should_emit = handle_f8_press(
            &mut is_recording,
            &mut start_instant,
            &mut qa_generation,
            now,
        );
        assert!(should_emit);
        assert!(is_recording);
        assert_eq!(start_instant, Some(now));
        assert_eq!(qa_generation, 1);
    }

    #[test]
    fn test_handle_f8_press_repeated_returns_false_and_preserves_timestamp() {
        let mut is_recording = true;
        let t0 = std::time::Instant::now();
        let mut start_instant = Some(t0);
        let mut qa_generation = 1;
        let t1 = t0 + std::time::Duration::from_millis(100);

        // Key-repeat dari OS saat tombol masih ditahan
        let should_emit = handle_f8_press(
            &mut is_recording,
            &mut start_instant,
            &mut qa_generation,
            t1,
        );
        assert!(!should_emit);
        assert!(is_recording);
        assert_eq!(start_instant, Some(t0)); // Timestamp awal tidak tertimpa
        assert_eq!(qa_generation, 1); // Generation tidak berubah saat repeated
    }

    #[test]
    fn test_handle_f8_release_below_threshold_returns_some_true() {
        let mut is_recording = true;
        let t0 = std::time::Instant::now();
        let mut start_instant = Some(t0);
        let t1 = t0 + std::time::Duration::from_millis(200); // 200ms < 400ms

        let res = handle_f8_release(&mut is_recording, &mut start_instant, t1);
        assert_eq!(res, Some(true)); // belowThreshold = true
        assert!(!is_recording);
        assert_eq!(start_instant, None);
    }

    #[test]
    fn test_handle_f8_release_above_threshold_returns_some_false() {
        let mut is_recording = true;
        let t0 = std::time::Instant::now();
        let mut start_instant = Some(t0);
        let t1 = t0 + std::time::Duration::from_millis(600); // 600ms >= 400ms

        let res = handle_f8_release(&mut is_recording, &mut start_instant, t1);
        assert_eq!(res, Some(false)); // belowThreshold = false
        assert!(!is_recording);
        assert_eq!(start_instant, None);
    }

    #[test]
    fn test_handle_f8_release_when_not_recording_returns_none_and_no_panic() {
        let mut is_recording = false;
        let mut start_instant = None;
        let now = std::time::Instant::now();

        // Released terpanggil padahal tidak sedang recording
        let res = handle_f8_release(&mut is_recording, &mut start_instant, now);
        assert_eq!(res, None);
        assert!(!is_recording);
        assert_eq!(start_instant, None);
    }

    #[test]
    fn test_is_target_file_event_matching_and_filtering() {
        let temp_dir = std::env::temp_dir().join("poc_overlay_test_filter");
        let _ = fs::create_dir_all(&temp_dir);
        let target_file = temp_dir.join("notes.md");
        let other_file = temp_dir.join("other.txt");
        let _ = fs::write(&target_file, "content");
        let _ = fs::write(&other_file, "other content");

        // 1. Path persis sama
        assert!(is_target_file_event(&[target_file.clone()], &target_file));

        // 2. Event berisi multiple paths termasuk target_file
        assert!(is_target_file_event(
            &[other_file.clone(), target_file.clone()],
            &target_file
        ));

        // 3. Event hanya berisi other_file -> harus false
        assert!(!is_target_file_event(&[other_file.clone()], &target_file));

        // 4. Event paths kosong -> harus false
        assert!(!is_target_file_event(&[], &target_file));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_startup_restore_when_path_is_none_no_action() {
        let config = Config {
            last_opened_notes_path: None,
            ..Default::default()
        };
        // Menguji logic ekstraksi path: None menghasilkan no-op
        let path_opt = config.last_opened_notes_path.as_deref();
        assert_eq!(path_opt, None);
    }

    #[test]
    fn test_startup_restore_when_path_valid_reads_content() {
        let temp_dir = std::env::temp_dir().join("poc_overlay_test_restore_valid");
        let _ = fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("valid_notes.md");
        let expected_content = "# Hello Notes\nIni konten awal.";
        fs::write(&test_file, expected_content).unwrap();

        assert!(test_file.exists());
        let read_content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(read_content, expected_content);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_startup_restore_when_path_missing_triggers_error() {
        let missing_path = PathBuf::from("C:\\non_existent_folder_xyz_123\\missing_notes.md");
        assert!(!missing_path.exists());

        let read_res = fs::read_to_string(&missing_path);
        assert!(read_res.is_err());
    }

    #[test]
    fn test_notes_watcher_detects_real_file_modification() {
        use notify::Watcher;

        let temp_dir = std::env::temp_dir().join("poc_overlay_test_real_watcher");
        let _ = fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("test_real_notes.md");
        fs::write(&test_file, "Initial content").unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let target_file = test_file.clone();

        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    if is_target_file_event(&event.paths, &target_file) {
                        let _ = tx.send(true);
                    }
                }
            })
            .unwrap();

        watcher
            .watch(&temp_dir, notify::RecursiveMode::NonRecursive)
            .unwrap();

        // Modifikasi file secara nyata di disk
        std::thread::sleep(std::time::Duration::from_millis(100));
        fs::write(&test_file, "Updated content from test").unwrap();

        // Tunggu event diterima oleh watcher channel (timeout 3 detik)
        let received = rx.recv_timeout(std::time::Duration::from_secs(3));
        assert!(received.is_ok());
        assert_eq!(received.unwrap(), true);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_start_watching_notes_file_integrated_flow() {
        let temp_dir = std::env::temp_dir().join("poc_overlay_test_integrated_notes");
        let _ = fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("my_notes.md");
        let content = "# Markdown Title\nIntegrated test content.";
        fs::write(&test_file, content).unwrap();

        let app_data_dir = temp_dir.join("app_data");

        // Jalankan logic terintegrasi: baca file + write-through ke config.json
        let (read_content, updated_config) =
            process_notes_file_load(&test_file, &app_data_dir).unwrap();

        // 1. Verifikasi content yang dibaca untuk emit notes:update persis
        assert_eq!(read_content, content);

        // 2. Verifikasi config in-memory ter-update dengan path file
        assert_eq!(
            updated_config.last_opened_notes_path,
            Some(test_file.to_string_lossy().to_string())
        );

        // 3. Verifikasi persistensi nyata di file config.json di disk
        let config_path = config::get_config_path(&app_data_dir);
        let raw_json = fs::read_to_string(&config_path).unwrap();
        let from_disk: Config = serde_json::from_str(&raw_json).unwrap();
        assert_eq!(
            from_disk.last_opened_notes_path,
            Some(test_file.to_string_lossy().to_string())
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_notes_state_when_empty_returns_both_none() {
        let state = OverlayState::default();
        assert_eq!(state.last_notes_content, None);
        assert_eq!(state.last_notes_error, None);
    }

    #[test]
    fn test_get_notes_state_reflects_content_and_error_states() {
        let mut state = OverlayState::default();

        // 1. Setelah sukses restore/load
        state.last_notes_content = Some("# Restored content".to_string());
        state.last_notes_error = None;
        assert_eq!(
            state.last_notes_content,
            Some("# Restored content".to_string())
        );
        assert_eq!(state.last_notes_error, None);

        // 2. Setelah error restore/runtime
        state.last_notes_error = Some("File not found".to_string());
        state.last_notes_content = None;
        assert_eq!(state.last_notes_content, None);
        assert_eq!(state.last_notes_error, Some("File not found".to_string()));
    }

    #[test]
    fn test_tray_recording_tooltip_constants() {
        assert_eq!(MAIN_TRAY_ID, "main-tray");
        assert_eq!(DEFAULT_TRAY_TOOLTIP, "Screen Overlay Tool");
        assert_eq!(RECORDING_TRAY_TOOLTIP, "Screen Overlay Tool — Recording...");
    }

    #[test]
    fn test_classify_http_error_matrix() {
        use reqwest::StatusCode;

        // Non-transient errors (Stop / fail-fast)
        assert_eq!(
            classify_http_error(Some(StatusCode::BAD_REQUEST), "bad request"),
            RetryAction::StopNonTransient("HTTP 400 Bad Request: bad request".to_string())
        );
        assert_eq!(
            classify_http_error(Some(StatusCode::NOT_FOUND), "not found"),
            RetryAction::StopNonTransient("HTTP 404 Not Found: not found".to_string())
        );
        assert_eq!(
            classify_http_error(Some(StatusCode::UNPROCESSABLE_ENTITY), "unprocessable"),
            RetryAction::StopNonTransient(
                "HTTP 422 Unprocessable Entity: unprocessable".to_string()
            )
        );

        // Transient errors (Retry next key)
        assert_eq!(
            classify_http_error(Some(StatusCode::UNAUTHORIZED), "invalid key"),
            RetryAction::RetryNextKey("HTTP 401 Unauthorized: invalid key".to_string())
        );
        assert_eq!(
            classify_http_error(Some(StatusCode::FORBIDDEN), "forbidden"),
            RetryAction::RetryNextKey("HTTP 403 Forbidden: forbidden".to_string())
        );
        assert_eq!(
            classify_http_error(Some(StatusCode::TOO_MANY_REQUESTS), "rate limited"),
            RetryAction::RetryNextKey("HTTP 429 Too Many Requests: rate limited".to_string())
        );
        assert_eq!(
            classify_http_error(Some(StatusCode::INTERNAL_SERVER_ERROR), "server error"),
            RetryAction::RetryNextKey("HTTP 500 Internal Server Error: server error".to_string())
        );
        assert_eq!(
            classify_http_error(None, "connection timeout"),
            RetryAction::RetryNextKey("connection timeout".to_string())
        );
    }

    #[test]
    fn test_execute_fallback_keys_first_success() {
        tauri::async_runtime::block_on(async {
            let keys = vec!["key1".to_string(), "key2".to_string()];
            let mut call_count = 0;

            let result = execute_fallback_keys(&keys, |_key| {
                call_count += 1;
                async move {
                    Ok::<String, (Option<reqwest::StatusCode>, String)>(
                        "success_response".to_string(),
                    )
                }
            })
            .await;

            assert_eq!(result, Ok("success_response".to_string()));
            assert_eq!(call_count, 1);
        });
    }

    #[test]
    fn test_execute_fallback_keys_transient_retries_and_succeeds_on_second_key() {
        tauri::async_runtime::block_on(async {
            let keys = vec!["key_429".to_string(), "key_valid".to_string()];
            let mut tried_keys = Vec::new();

            let result = execute_fallback_keys(&keys, |key| {
                tried_keys.push(key.to_string());
                let k = key.to_string();
                async move {
                    if k == "key_429" {
                        Err((
                            Some(reqwest::StatusCode::TOO_MANY_REQUESTS),
                            "rate limited".to_string(),
                        ))
                    } else {
                        Ok("second_key_success".to_string())
                    }
                }
            })
            .await;

            assert_eq!(result, Ok("second_key_success".to_string()));
            assert_eq!(tried_keys, vec!["key_429", "key_valid"]);
        });
    }

    #[test]
    fn test_execute_fallback_keys_non_transient_stops_immediately() {
        tauri::async_runtime::block_on(async {
            let keys = vec!["key_400".to_string(), "key_never_called".to_string()];
            let mut tried_keys = Vec::new();

            let result = execute_fallback_keys(&keys, |key| {
                tried_keys.push(key.to_string());
                let k = key.to_string();
                async move {
                    if k == "key_400" {
                        Err((
                            Some(reqwest::StatusCode::BAD_REQUEST),
                            "bad prompt".to_string(),
                        ))
                    } else {
                        Ok("unexpected".to_string())
                    }
                }
            })
            .await;

            assert!(result.is_err());
            assert!(result.unwrap_err().contains("400 Bad Request"));
            assert_eq!(tried_keys, vec!["key_400"]);
        });
    }

    #[test]
    fn test_execute_fallback_keys_all_transient_fails_with_last_error() {
        tauri::async_runtime::block_on(async {
            let keys = vec!["key_429".to_string(), "key_401".to_string()];

            let result: Result<String, String> = execute_fallback_keys(&keys, |key| {
                let k = key.to_string();
                async move {
                    if k == "key_429" {
                        Err((
                            Some(reqwest::StatusCode::TOO_MANY_REQUESTS),
                            "rate limited".to_string(),
                        ))
                    } else {
                        Err((
                            Some(reqwest::StatusCode::UNAUTHORIZED),
                            "invalid token".to_string(),
                        ))
                    }
                }
            })
            .await;

            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(err.contains("401 Unauthorized"));
        });
    }

    #[test]
    fn test_endpoint_getters_default_and_override() {
        // 1. Saat E2E_TEST_MODE tidak aktif -> default production URL
        std::env::remove_var("E2E_TEST_MODE");
        std::env::set_var("GROQ_API_URL", "http://127.0.0.1:9999/groq");
        std::env::set_var("OPENROUTER_API_URL", "http://127.0.0.1:9999/openrouter");

        assert_eq!(
            get_groq_endpoint(),
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
        assert_eq!(
            get_openrouter_endpoint(),
            "https://openrouter.ai/api/v1/chat/completions"
        );

        // 2. Saat E2E_TEST_MODE=true -> return overridden URL
        std::env::set_var("E2E_TEST_MODE", "true");
        assert_eq!(get_groq_endpoint(), "http://127.0.0.1:9999/groq");
        assert_eq!(
            get_openrouter_endpoint(),
            "http://127.0.0.1:9999/openrouter"
        );

        // Cleanup env vars
        std::env::remove_var("E2E_TEST_MODE");
        std::env::remove_var("GROQ_API_URL");
        std::env::remove_var("OPENROUTER_API_URL");
    }
}


