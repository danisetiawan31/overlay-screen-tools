use crate::config;
use crate::OverlayState;
#[cfg(debug_assertions)]
use crate::{
    handle_f8_pressed_event, handle_f8_released_event, is_e2e_test_mode, toggle_overlay_visibility,
};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, SetWindowDisplayAffinity,
};

const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub font_size: u32,
    pub qa_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NoteDocument {
    pub path: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PickNotesFileResponse {
    pub path: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotesState {
    pub documents: Vec<NoteDocument>,
    pub active_path: Option<String>,
    pub content: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SendAudioBlobArgs {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AskAiTextArgs {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CopyToClipboardArgs {
    pub text: String,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TestTriggerHotkeyArgs {
    pub hotkey: String,
    pub state: String,
}

#[tauri::command]
#[specta::specta]
pub fn get_notes_state(state: tauri::State<Mutex<OverlayState>>) -> Result<NotesState, String> {
    let state_guard = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let ordered_paths = state_guard
        .config
        .as_ref()
        .map(|c| c.get_effective_opened_notes_paths())
        .unwrap_or_default();

    let mut documents = Vec::new();
    for p in &ordered_paths {
        if let Some(cnt) = state_guard.opened_notes.get(p) {
            let path_buf = std::path::PathBuf::from(p);
            let title = path_buf
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.clone());
            documents.push(NoteDocument {
                path: p.clone(),
                title,
                content: cnt.clone(),
            });
        }
    }

    for (p, cnt) in &state_guard.opened_notes {
        if !ordered_paths.contains(p) {
            let path_buf = std::path::PathBuf::from(p);
            let title = path_buf
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.clone());
            documents.push(NoteDocument {
                path: p.clone(),
                title,
                content: cnt.clone(),
            });
        }
    }

    Ok(NotesState {
        documents,
        active_path: state_guard.active_notes_path.clone(),
        content: state_guard.last_notes_content.clone(),
        error: state_guard.last_notes_error.clone(),
    })
}

pub struct EnumDialogContext {
    pub target_pid: u32,
    pub main_hwnd_val: isize,
    pub found_dialog_hwnd_val: isize,
}

/// Helper murni untuk mengevaluasi apakah HWND kandidat adalah dialog modal milik proses yang sama dan bukan main HWND.
pub fn is_matching_dialog_window(
    candidate_hwnd_val: isize,
    candidate_pid: u32,
    main_hwnd_val: isize,
    target_pid: u32,
) -> bool {
    if candidate_hwnd_val == main_hwnd_val || candidate_hwnd_val == 0 {
        return false;
    }
    candidate_pid == target_pid
}

/// Callback prosedur enumerasi window Win32 untuk menemukan HWND modal dialog milik proses saat ini.
///
/// # Safety
///
/// Fungsi ini aman dipanggil oleh `EnumWindows` selama parameter `lparam` menunjuk ke alokasi
/// `EnumDialogContext` yang valid dan masih hidup sepanjang siklus enumerasi.
pub unsafe extern "system" fn enum_dialog_window_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let ctx = &mut *(lparam as *mut EnumDialogContext);

    let mut process_id: u32 = 0;
    // SAFETY: process_id adalah variabel stack lokal yang valid dan pointernya diberikan ke GetWindowThreadProcessId.
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut process_id);
    }

    if is_matching_dialog_window(hwnd as isize, process_id, ctx.main_hwnd_val, ctx.target_pid) {
        ctx.found_dialog_hwnd_val = hwnd as isize;
        return 0; // Berhenti enumerasi karena HWND dialog sudah ditemukan
    }

    1 // Lanjut mencari window lainnya
}

/// Men-spawn thread background pengawas untuk memproteksi native file dialog dari screen capture (TDD §3).
pub fn spawn_dialog_watcher_thread(app: tauri::AppHandle, main_hwnd_val: isize) {
    // SAFETY: GetCurrentProcessId tidak memerlukan parameter atau alokasi memori dan selalu aman dipanggil.
    let current_pid = unsafe { GetCurrentProcessId() };

    std::thread::spawn(move || {
        let start_time = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(5);
        let poll_interval = std::time::Duration::from_millis(30);

        while start_time.elapsed() < timeout {
            let mut context = EnumDialogContext {
                target_pid: current_pid,
                main_hwnd_val,
                found_dialog_hwnd_val: 0,
            };

            // SAFETY: context adalah stack frame lokal thread ini dan lparam menunjuk ke alamat context selama EnumWindows berjalan.
            unsafe {
                EnumWindows(
                    Some(enum_dialog_window_proc),
                    &mut context as *mut EnumDialogContext as LPARAM,
                );
            }

            if context.found_dialog_hwnd_val != 0 {
                let dialog_hwnd = context.found_dialog_hwnd_val as HWND;
                // SAFETY: dialog_hwnd adalah HWND valid milik proses ini yang baru ditemukan lewat EnumWindows.
                unsafe {
                    let res = SetWindowDisplayAffinity(dialog_hwnd, WDA_EXCLUDEFROMCAPTURE);
                    println!(
                        "[STEALTH DIALOG] Applied WDA_EXCLUDEFROMCAPTURE to dialog HWND ({:?}): result={}",
                        dialog_hwnd, res
                    );
                }

                // Catat HWND dialog aktif di OverlayState agar bisa dikontrol (misal diabaikan saat toggle visibility F9)
                let state = app.state::<Mutex<crate::OverlayState>>();
                let mut state_guard = match state.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                state_guard.active_dialog_hwnd = Some(context.found_dialog_hwnd_val);
                break; // Keluar begitu HWND dialog berhasil diproteksi
            }

            std::thread::sleep(poll_interval);
        }
    });
}

#[tauri::command]
#[specta::specta]
pub fn pick_notes_file(app: tauri::AppHandle) -> Result<Option<PickNotesFileResponse>, String> {
    use tauri_plugin_dialog::DialogExt;

    let main_hwnd = {
        let state = app.state::<Mutex<crate::OverlayState>>();
        let state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state_guard.hwnd
    };

    if main_hwnd != 0 {
        spawn_dialog_watcher_thread(app.clone(), main_hwnd);
    }

    // Nonaktifkan always_on_top sementara agar dialog file picker OS muncul di depan secara natural
    let main_win = app.get_webview_window("main");
    if let Some(ref win) = main_win {
        let _ = win.set_always_on_top(false);
    }

    let file_path = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md"])
        .blocking_pick_file();

    // Kembalikan always_on_top setelah dialog selesai
    if let Some(ref win) = main_win {
        let _ = win.set_always_on_top(true);
    }

    // Reset active_dialog_hwnd setelah dialog selesai (baik user memilih file atau cancel / ditutup via F9)
    {
        let state = app.state::<Mutex<crate::OverlayState>>();
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state_guard.active_dialog_hwnd = None;
    }

    match file_path {
        Some(fp) => {
            let path_buf = fp.into_path().map_err(|err| err.to_string())?;
            let path_str = path_buf.to_string_lossy().to_string();
            let title = path_buf
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path_str.clone());
            let content = crate::start_watching_notes_file(&app, path_buf)?;
            Ok(Some(PickNotesFileResponse {
                path: path_str,
                title,
                content,
            }))
        }
        None => Ok(None),
    }
}

#[tauri::command]
#[specta::specta]
pub fn set_active_notes_file(app: tauri::AppHandle, path: String) -> Result<(), String> {
    crate::set_active_notes_file_core(&app, &path)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn close_notes_file(app: tauri::AppHandle, path: String) -> Result<(), String> {
    crate::close_notes_file_core(&app, &path)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn update_font_size(
    size: u32,
    app: tauri::AppHandle,
    state: tauri::State<Mutex<OverlayState>>,
) -> Result<(), String> {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(err) => {
            return Err(format!("Gagal mengakses direktori data aplikasi: {}", err));
        }
    };

    let updated_config = config::update_config_font_size(&app_data_dir, size)?;

    let mut state_guard = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    state_guard.config = Some(updated_config);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn send_audio_blob(args: SendAudioBlobArgs, app: tauri::AppHandle) -> Result<(), String> {
    if args.bytes.is_empty() {
        return Err("Audio blob kosong".to_string());
    }

    let (groq_keys, openrouter_keys, current_gen, obsidian_vault_path) = {
        let state = app.state::<Mutex<OverlayState>>();
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Ok(app_data_dir) = app.path().app_data_dir() {
            if let Ok(fresh_cfg) = crate::config::load_config(&app_data_dir) {
                state_guard.config = Some(fresh_cfg);
            }
        }
        let cfg = state_guard
            .config
            .clone()
            .ok_or_else(|| "Config belum dimuat di memori aplikasi".to_string())?;
        (
            cfg.groq_api_keys,
            cfg.openrouter_api_keys,
            state_guard.qa_generation,
            cfg.obsidian_vault_path,
        )
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Gagal inisialisasi HTTP client: {}", e))?;

    // 1. STT via Groq dengan loop fallback API key
    let transcript_res = crate::execute_fallback_keys(&groq_keys, |key| {
        let key = key.to_string();
        let client = client.clone();
        let bytes = args.bytes.clone();
        async move { crate::call_groq_stt(&client, &key, &bytes).await }
    })
    .await;

    // Discard stale check setelah STT: jika F8 sudah ditekan ulang, buang hasil & skip call AI
    if !crate::is_qa_generation_current(&app, current_gen) {
        println!(
            "[QA] Request generasi {} sudah stale setelah STT, discard transcript & skip AI call.",
            current_gen
        );
        return Ok(());
    }

    let transcript = match transcript_res {
        Ok(t) => {
            let _ = app.emit(
                "transcript:result",
                crate::TranscriptResultPayload { text: t.clone() },
            );
            t
        }
        Err(err_msg) => {
            let _ = app.emit(
                "qa:error",
                crate::QaErrorPayload {
                    stage: "stt".to_string(),
                    message: err_msg.clone(),
                },
            );
            return Err(err_msg);
        }
    };

    // 2. Ekstraksi konteks catatan Obsidian jika vault_path dikonfigurasi
    let vault_context = if let Some(ref path_str) = obsidian_vault_path {
        let path = std::path::Path::new(path_str);
        match crate::vault::scan_vault_markdown_files(path) {
            Ok(docs) => {
                let ctx = crate::vault::extract_relevant_context(&docs, &transcript, 3000);
                if let Some(ref matched) = ctx {
                    println!(
                        "[VAULT CONTEXT] Menyuntikkan {} karakter konteks Obsidian ke prompt OpenRouter",
                        matched.len()
                    );
                }
                ctx
            }
            Err(err) => {
                eprintln!(
                    "[VAULT ERROR] Gagal memindai Obsidian Vault di '{}': {}",
                    path_str, err
                );
                None
            }
        }
    } else {
        None
    };

    // 3. AI answering via OpenRouter dengan loop fallback API key dan injeksi konteks Obsidian
    let answer_res = crate::execute_fallback_keys(&openrouter_keys, |key| {
        let key = key.to_string();
        let client = client.clone();
        let transcript = transcript.clone();
        let vault_context = vault_context.clone();
        async move {
            crate::call_openrouter_ai(&client, &key, &transcript, vault_context.as_deref()).await
        }
    })
    .await;

    // Discard stale check setelah AI: jika F8 sudah ditekan ulang, buang hasil answer
    if !crate::is_qa_generation_current(&app, current_gen) {
        println!(
            "[QA] Request generasi {} sudah stale setelah AI, discard answer.",
            current_gen
        );
        return Ok(());
    }

    match answer_res {
        Ok(ans) => {
            let _ = app.emit("answer:result", crate::AnswerResultPayload { text: ans });
            Ok(())
        }
        Err(err_msg) => {
            let _ = app.emit(
                "qa:error",
                crate::QaErrorPayload {
                    stage: "ai".to_string(),
                    message: err_msg.clone(),
                },
            );
            Err(err_msg)
        }
    }
}

pub fn validate_ask_ai_prompt(prompt: &str) -> Result<String, String> {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        Err("Prompt pertanyaan tidak boleh kosong".to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn ask_ai_text(args: AskAiTextArgs, app: tauri::AppHandle) -> Result<(), String> {
    let prompt_text = validate_ask_ai_prompt(&args.prompt)?;

    let (openrouter_keys, current_gen, obsidian_vault_path) = {
        let state = app.state::<Mutex<OverlayState>>();
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Ok(app_data_dir) = app.path().app_data_dir() {
            if let Ok(fresh_cfg) = crate::config::load_config(&app_data_dir) {
                state_guard.config = Some(fresh_cfg);
            }
        }
        state_guard.qa_generation += 1;
        let gen = state_guard.qa_generation;
        let cfg = state_guard
            .config
            .clone()
            .ok_or_else(|| "Config belum dimuat di memori aplikasi".to_string())?;
        (cfg.openrouter_api_keys, gen, cfg.obsidian_vault_path)
    };

    // Emit transcript:result agar UI langsung menampilkan pertanyaan teks di panel
    let _ = app.emit(
        "transcript:result",
        crate::TranscriptResultPayload {
            text: prompt_text.clone(),
        },
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Gagal inisialisasi HTTP client: {}", e))?;

    // Ekstraksi konteks catatan Obsidian jika vault_path dikonfigurasi
    let vault_context = if let Some(ref path_str) = obsidian_vault_path {
        let path = std::path::Path::new(path_str);
        match crate::vault::scan_vault_markdown_files(path) {
            Ok(docs) => {
                let ctx = crate::vault::extract_relevant_context(&docs, &prompt_text, 3000);
                if let Some(ref matched) = ctx {
                    println!(
                        "[VAULT CONTEXT] Menyuntikkan {} karakter konteks Obsidian ke prompt OpenRouter (Text Prompt)",
                        matched.len()
                    );
                }
                ctx
            }
            Err(err) => {
                eprintln!(
                    "[VAULT ERROR] Gagal memindai Obsidian Vault di '{}': {}",
                    path_str, err
                );
                None
            }
        }
    } else {
        None
    };

    // AI answering via OpenRouter dengan loop fallback API key dan injeksi konteks Obsidian
    let answer_res = crate::execute_fallback_keys(&openrouter_keys, |key| {
        let key = key.to_string();
        let client = client.clone();
        let prompt_text = prompt_text.clone();
        let vault_context = vault_context.clone();
        async move {
            crate::call_openrouter_ai(&client, &key, &prompt_text, vault_context.as_deref()).await
        }
    })
    .await;

    // Discard stale check setelah AI: jika ada request baru (audio atau teks), buang hasil answer
    if !crate::is_qa_generation_current(&app, current_gen) {
        println!(
            "[QA] Request generasi {} sudah stale setelah AI, discard answer.",
            current_gen
        );
        return Ok(());
    }

    match answer_res {
        Ok(ans) => {
            let _ = app.emit("answer:result", crate::AnswerResultPayload { text: ans });
            Ok(())
        }
        Err(err_msg) => {
            let _ = app.emit(
                "qa:error",
                crate::QaErrorPayload {
                    stage: "ai".to_string(),
                    message: err_msg.clone(),
                },
            );
            Err(err_msg)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn ask_ai_screen(app: tauri::AppHandle) -> Result<(), String> {
    crate::trigger_screen_qa(&app).await
}

#[tauri::command]
#[specta::specta]
pub async fn ask_ai_selected_text(app: tauri::AppHandle) -> Result<(), String> {
    crate::trigger_selected_text_qa(&app).await
}

#[tauri::command]
#[specta::specta]
pub fn get_app_state(state: tauri::State<Mutex<OverlayState>>) -> Result<AppState, String> {
    let state_guard = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let font_size = state_guard
        .config
        .as_ref()
        .map(|c| c.font_size)
        .unwrap_or(config::DEFAULT_FONT_SIZE);

    Ok(AppState {
        font_size,
        qa_available: state_guard.qa_available,
    })
}

#[cfg(debug_assertions)]
#[tauri::command]
#[specta::specta]
pub fn test_trigger_hotkey(
    args: TestTriggerHotkeyArgs,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if !is_e2e_test_mode() {
        return Err("Command test_trigger_hotkey hanya diizinkan dalam E2E_TEST_MODE".to_string());
    }

    match (
        args.hotkey.trim().to_uppercase().as_str(),
        args.state.trim().to_lowercase().as_str(),
    ) {
        ("F9", "pressed") => {
            toggle_overlay_visibility(&app);
            Ok(())
        }
        ("F8", "pressed") => {
            handle_f8_pressed_event(&app);
            Ok(())
        }
        ("F8", "released") => {
            handle_f8_released_event(&app);
            Ok(())
        }
        ("CTRL+F10", "pressed") | ("F10", "pressed") => {
            use tauri::Emitter;
            let _ = app.emit("notes:cycle-tab", crate::NotesCycleTabPayload {});
            Ok(())
        }
        ("CTRL+F11", "pressed") | ("F11", "pressed") | ("CTRL+F12", "pressed") | ("F12", "pressed") => {
            use tauri::Emitter;
            let _ = app.emit("notes:toggle-find", crate::NotesToggleFindPayload {});
            Ok(())
        }
        _ => Err(
            "Kombinasi hotkey/state simulasi tidak valid (didukung: F9 Pressed, F8 Pressed/Released, CTRL+F10 Pressed, CTRL+F11 Pressed)"
                .to_string(),
        ),
    }
}

#[tauri::command]
#[specta::specta]
pub fn copy_to_clipboard(args: CopyToClipboardArgs) -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::clipboard::set_clipboard_text(&args.text)
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        Err("Clipboard copy hanya didukung di Windows".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_matching_dialog_window_different_process_rejected() {
        assert!(!is_matching_dialog_window(2001, 9999, 1001, 1234));
    }

    #[test]
    fn test_is_matching_dialog_window_main_hwnd_rejected() {
        // HWND sama dengan main_hwnd harus di-reject walau PID sama
        assert!(!is_matching_dialog_window(1001, 1234, 1001, 1234));
    }

    #[test]
    fn test_is_matching_dialog_window_valid_dialog_accepted() {
        // HWND baru milik proses yang sama harus di-accept
        assert!(is_matching_dialog_window(2001, 1234, 1001, 1234));
    }

    #[test]
    fn test_is_matching_dialog_window_null_hwnd_rejected() {
        assert!(!is_matching_dialog_window(0, 1234, 1001, 1234));
    }

    #[test]
    fn test_validate_ask_ai_prompt_empty_rejected() {
        assert!(validate_ask_ai_prompt("").is_err());
        assert!(validate_ask_ai_prompt("   ").is_err());
        assert!(validate_ask_ai_prompt("\n\t  \r").is_err());
    }

    #[test]
    fn test_validate_ask_ai_prompt_valid_trimmed_accepted() {
        let res = validate_ask_ai_prompt("  Halo apa kabar?  ");
        assert_eq!(res, Ok("Halo apa kabar?".to_string()));
    }
}
