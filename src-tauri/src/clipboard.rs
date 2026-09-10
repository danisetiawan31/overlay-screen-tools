use std::thread::sleep;
use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::{
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
        },
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
    },
    UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL,
    },
};

const VK_KEY_C: u16 = 0x43;
const CF_UNICODETEXT: u32 = 13;

/// Mensimulasikan penekanan tombol `Ctrl + C` untuk menyalin teks yang sedang aktif disorot/diseleksi.
#[cfg(windows)]
pub fn simulate_ctrl_c() {
    unsafe {
        let mut inputs: [INPUT; 4] = [
            // 1. Ctrl Down
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // 2. C Down
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_KEY_C,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // 3. C Up
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_KEY_C,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // 4. Ctrl Up
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        let sent = SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );

        if sent != inputs.len() as u32 {
            eprintln!("[CLIPBOARD] Peringatan: SendInput Ctrl+C hanya terkirim {} dari 4 input", sent);
        }
    }
}

#[cfg(not(windows))]
pub fn simulate_ctrl_c() {}

/// Membaca teks Unicode dari clipboard Windows secara aman.
#[cfg(windows)]
pub fn get_clipboard_text() -> Result<String, String> {
    unsafe {
        // Coba buka clipboard dengan retry singkat jika clipboard sedang dipakai proses lain
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(std::ptr::null_mut()) != 0 {
                opened = true;
                break;
            }
            sleep(Duration::from_millis(15));
        }

        if !opened {
            return Err("Gagal membuka clipboard sistem (sedang digunakan proses lain)".to_string());
        }

        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            CloseClipboard();
            return Err("Tidak ada data teks Unicode pada clipboard".to_string());
        }

        let ptr = GlobalLock(handle) as *const u16;
        if ptr.is_null() {
            CloseClipboard();
            return Err("Gagal mengunci memori clipboard (GlobalLock)".to_string());
        }

        // Cari panjang string UTF-16 yang diakhiri null character
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }

        let slice = std::slice::from_raw_parts(ptr, len);
        let text = String::from_utf16_lossy(slice);

        GlobalUnlock(handle);
        CloseClipboard();

        Ok(text)
    }
}

#[cfg(not(windows))]
pub fn get_clipboard_text() -> Result<String, String> {
    Err("Clipboard text capture hanya didukung di Windows".to_string())
}

/// Menulis teks Unicode ke clipboard Windows secara native tanpa bergantung pada fokus window atau Chromium.
#[cfg(windows)]
pub fn set_clipboard_text(text: &str) -> Result<(), String> {
    let utf16_units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let num_bytes = utf16_units.len() * std::mem::size_of::<u16>();

    unsafe {
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(std::ptr::null_mut()) != 0 {
                opened = true;
                break;
            }
            sleep(Duration::from_millis(15));
        }

        if !opened {
            return Err("Gagal membuka clipboard sistem (sedang digunakan proses lain)".to_string());
        }

        EmptyClipboard();

        let h_mem = GlobalAlloc(GMEM_MOVEABLE, num_bytes);
        if h_mem.is_null() {
            CloseClipboard();
            return Err("Gagal mengalokasikan memori clipboard (GlobalAlloc)".to_string());
        }

        let ptr = GlobalLock(h_mem) as *mut u16;
        if ptr.is_null() {
            CloseClipboard();
            return Err("Gagal mengunci memori clipboard (GlobalLock)".to_string());
        }

        std::ptr::copy_nonoverlapping(utf16_units.as_ptr(), ptr, utf16_units.len());
        GlobalUnlock(h_mem);

        if SetClipboardData(CF_UNICODETEXT, h_mem as _).is_null() {
            CloseClipboard();
            return Err("Gagal menetapkan data ke clipboard sistem (SetClipboardData)".to_string());
        }

        CloseClipboard();
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn set_clipboard_text(_text: &str) -> Result<(), String> {
    Err("Clipboard write hanya didukung di Windows".to_string())
}

/// Menangkap teks yang sedang disorot/diseleksi secara senyap:
/// 1. Simulasikan Ctrl+C
/// 2. Tunggu jeda singkat agar aplikasi target memproses WM_COPY
/// 3. Baca clipboard dan validasi string
pub fn capture_selected_text_silently() -> Result<String, String> {
    // 1. Simulasikan penekanan Ctrl+C
    simulate_ctrl_c();

    // 2. Berikan jeda waktu 60ms agar OS dan aplikasi aktif (Chrome/Edge/IDE) selesai menyalin
    sleep(Duration::from_millis(60));

    // 3. Baca isi teks dari clipboard
    let text = get_clipboard_text()?;
    validate_captured_text(&text)
}

/// Validasi string teks hasil salinan: menghapus whitespace di ujung dan menolak string kosong.
pub fn validate_captured_text(text: &str) -> Result<String, String> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return Err(
            "Teks yang disalin kosong. Pastikan teks soal sudah disorot/diblok dengan mouse."
                .to_string(),
        );
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_selected_text_validation_empty() {
        assert!(validate_captured_text("").is_err());
        assert!(validate_captured_text("   \n\t  ").is_err());
    }

    #[test]
    fn test_capture_selected_text_validation_valid() {
        let res = validate_captured_text("  Selesaikan algoritma QuickSort  ");
        assert_eq!(res, Ok("Selesaikan algoritma QuickSort".to_string()));
    }

    #[test]
    fn test_utf16_lossy_conversion() {
        let sample = "Halo AI, tolong selesaikan soal ini";
        let u16_vec: Vec<u16> = sample.encode_utf16().collect();
        let decoded = String::from_utf16_lossy(&u16_vec);
        assert_eq!(decoded, sample);
    }

    #[test]
    #[cfg(windows)]
    fn test_set_and_get_clipboard_text_roundtrip() {
        let test_payload = "function testClipboard() { return 42; }";
        let set_res = set_clipboard_text(test_payload);
        assert!(set_res.is_ok(), "set_clipboard_text failed: {:?}", set_res);
        let get_res = get_clipboard_text();
        assert!(get_res.is_ok(), "get_clipboard_text failed: {:?}", get_res);
        assert_eq!(get_res.unwrap(), test_payload);
    }
}
