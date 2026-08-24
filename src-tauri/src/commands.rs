use crate::config;
use crate::OverlayState;
#[cfg(debug_assertions)]
use crate::{
    handle_f8_pressed_event, handle_f8_released_event, is_e2e_test_mode, toggle_overlay_visibility,
};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub font_size: u32,
    pub qa_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PickNotesFileResponse {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotesState {
    pub content: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SendAudioBlobArgs {
    pub bytes: Vec<u8>,
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

    Ok(NotesState {
        content: state_guard.last_notes_content.clone(),
        error: state_guard.last_notes_error.clone(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn pick_notes_file(app: tauri::AppHandle) -> Result<Option<PickNotesFileResponse>, String> {
    use tauri_plugin_dialog::DialogExt;

    let file_path = app
        .dialog()
        .file()
        .add_filter("Markdown", &["md"])
        .blocking_pick_file();

    match file_path {
        Some(fp) => {
            let path_buf = fp.into_path().map_err(|err| err.to_string())?;
            let path_str = path_buf.to_string_lossy().to_string();
            crate::start_watching_notes_file(&app, path_buf)?;
            Ok(Some(PickNotesFileResponse { path: path_str }))
        }
        None => Ok(None),
    }
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

    let (groq_keys, openrouter_keys, current_gen) = {
        let state = app.state::<Mutex<OverlayState>>();
        let state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let cfg = state_guard
            .config
            .clone()
            .ok_or_else(|| "Config belum dimuat di memori aplikasi".to_string())?;
        (
            cfg.groq_api_keys,
            cfg.openrouter_api_keys,
            state_guard.qa_generation,
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

    // 2. AI answering via OpenRouter dengan loop fallback API key
    let answer_res = crate::execute_fallback_keys(&openrouter_keys, |key| {
        let key = key.to_string();
        let client = client.clone();
        let transcript = transcript.clone();
        async move { crate::call_openrouter_ai(&client, &key, &transcript).await }
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
        _ => Err(
            "Kombinasi hotkey/state simulasi tidak valid (didukung: F9 Pressed, F8 Pressed/Released)"
                .to_string(),
        ),
    }
}
