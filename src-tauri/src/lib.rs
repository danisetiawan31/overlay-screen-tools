pub mod commands;
pub mod config;

use config::{Config, ConfigErrorPayload};
use serde::{Deserialize, Serialize};
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(debug_assertions)]
    let builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::update_font_size,
            commands::get_app_state,
            commands::test_trigger_hotkey
        ])
        .events(collect_events![ConfigErrorPayload, RecordingEndedPayload]);

    #[cfg(not(debug_assertions))]
    let builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::update_font_size,
            commands::get_app_state
        ])
        .events(collect_events![ConfigErrorPayload, RecordingEndedPayload]);

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
}
