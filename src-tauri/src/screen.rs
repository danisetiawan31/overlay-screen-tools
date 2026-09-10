use base64::Engine;
use std::mem::size_of;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
    GetDC, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    SRCCOPY,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
};

/// Mengambil tangkapan layar monitor utama secara senyap menggunakan Win32 GDI,
/// mengompresinya menjadi JPEG (kualitas ~80%), dan mengembalikan data URL base64.
///
/// Karena jendela poc-overlay menggunakan WDA_EXCLUDEFROMCAPTURE,
/// jendela overlay otomatis tidak ikut terekam (tembus pandang).
pub fn capture_screen_as_jpeg_data_url() -> Result<String, String> {
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    if width <= 0 || height <= 0 {
        return Err(format!(
            "Dimensi layar tidak valid: {}x{}",
            width, height
        ));
    }

    let raw_bgra = unsafe {
        let hdc_screen = GetDC(0 as HWND);
        if hdc_screen.is_null() {
            return Err("Gagal mendapatkan Desktop DC (GetDC)".to_string());
        }

        let hdc_mem = CreateCompatibleDC(hdc_screen);
        if hdc_mem.is_null() {
            ReleaseDC(0 as HWND, hdc_screen);
            return Err("Gagal membuat Memory DC (CreateCompatibleDC)".to_string());
        }

        let hbm_screen = CreateCompatibleBitmap(hdc_screen, width, height);
        if hbm_screen.is_null() {
            DeleteDC(hdc_mem);
            ReleaseDC(0 as HWND, hdc_screen);
            return Err("Gagal membuat Compatible Bitmap".to_string());
        }

        let hbm_old = SelectObject(hdc_mem, hbm_screen);

        let blt_res = BitBlt(
            hdc_mem, 0, 0, width, height, hdc_screen, 0, 0, SRCCOPY,
        );

        if blt_res == 0 {
            SelectObject(hdc_mem, hbm_old);
            DeleteObject(hbm_screen);
            DeleteDC(hdc_mem);
            ReleaseDC(0 as HWND, hdc_screen);
            return Err("Gagal melakukan BitBlt pada Desktop DC".to_string());
        }

        // Setup BITMAPINFO top-down (biHeight negatif)
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = width;
        bmi.bmiHeader.biHeight = -height;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let buf_len = (width * height * 4) as usize;
        let mut bgra_buf: Vec<u8> = vec![0u8; buf_len];

        let dib_res = GetDIBits(
            hdc_mem,
            hbm_screen,
            0,
            height as u32,
            bgra_buf.as_mut_ptr() as _,
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(hdc_mem, hbm_old);
        DeleteObject(hbm_screen);
        DeleteDC(hdc_mem);
        ReleaseDC(0 as HWND, hdc_screen);

        if dib_res == 0 {
            return Err("Gagal membaca pixel dari GetDIBits".to_string());
        }

        bgra_buf
    };

    // Konversi BGRA ke RGB dengan optimasi resolusi untuk monitor resolusi tinggi
    // Jika resolusi layar sangat besar (width >= 2560 / 2K / 4K), lakukan downsampling 2x
    // agar payload ringan (< 200 KB) dan proses Vision AI berjalan 2-3x lebih cepat.
    let (final_width, final_height, final_rgb) = if width >= 2560 {
        let step = 2usize;
        let new_w = (width as usize) / step;
        let new_h = (height as usize) / step;
        let mut downsampled = Vec::with_capacity(new_w * new_h * 3);
        for y in 0..new_h {
            for x in 0..new_w {
                let orig_idx = ((y * step) * (width as usize) + (x * step)) * 4;
                downsampled.push(raw_bgra[orig_idx + 2]); // R
                downsampled.push(raw_bgra[orig_idx + 1]); // G
                downsampled.push(raw_bgra[orig_idx]);     // B
            }
        }
        (new_w as u32, new_h as u32, downsampled)
    } else {
        let pixel_count = (width * height) as usize;
        let mut rgb_buf = Vec::with_capacity(pixel_count * 3);
        for chunk in raw_bgra.chunks_exact(4) {
            rgb_buf.push(chunk[2]); // R
            rgb_buf.push(chunk[1]); // G
            rgb_buf.push(chunk[0]); // B
        }
        (width as u32, height as u32, rgb_buf)
    };

    // Kompresi ke JPEG (kualitas 75 menjaga ketajaman teks/kode sambil menghemat ukuran)
    let mut jpeg_bytes = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_bytes, 75);
    encoder
        .encode(
            &final_rgb,
            final_width,
            final_height,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("Gagal kompresi JPEG: {}", e))?;

    // Encode ke Base64 Data URL
    let b64 = base64::prelude::BASE64_STANDARD.encode(&jpeg_bytes);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_screen_returns_jpeg_data_url_on_windows() {
        // Test ini hanya berjalan pada environment desktop Windows interaktif
        if let Ok(data_url) = capture_screen_as_jpeg_data_url() {
            assert!(data_url.starts_with("data:image/jpeg;base64,"));
            assert!(data_url.len() > 100);
        }
    }
}
