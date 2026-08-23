use std::sync::Mutex;
use tauri::{Manager, PhysicalPosition};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_specta::{collect_commands, Builder};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, SetWindowDisplayAffinity, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
};

const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;

unsafe extern "system" fn enum_child_proc(hwnd: HWND, _lparam: LPARAM) -> i32 {
    SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    1
}

fn apply_stealth(hwnd_val: isize) {
    unsafe {
        let hwnd = hwnd_val as HWND;
        let res = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        println!("[STEALTH] SetWindowDisplayAffinity main HWND ({:?}): result={}", hwnd, res);
        EnumChildWindows(hwnd, Some(enum_child_proc), 0);
        println!("[STEALTH] Applied WDA_EXCLUDEFROMCAPTURE to all child WebViews");
    }
}

struct OverlayState {
    is_concealed: bool,
    last_normal_position: PhysicalPosition<i32>,
    hwnd: isize,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            is_concealed: false,
            last_normal_position: PhysicalPosition::new(100, 100),
            hwnd: 0,
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = Builder::<tauri::Wry>::new()
        .commands(collect_commands![]);

    #[cfg(debug_assertions)]
    builder
        .export(specta_typescript::Typescript::default(), "../src/bindings.ts")
        .expect("Failed to export typescript bindings");

    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .manage(Mutex::new(OverlayState::default()))
        .setup(move |app| {
            builder.mount_events(app);
            let window = app
                .get_webview_window("main")
                .expect("main window not found");

            let _ = window.set_always_on_top(true);
            let _ = window.center();
            let _ = window.show();
            let _ = window.set_focus();

            let hwnd_raw = window.hwnd().expect("failed to get window HWND").0 as isize;
            println!("[STEALTH] HWND didapatkan: {:?}", hwnd_raw);
            
            // 1. Terapkan stealth ke main HWND dan seluruh child HWND (WebView2)
            apply_stealth(hwnd_raw);

            // 2. Simpan posisi awal & HWND
            if let Ok(pos) = window.outer_position() {
                let state = app.state::<Mutex<OverlayState>>();
                let mut state = state.lock().unwrap();
                state.last_normal_position = pos;
                state.hwnd = hwnd_raw;
            }

            // 3. Webview & Window event listener untuk Re-Apply Stealth secara otomatis
            let hwnd_for_webview = hwnd_raw;
            window.on_webview_event(move |_event| {
                apply_stealth(hwnd_for_webview);
                println!("[STEALTH] Re-applied pada WebviewEvent");
            });

            let hwnd_for_window = hwnd_raw;
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(true) = event {
                    apply_stealth(hwnd_for_window);
                    println!("[STEALTH] Re-applied pada WindowEvent::Focused");
                }
            });

            // 4. Shortcut F9 Toggle
            let f9_shortcut = Shortcut::new(None, Code::F9);

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, shortcut, event| {
                        if shortcut != &f9_shortcut || event.state() != ShortcutState::Pressed {
                            return;
                        }

                        let state = app.state::<Mutex<OverlayState>>();
                        let mut state = state.lock().unwrap();
                        let hwnd = state.hwnd as HWND;

                        if state.is_concealed {
                            let pos = state.last_normal_position;
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
                            println!("[POC] F9 -> Normal: posisi ({}, {}) & stealth re-applied", pos.x, pos.y);
                        } else {
                            if let Some(window) = app.get_webview_window("main") {
                                if let Ok(pos) = window.outer_position() {
                                    state.last_normal_position = pos;
                                }
                            }
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
            )?;

            app.handle().plugin(tauri_plugin_dialog::init())?;

            app.global_shortcut().register(f9_shortcut)?;
            println!("[POC] F9 shortcut terdaftar - siap ditest");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
