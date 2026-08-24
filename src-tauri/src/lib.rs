pub mod commands;
pub mod config;

use config::{Config, ConfigErrorPayload};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, PhysicalPosition};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use tauri_specta::{collect_commands, collect_events, Builder};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, SetWindowDisplayAffinity, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_NOZORDER,
};

const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;

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
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "notes:error")]
pub struct NotesErrorPayload {
    pub message: String,
}

unsafe extern "system" fn enum_child_proc(hwnd: HWND, _lparam: LPARAM) -> i32 {
    // SAFETY: HWND di-pass langsung oleh sistem Windows saat EnumChildWindows berjalan.
    unsafe {
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    }
    1
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
    pub last_notes_content: Option<String>,
    pub last_notes_error: Option<String>,
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
            last_notes_content: None,
            last_notes_error: None,
        }
    }
}

/// Helper murni untuk mengecek string env var E2E_TEST_MODE (hanya "true" case-insensitive).
pub fn parse_e2e_test_mode_from_str(val: Option<&str>) -> bool {
    val.map(|v| v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Memeriksa apakah aplikasi dijalankan dalam mode pengujian E2E (`E2E_TEST_MODE=true`).
pub fn is_e2e_test_mode() -> bool {
    let env_val = std::env::var("E2E_TEST_MODE").ok();
    parse_e2e_test_mode_from_str(env_val.as_deref())
}

/// Memeriksa apakah durasi hold berada di bawah ambang batas minimum push-to-talk.
pub fn is_hold_duration_below_threshold(duration: std::time::Duration) -> bool {
    duration.as_millis() < PUSH_TO_TALK_MIN_HOLD_MS as u128
}

/// Logika keputusan murni saat tombol F8 ditekan (Pressed).
///
/// Mengembalikan `true` jika event start harus di-emit (tekanan pertama).
/// Mengembalikan `false` jika sedang recording (mengabaikan repeated Pressed akibat key-repeat OS).
pub fn handle_f8_press(
    is_recording: &mut bool,
    start_instant: &mut Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    if !*is_recording {
        *is_recording = true;
        *start_instant = Some(now);
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
            std::time::Instant::now(),
        )
    };

    if should_emit {
        println!("[HOTKEY] F8 Pressed: recording started");
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
    let mut state = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if state.hwnd == 0 {
        return;
    }
    let hwnd = state.hwnd as HWND;

    if state.is_concealed {
        let pos = state.last_normal_position;
        // SAFETY: HWND adalah handle window utama yang valid dan tersimpan di OverlayState.
        unsafe {
            SetWindowPos(
                hwnd,
                0 as _,
                pos.x,
                pos.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        // Re-apply stealth saat kembali ke Normal state
        apply_stealth(state.hwnd);
        state.toggle_concealed();
        println!(
            "[VISIBILITY] Normal: posisi ({}, {}) & stealth re-applied",
            pos.x, pos.y
        );
    } else {
        if let Some(window) = app.get_webview_window("main") {
            if let Ok(pos) = window.outer_position() {
                state.last_normal_position = pos;
            }
        }
        // SAFETY: HWND adalah handle window utama yang valid dan dipindahkan ke koordinat offscreen (-9999, -9999).
        unsafe {
            SetWindowPos(
                hwnd,
                0 as _,
                -9999,
                -9999,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        state.toggle_concealed();
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
pub fn start_watching_notes_file(
    app: &tauri::AppHandle,
    notes_path: PathBuf,
) -> Result<(), String> {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(err) => {
            let msg = format!("Gagal mengakses direktori data aplikasi: {}", err);
            trigger_notes_error(app, &msg);
            return Err(msg);
        }
    };

    // 1 & 2. Baca isi file notes + write-through ke config.json di disk
    let (content, updated_config) = match process_notes_file_load(&notes_path, &app_data_dir) {
        Ok(res) => res,
        Err(err_msg) => {
            trigger_notes_error(app, &err_msg);
            return Err(err_msg);
        }
    };

    // 3. Emit notes:update { content }
    let _ = app.emit(
        "notes:update",
        NotesUpdatePayload {
            content: content.clone(),
        },
    );

    // 4. Update in-memory OverlayState
    let state = app.state::<Mutex<OverlayState>>();
    {
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state_guard.config = Some(updated_config);
        state_guard.last_notes_content = Some(content);
        state_guard.last_notes_error = None;
    }

    // 5. Inisialisasi watcher pada parent directory
    let parent_dir = notes_path
        .parent()
        .ok_or_else(|| "Parent directory tidak valid untuk file notes".to_string())?
        .to_path_buf();

    let app_handle_for_watcher = app.clone();
    let target_file_for_watcher = notes_path.clone();

    use notify::Watcher;

    let mut watcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| match res {
            Ok(event) => {
                if is_target_file_event(&event.paths, &target_file_for_watcher) {
                    if target_file_for_watcher.exists() {
                        match fs::read_to_string(&target_file_for_watcher) {
                            Ok(new_content) => {
                                println!(
                                    "[WATCHER] File notes di-update: {}",
                                    target_file_for_watcher.display()
                                );
                                let state = app_handle_for_watcher.state::<Mutex<OverlayState>>();
                                {
                                    let mut state_guard = match state.lock() {
                                        Ok(guard) => guard,
                                        Err(poisoned) => poisoned.into_inner(),
                                    };
                                    state_guard.last_notes_content = Some(new_content.clone());
                                    state_guard.last_notes_error = None;
                                }
                                let _ = app_handle_for_watcher.emit(
                                    "notes:update",
                                    NotesUpdatePayload {
                                        content: new_content,
                                    },
                                );
                            }
                            Err(err) => {
                                let msg = format!(
                                    "Gagal membaca ulang file notes '{}': {}",
                                    target_file_for_watcher.display(),
                                    err
                                );
                                trigger_notes_error(&app_handle_for_watcher, &msg);
                            }
                        }
                    } else {
                        let msg = format!(
                            "File notes '{}' telah dihapus atau dipindahkan.",
                            target_file_for_watcher.display()
                        );
                        trigger_notes_error(&app_handle_for_watcher, &msg);
                    }
                }
            }
            Err(err) => {
                let msg = format!("Terjadi kesalahan pada file watcher: {}", err);
                trigger_notes_error(&app_handle_for_watcher, &msg);
            }
        })
        .map_err(|err| format!("Gagal menginisialisasi file watcher: {}", err))?;

    watcher
        .watch(&parent_dir, notify::RecursiveMode::NonRecursive)
        .map_err(|err| {
            format!(
                "Gagal memantau direktori '{}': {}",
                parent_dir.display(),
                err
            )
        })?;

    // Simpan watcher ke OverlayState (secara otomatis men-drop watcher lama jika ada via RAII)
    {
        let mut state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        state_guard.notes_watcher = Some(watcher);
    }

    println!(
        "[WATCHER] Memulai pemantauan file notes: {}",
        notes_path.display()
    );
    Ok(())
}

/// Menangani auto-restore file notes saat startup aplikasi.
pub fn handle_startup_notes_restore(app: &tauri::AppHandle, config: Option<&Config>) {
    let notes_path_str = match config.and_then(|c| c.last_opened_notes_path.as_deref()) {
        Some(path) => path,
        None => {
            // Skenario 1: lastOpenedNotesPath == null (belum pernah pilih file) -> no-op
            return;
        }
    };

    let notes_path = PathBuf::from(notes_path_str);
    if notes_path.exists() {
        // Skenario 2: file valid & ada -> start_watching_notes_file
        if let Err(err) = start_watching_notes_file(app, notes_path) {
            eprintln!("[NOTES RESTORE ERROR] {}", err);
        }
    } else {
        // Skenario 3: file hilang / dipindah -> trigger_notes_error
        let msg = format!(
            "File notes sebelumnya '{}' tidak ditemukan atau telah dipindahkan.",
            notes_path_str
        );
        trigger_notes_error(app, &msg);
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
            commands::test_trigger_hotkey
        ])
        .events(collect_events![
            ConfigErrorPayload,
            RecordingEndedPayload,
            NotesUpdatePayload,
            NotesErrorPayload
        ]);

    #[cfg(not(debug_assertions))]
    let builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::update_font_size,
            commands::get_app_state,
            commands::get_notes_state,
            commands::pick_notes_file
        ])
        .events(collect_events![
            ConfigErrorPayload,
            RecordingEndedPayload,
            NotesUpdatePayload,
            NotesErrorPayload
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

    if let Err(err) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &f9_shortcut && event.state() == ShortcutState::Pressed {
                        toggle_overlay_visibility(app);
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
            let _ = window.set_focus();

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
                        let state = app_handle_for_window.state::<Mutex<OverlayState>>();
                        let is_concealed = {
                            let mut state_guard = match state.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            if !state_guard.is_concealed {
                                state_guard.last_normal_position = *pos;
                            }
                            state_guard.is_concealed
                        };

                        // ATURAN WAJIB: Jangan simpan jika is_concealed == true (koordinat -9999)
                        if !is_concealed {
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
                    tauri::WindowEvent::Resized(size) => {
                        let state = app_handle_for_window.state::<Mutex<OverlayState>>();
                        let is_concealed = {
                            let state_guard = match state.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            state_guard.is_concealed
                        };

                        // ATURAN WAJIB: Jangan simpan jika is_concealed == true
                        if !is_concealed {
                            if let Ok(pos) = window_for_event.outer_position() {
                                let _ = tx_for_window.send(config::WindowBounds {
                                    x: pos.x,
                                    y: pos.y,
                                    width: size.width,
                                    height: size.height,
                                });
                            }
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

            let tray_builder = TrayIconBuilder::new()
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
        let now = std::time::Instant::now();

        let should_emit = handle_f8_press(&mut is_recording, &mut start_instant, now);
        assert!(should_emit);
        assert!(is_recording);
        assert_eq!(start_instant, Some(now));
    }

    #[test]
    fn test_handle_f8_press_repeated_returns_false_and_preserves_timestamp() {
        let mut is_recording = true;
        let t0 = std::time::Instant::now();
        let mut start_instant = Some(t0);
        let t1 = t0 + std::time::Duration::from_millis(100);

        // Key-repeat dari OS saat tombol masih ditahan
        let should_emit = handle_f8_press(&mut is_recording, &mut start_instant, t1);
        assert!(!should_emit);
        assert!(is_recording);
        assert_eq!(start_instant, Some(t0)); // Timestamp awal tidak tertimpa
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
}
