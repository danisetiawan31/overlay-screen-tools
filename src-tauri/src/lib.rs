pub mod commands;
pub mod config;

use config::{Config, ConfigErrorPayload};
use std::sync::Mutex;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, PhysicalPosition};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_specta::{collect_commands, collect_events, Builder};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, SetWindowDisplayAffinity, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_NOZORDER,
};

const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;

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
        }
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
    let builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![commands::update_font_size])
        .events(collect_events![ConfigErrorPayload]);

    #[cfg(debug_assertions)]
    if let Err(err) = builder.export(
        specta_typescript::Typescript::default(),
        "../src/bindings.ts",
    ) {
        eprintln!("[SPECTA] Failed to export typescript bindings: {}", err);
    }

    let f9_shortcut = Shortcut::new(None, Code::F9);

    if let Err(err) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &f9_shortcut && event.state() == ShortcutState::Pressed {
                        toggle_overlay_visibility(app);
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
                    let _ = app.emit("config:error", ConfigErrorPayload { message: err_msg });
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
                    let _ = app.emit("config:error", ConfigErrorPayload { message: err_msg });
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

            if let Err(err) = app.global_shortcut().register(f9_shortcut) {
                eprintln!("[ERROR] Gagal mendaftarkan F9 shortcut: {}", err);
            } else {
                println!("[POC] F9 shortcut terdaftar - siap ditest");
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
}
