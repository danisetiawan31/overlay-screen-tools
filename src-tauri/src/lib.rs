pub mod config;

use config::{Config, ConfigErrorPayload};
use std::sync::Mutex;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![])
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
                    if shortcut != &f9_shortcut || event.state() != ShortcutState::Pressed {
                        return;
                    }

                    let state = app.state::<Mutex<OverlayState>>();
                    let mut state = match state.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
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
                        state.is_concealed = false;
                        println!(
                            "[POC] F9 -> Normal: posisi ({}, {}) & stealth re-applied",
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
                        state.is_concealed = true;
                        println!("[POC] F9 -> Concealed: dipindahkan ke (-9999, -9999)");
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

            match config::load_config(&app_data_dir) {
                Ok(loaded_cfg) => {
                    println!("[CONFIG] Berhasil memuat config.json");
                    let state = app.state::<Mutex<OverlayState>>();
                    let mut state_guard = match state.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    state_guard.config = Some(loaded_cfg);
                }
                Err(err_msg) => {
                    eprintln!("[CONFIG ERROR] {}", err_msg);
                    let _ = app.emit("config:error", ConfigErrorPayload { message: err_msg });
                }
            }

            let window = match app.get_webview_window("main") {
                Some(w) => w,
                None => {
                    eprintln!("[ERROR] Main window tidak ditemukan saat startup.");
                    return Ok(());
                }
            };

            let _ = window.set_always_on_top(true);
            let _ = window.center();
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

            if let Ok(pos) = window.outer_position() {
                let state = app.state::<Mutex<OverlayState>>();
                let mut state_guard = match state.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                state_guard.last_normal_position = pos;
                state_guard.hwnd = hwnd_raw;
            }

            // 3. Webview & Window event listener untuk Re-Apply Stealth secara otomatis
            let hwnd_for_webview = hwnd_raw;
            window.on_webview_event(move |_event| {
                if hwnd_for_webview != 0 {
                    apply_stealth(hwnd_for_webview);
                    println!("[STEALTH] Re-applied pada WebviewEvent");
                }
            });

            let hwnd_for_window = hwnd_raw;
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(true) = event {
                    if hwnd_for_window != 0 {
                        apply_stealth(hwnd_for_window);
                        println!("[STEALTH] Re-applied pada WindowEvent::Focused");
                    }
                }
            });

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
