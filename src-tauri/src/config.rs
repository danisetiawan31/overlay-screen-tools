use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE_NAME: &str = "config.json";
pub const DEFAULT_FONT_SIZE: u32 = 14;
pub const MIN_FONT_SIZE: u32 = 10;
pub const MAX_FONT_SIZE: u32 = 32;

// CATATAN KEAMANAN (AGENTS.md §10):
// Struct `Config`, `WindowBounds`, dan `HotkeysConfig` murni digunakan untuk persistensi lokal
// core (Rust). Struct ini TIDAK BOLEH mengimplementasikan `specta::Type` atau di-expose ke IPC
// frontend agar API key (openrouterApiKey & groqApiKey) tidak pernah bocor ke webview.

fn default_font_size() -> u32 {
    DEFAULT_FONT_SIZE
}

fn default_window_x() -> i32 {
    100
}

fn default_window_y() -> i32 {
    100
}

fn default_window_width() -> u32 {
    550
}

fn default_window_height() -> u32 {
    350
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowBounds {
    #[serde(default = "default_window_x")]
    pub x: i32,
    #[serde(default = "default_window_y")]
    pub y: i32,
    #[serde(default = "default_window_width")]
    pub width: u32,
    #[serde(default = "default_window_height")]
    pub height: u32,
}

impl Default for WindowBounds {
    fn default() -> Self {
        Self {
            x: 100,
            y: 100,
            width: 550,
            height: 350,
        }
    }
}

fn default_push_to_talk() -> String {
    "F8".to_string()
}

fn default_toggle_visibility() -> String {
    "F9".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeysConfig {
    #[serde(default = "default_push_to_talk")]
    pub push_to_talk: String,
    #[serde(default = "default_toggle_visibility")]
    pub toggle_visibility: String,
}

impl Default for HotkeysConfig {
    fn default() -> Self {
        Self {
            push_to_talk: "F8".to_string(),
            toggle_visibility: "F9".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub openrouter_api_keys: Vec<String>,
    pub groq_api_keys: Vec<String>,
    #[serde(default)]
    pub window_bounds: WindowBounds,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default)]
    pub last_opened_notes_path: Option<String>,
    #[serde(default)]
    pub obsidian_vault_path: Option<String>,
    #[serde(default)]
    pub hotkeys: HotkeysConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            openrouter_api_keys: Vec::new(),
            groq_api_keys: Vec::new(),
            window_bounds: WindowBounds::default(),
            font_size: DEFAULT_FONT_SIZE,
            last_opened_notes_path: None,
            obsidian_vault_path: None,
            hotkeys: HotkeysConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, tauri_specta::Event)]
#[tauri_specta(event_name = "config:error")]
pub struct ConfigErrorPayload {
    pub message: String,
}

/// Mendapatkan path file config.json di dalam direktori data aplikasi.
pub fn get_config_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(CONFIG_FILE_NAME)
}

/// Membaca dan memvalidasi file config.json dari direktori data aplikasi.
///
/// Mengembalikan `Err(String)` jika:
/// - File tidak ditemukan
/// - File gagal dibaca / format JSON malformed
/// - `openrouterApiKeys` atau `groqApiKeys` tidak memiliki minimal 1 key valid
pub fn load_config(app_data_dir: &Path) -> Result<Config, String> {
    let config_path = get_config_path(app_data_dir);

    if !config_path.exists() {
        return Err(format!(
            "File config.json tidak ditemukan di '{}'. Silakan buat dan isi API key Anda.",
            config_path.display()
        ));
    }

    let raw_content = fs::read_to_string(&config_path).map_err(|err| {
        format!(
            "Gagal membaca file config.json di '{}': {}",
            config_path.display(),
            err
        )
    })?;

    parse_and_validate_config(&raw_content)
}

/// Parse string JSON ke struct `Config` dan validasi API keys.
pub fn parse_and_validate_config(raw_json: &str) -> Result<Config, String> {
    let clean_json = raw_json.trim_start_matches('\u{feff}').trim();
    let config: Config = serde_json::from_str(clean_json).map_err(|err| {
        format!(
            "Format JSON di config.json tidak valid / malformed: {}",
            err
        )
    })?;

    let has_valid_openrouter = config
        .openrouter_api_keys
        .iter()
        .any(|k| !k.trim().is_empty());
    let has_valid_groq = config.groq_api_keys.iter().any(|k| !k.trim().is_empty());

    if !has_valid_openrouter || !has_valid_groq {
        return Err("openrouterApiKeys atau groqApiKeys di config.json belum memiliki key valid (minimal 1 key tidak kosong). Silakan isi API key tersebut.".to_string());
    }

    Ok(config)
}

/// Menyimpan struct `Config` ke file config.json di direktori data aplikasi.
pub fn save_config(app_data_dir: &Path, config: &Config) -> Result<(), String> {
    if !app_data_dir.exists() {
        fs::create_dir_all(app_data_dir).map_err(|err| {
            format!(
                "Gagal membuat direktori data aplikasi di '{}': {}",
                app_data_dir.display(),
                err
            )
        })?;
    }

    let config_path = get_config_path(app_data_dir);
    let serialized = serde_json::to_string_pretty(config)
        .map_err(|err| format!("Gagal memformat konfigurasi ke JSON: {}", err))?;

    fs::write(&config_path, serialized).map_err(|err| {
        format!(
            "Gagal menulis file config.json di '{}': {}",
            config_path.display(),
            err
        )
    })?;

    Ok(())
}

/// Memvalidasi range ukuran font (10–32 px).
pub fn validate_font_size(size: u32) -> Result<(), String> {
    if !(MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&size) {
        return Err(format!(
            "Ukuran font {}px di luar batas valid ({}–{}px).",
            size, MIN_FONT_SIZE, MAX_FONT_SIZE
        ));
    }
    Ok(())
}

/// Memperbarui ukuran font di file config.json.
///
/// Jika file config.json belum ada di app_data_dir, fungsi ini akan membuat
/// default Config dengan font_size baru dan menyimpannya.
pub fn update_config_font_size(app_data_dir: &Path, size: u32) -> Result<Config, String> {
    validate_font_size(size)?;

    let config_path = get_config_path(app_data_dir);
    let mut config = if config_path.exists() {
        let raw = fs::read_to_string(&config_path).map_err(|err| {
            format!(
                "Gagal membaca file config.json di '{}': {}",
                config_path.display(),
                err
            )
        })?;
        let clean = raw.trim_start_matches('\u{feff}').trim();
        serde_json::from_str::<Config>(clean).map_err(|err| {
            format!(
                "Format JSON di config.json tidak valid / malformed: {}",
                err
            )
        })?
    } else {
        Config::default()
    };

    config.font_size = size;
    save_config(app_data_dir, &config)?;

    Ok(config)
}

/// Memperbarui field `window_bounds` di `config.json`.
pub fn update_config_window_bounds(
    app_data_dir: &Path,
    bounds: WindowBounds,
) -> Result<Config, String> {
    let config_path = get_config_path(app_data_dir);
    let mut config = if config_path.exists() {
        let raw = fs::read_to_string(&config_path).map_err(|err| {
            format!(
                "Gagal membaca file config.json di '{}': {}",
                config_path.display(),
                err
            )
        })?;
        let clean = raw.trim_start_matches('\u{feff}').trim();
        serde_json::from_str::<Config>(clean).map_err(|err| {
            format!(
                "Format JSON di config.json tidak valid / malformed: {}",
                err
            )
        })?
    } else {
        Config::default()
    };

    config.window_bounds = bounds;
    save_config(app_data_dir, &config)?;

    Ok(config)
}

/// Memperbarui field `last_opened_notes_path` di `config.json`.
pub fn update_config_last_opened_notes_path(
    app_data_dir: &Path,
    path: Option<String>,
) -> Result<Config, String> {
    let config_path = get_config_path(app_data_dir);
    let mut config = if config_path.exists() {
        let raw = fs::read_to_string(&config_path).map_err(|err| {
            format!(
                "Gagal membaca file config.json di '{}': {}",
                config_path.display(),
                err
            )
        })?;
        let clean = raw.trim_start_matches('\u{feff}').trim();
        serde_json::from_str::<Config>(clean).map_err(|err| {
            format!(
                "Format JSON di config.json tidak valid / malformed: {}",
                err
            )
        })?
    } else {
        Config::default()
    };

    config.last_opened_notes_path = path;
    save_config(app_data_dir, &config)?;

    Ok(config)
}

/// Memperbarui field `obsidian_vault_path` di `config.json`.
pub fn update_config_obsidian_vault_path(
    app_data_dir: &Path,
    path: Option<String>,
) -> Result<Config, String> {
    let config_path = get_config_path(app_data_dir);
    let mut config = if config_path.exists() {
        let raw = fs::read_to_string(&config_path).map_err(|err| {
            format!(
                "Gagal membaca file config.json di '{}': {}",
                config_path.display(),
                err
            )
        })?;
        let clean = raw.trim_start_matches('\u{feff}').trim();
        serde_json::from_str::<Config>(clean).map_err(|err| {
            format!(
                "Format JSON di config.json tidak valid / malformed: {}",
                err
            )
        })?
    } else {
        Config::default()
    };

    config.obsidian_vault_path = path;
    save_config(app_data_dir, &config)?;

    Ok(config)
}

/// Representasi area monitor dalam koordinat fisik (pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl MonitorRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Memeriksa apakah setidaknya titik koordinat window (posisi x, y) berada di dalam salah satu monitor aktif.
pub fn is_window_within_monitors(bounds: &WindowBounds, monitors: &[MonitorRect]) -> bool {
    if monitors.is_empty() {
        return false;
    }

    monitors.iter().any(|m| {
        let monitor_right = m.x + m.width as i32;
        let monitor_bottom = m.y + m.height as i32;
        bounds.x >= m.x && bounds.x < monitor_right && bounds.y >= m.y && bounds.y < monitor_bottom
    })
}

/// Menentukan posisi window awal: menggunakan `saved_bounds` jika berada dalam monitor aktif,
/// atau fallback ke `default_bounds` jika berada di luar area semua monitor.
pub fn resolve_initial_window_bounds(
    saved_bounds: &WindowBounds,
    monitors: &[MonitorRect],
    default_bounds: &WindowBounds,
) -> WindowBounds {
    if is_window_within_monitors(saved_bounds, monitors) {
        saved_bounds.clone()
    } else {
        default_bounds.clone()
    }
}

/// Menyimpan `window_bounds` HANYA jika `is_concealed == false`.
///
/// Jika window sedang dalam keadaan concealed (`is_concealed == true`),
/// fungsi ini akan langsung mengembalikan `Ok(None)` tanpa menulis apapun ke disk.
pub fn persist_window_bounds_if_visible(
    app_data_dir: &Path,
    bounds: WindowBounds,
    is_concealed: bool,
) -> Result<Option<Config>, String> {
    if is_concealed {
        return Ok(None);
    }

    let updated = update_config_window_bounds(app_data_dir, bounds)?;
    Ok(Some(updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config_parsed_correctly() {
        let valid_json = r#"{
            "openrouterApiKeys": ["sk-or-v1-abc123test", "sk-or-v1-fallback456"],
            "groqApiKeys": ["gsk_test456key", "gsk_fallback789key"],
            "windowBounds": { "x": 120, "y": 80, "width": 600, "height": 400 },
            "fontSize": 16,
            "lastOpenedNotesPath": "C:\\notes\\session.md",
            "hotkeys": { "pushToTalk": "F8", "toggleVisibility": "F9" }
        }"#;

        let result = parse_and_validate_config(valid_json);
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(
            config.openrouter_api_keys,
            vec!["sk-or-v1-abc123test", "sk-or-v1-fallback456"]
        );
        assert_eq!(
            config.groq_api_keys,
            vec!["gsk_test456key", "gsk_fallback789key"]
        );
        assert_eq!(config.window_bounds.x, 120);
        assert_eq!(config.window_bounds.y, 80);
        assert_eq!(config.window_bounds.width, 600);
        assert_eq!(config.window_bounds.height, 400);
        assert_eq!(config.font_size, 16);
        assert_eq!(
            config.last_opened_notes_path,
            Some("C:\\notes\\session.md".to_string())
        );
        assert_eq!(config.hotkeys.push_to_talk, "F8");
        assert_eq!(config.hotkeys.toggle_visibility, "F9");
    }

    #[test]
    fn test_config_missing_hotkeys_and_optional_fields_uses_defaults() {
        let legacy_json = r#"{
            "openrouterApiKeys": ["sk-or-v1-abc123test"],
            "groqApiKeys": ["gsk_test456key"]
        }"#;

        let result = parse_and_validate_config(legacy_json);
        assert!(
            result.is_ok(),
            "Config legacy tanpa hotkeys harus tetap berhasil diparse"
        );

        let config = result.unwrap();
        assert_eq!(config.hotkeys.push_to_talk, "F8");
        assert_eq!(config.hotkeys.toggle_visibility, "F9");
        assert_eq!(config.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(config.window_bounds.width, 550);
        assert_eq!(config.window_bounds.height, 350);
        assert_eq!(config.last_opened_notes_path, None);
    }

    #[test]
    fn test_appdata_config_if_exists() {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let path = Path::new(&appdata).join("com.dnist.tauri-app");
            if path.join("config.json").exists() {
                let res = load_config(&path);
                assert!(
                    res.is_ok(),
                    "Config di AppData harus berhasil diload: {:?}",
                    res.err()
                );
            }
        }
    }

    #[test]
    fn test_missing_config_file() {
        let non_existent_dir = Path::new("non_existent_directory_for_test_12345");
        let result = load_config(non_existent_dir);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("tidak ditemukan"));
    }

    #[test]
    fn test_malformed_json() {
        let malformed_json = r#"{
            "openrouterApiKeys": ["sk-or-123"],
            "groqApiKeys": ["gsk-456"],
            "fontSize": "bukan_angka",
        }"#;

        let result = parse_and_validate_config(malformed_json);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("tidak valid / malformed"));
    }

    #[test]
    fn test_empty_api_keys() {
        // Kasus 1: OpenRouter keys array kosong atau hanya whitespace
        let empty_openrouter = r#"{
            "openrouterApiKeys": ["   ", ""],
            "groqApiKeys": ["gsk_valid_key"],
            "windowBounds": { "x": 0, "y": 0, "width": 400, "height": 600 },
            "fontSize": 14,
            "lastOpenedNotesPath": null,
            "hotkeys": { "pushToTalk": "F8", "toggleVisibility": "F9" }
        }"#;
        let res1 = parse_and_validate_config(empty_openrouter);
        assert!(res1.is_err());
        assert!(res1.unwrap_err().contains("belum memiliki key valid"));

        // Kasus 2: Groq keys array kosong
        let empty_groq = r#"{
            "openrouterApiKeys": ["sk-or-valid"],
            "groqApiKeys": [],
            "windowBounds": { "x": 0, "y": 0, "width": 400, "height": 600 },
            "fontSize": 14,
            "lastOpenedNotesPath": null,
            "hotkeys": { "pushToTalk": "F8", "toggleVisibility": "F9" }
        }"#;
        let res2 = parse_and_validate_config(empty_groq);
        assert!(res2.is_err());
        assert!(res2.unwrap_err().contains("belum memiliki key valid"));
    }

    #[test]
    fn test_font_size_validation_valid_size() {
        assert!(validate_font_size(14).is_ok());
        assert!(validate_font_size(18).is_ok());
        assert!(validate_font_size(24).is_ok());
    }

    #[test]
    fn test_font_size_validation_below_min_rejected() {
        let res_below = validate_font_size(9);
        assert!(res_below.is_err());
        assert!(res_below.unwrap_err().contains("di luar batas valid"));

        let res_zero = validate_font_size(0);
        assert!(res_zero.is_err());
    }

    #[test]
    fn test_font_size_validation_above_max_rejected() {
        let res_above = validate_font_size(33);
        assert!(res_above.is_err());
        assert!(res_above.unwrap_err().contains("di luar batas valid"));

        let res_large = validate_font_size(100);
        assert!(res_large.is_err());
    }

    #[test]
    fn test_font_size_validation_exact_boundaries_accepted() {
        // Batas bawah tepat (10 px)
        assert!(validate_font_size(10).is_ok());
        // Batas atas tepat (32 px)
        assert!(validate_font_size(32).is_ok());
    }

    #[test]
    fn test_update_config_font_size_persists_correctly() {
        let temp_dir = std::env::temp_dir().join("poc_overlay_test_font_size");
        let _ = fs::remove_dir_all(&temp_dir);

        // Test update font size ke nilai valid (18 px)
        let res = update_config_font_size(&temp_dir, 18);
        assert!(res.is_ok());
        let updated = res.unwrap();
        assert_eq!(updated.font_size, 18);

        // Verifikasi dari file di disk
        let file_path = get_config_path(&temp_dir);
        assert!(file_path.exists());
        let raw = fs::read_to_string(&file_path).unwrap();
        let from_file: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(from_file.font_size, 18);

        // Test update lagi ke nilai batas atas (32 px)
        let res32 = update_config_font_size(&temp_dir, 32);
        assert!(res32.is_ok());
        let raw32 = fs::read_to_string(&file_path).unwrap();
        let from_file32: Config = serde_json::from_str(&raw32).unwrap();
        assert_eq!(from_file32.font_size, 32);

        // Test update ke nilai invalid (35 px) - harus gagal dan tidak mengubah file
        let res_invalid = update_config_font_size(&temp_dir, 35);
        assert!(res_invalid.is_err());
        let raw_unchanged = fs::read_to_string(&file_path).unwrap();
        let from_file_unchanged: Config = serde_json::from_str(&raw_unchanged).unwrap();
        assert_eq!(from_file_unchanged.font_size, 32);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_monitor_validation_position_within_monitor_used_as_is() {
        let monitors = vec![
            MonitorRect::new(0, 0, 1920, 1080),
            MonitorRect::new(1920, 0, 1920, 1080),
        ];
        let default_bounds = WindowBounds {
            x: 100,
            y: 100,
            width: 550,
            height: 350,
        };

        // Posisi dalam monitor primer
        let saved_in_primary = WindowBounds {
            x: 200,
            y: 300,
            width: 500,
            height: 400,
        };
        let resolved = resolve_initial_window_bounds(&saved_in_primary, &monitors, &default_bounds);
        assert_eq!(resolved, saved_in_primary);

        // Posisi dalam monitor sekunder
        let saved_in_secondary = WindowBounds {
            x: 2100,
            y: 150,
            width: 500,
            height: 400,
        };
        let resolved_sec =
            resolve_initial_window_bounds(&saved_in_secondary, &monitors, &default_bounds);
        assert_eq!(resolved_sec, saved_in_secondary);
    }

    #[test]
    fn test_monitor_validation_outside_all_monitors_fallback_to_default() {
        let monitors = vec![MonitorRect::new(0, 0, 1920, 1080)];
        let default_bounds = WindowBounds {
            x: 100,
            y: 100,
            width: 550,
            height: 350,
        };

        // Posisi bekas monitor sekunder yang sudah dilepas (x: 2500 di luar monitor 1920)
        let disconnected_monitor_pos = WindowBounds {
            x: 2500,
            y: 200,
            width: 500,
            height: 400,
        };
        let resolved =
            resolve_initial_window_bounds(&disconnected_monitor_pos, &monitors, &default_bounds);
        assert_eq!(resolved, default_bounds);

        // Posisi koordinat concealed negatif (-9999, -9999)
        let concealed_pos = WindowBounds {
            x: -9999,
            y: -9999,
            width: 500,
            height: 400,
        };
        let resolved_concealed =
            resolve_initial_window_bounds(&concealed_pos, &monitors, &default_bounds);
        assert_eq!(resolved_concealed, default_bounds);
    }

    #[test]
    fn test_monitor_validation_empty_monitors_fallback_to_default() {
        let empty_monitors: Vec<MonitorRect> = Vec::new();
        let default_bounds = WindowBounds {
            x: 100,
            y: 100,
            width: 550,
            height: 350,
        };
        let saved = WindowBounds {
            x: 50,
            y: 50,
            width: 400,
            height: 300,
        };

        let resolved = resolve_initial_window_bounds(&saved, &empty_monitors, &default_bounds);
        assert_eq!(resolved, default_bounds);
    }

    #[test]
    fn test_persist_window_bounds_when_visible_writes_to_config() {
        let temp_dir = std::env::temp_dir().join("poc_overlay_test_wb_visible");
        let _ = fs::remove_dir_all(&temp_dir);

        let new_bounds = WindowBounds {
            x: 350,
            y: 220,
            width: 600,
            height: 450,
        };

        // Ketika is_concealed == false, penulisan ke disk harus terjadi
        let result = persist_window_bounds_if_visible(&temp_dir, new_bounds.clone(), false);
        assert!(result.is_ok());
        let opt_cfg = result.unwrap();
        assert!(opt_cfg.is_some());
        assert_eq!(opt_cfg.unwrap().window_bounds, new_bounds);

        // Verifikasi langsung dari file di disk
        let file_path = get_config_path(&temp_dir);
        assert!(file_path.exists());
        let raw = fs::read_to_string(&file_path).unwrap();
        let from_disk: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(from_disk.window_bounds, new_bounds);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_persist_window_bounds_when_concealed_does_not_write() {
        let temp_dir = std::env::temp_dir().join("poc_overlay_test_wb_concealed");
        let _ = fs::remove_dir_all(&temp_dir);

        // Siapkan config awal di disk
        let initial_bounds = WindowBounds {
            x: 150,
            y: 150,
            width: 500,
            height: 350,
        };
        let _ = update_config_window_bounds(&temp_dir, initial_bounds.clone());

        // Simulasikan event posisi concealed (-9999, -9999) dengan is_concealed == true
        let concealed_bounds = WindowBounds {
            x: -9999,
            y: -9999,
            width: 500,
            height: 350,
        };
        let result = persist_window_bounds_if_visible(&temp_dir, concealed_bounds, true);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);

        // Verifikasi bahwa data di disk TIDAK berubah (-9999 tidak pernah tertulis)
        let file_path = get_config_path(&temp_dir);
        let raw = fs::read_to_string(&file_path).unwrap();
        let from_disk: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(from_disk.window_bounds, initial_bounds);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_update_config_last_opened_notes_path_persists_to_disk() {
        let temp_dir = std::env::temp_dir().join("poc_overlay_test_notes_path");
        let _ = fs::remove_dir_all(&temp_dir);

        let test_path = "C:\\path\\to\\my_notes.md".to_string();
        let updated =
            update_config_last_opened_notes_path(&temp_dir, Some(test_path.clone())).unwrap();
        assert_eq!(updated.last_opened_notes_path, Some(test_path.clone()));

        let file_path = get_config_path(&temp_dir);
        let raw = fs::read_to_string(&file_path).unwrap();
        let from_disk: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(from_disk.last_opened_notes_path, Some(test_path));

        // Test updating to None
        let updated_none = update_config_last_opened_notes_path(&temp_dir, None).unwrap();
        assert_eq!(updated_none.last_opened_notes_path, None);
        let raw2 = fs::read_to_string(&file_path).unwrap();
        let from_disk2: Config = serde_json::from_str(&raw2).unwrap();
        assert_eq!(from_disk2.last_opened_notes_path, None);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_update_config_obsidian_vault_path_persists_to_disk() {
        let temp_dir = std::env::temp_dir().join("poc_overlay_test_vault_path");
        let _ = fs::remove_dir_all(&temp_dir);

        let test_vault = "D:\\project\\personal-vault".to_string();
        let updated =
            update_config_obsidian_vault_path(&temp_dir, Some(test_vault.clone())).unwrap();
        assert_eq!(updated.obsidian_vault_path, Some(test_vault.clone()));

        let file_path = get_config_path(&temp_dir);
        let raw = fs::read_to_string(&file_path).unwrap();
        let from_disk: Config = serde_json::from_str(&raw).unwrap();
        assert_eq!(from_disk.obsidian_vault_path, Some(test_vault));

        // Test updating to None
        let updated_none = update_config_obsidian_vault_path(&temp_dir, None).unwrap();
        assert_eq!(updated_none.obsidian_vault_path, None);
        let raw2 = fs::read_to_string(&file_path).unwrap();
        let from_disk2: Config = serde_json::from_str(&raw2).unwrap();
        assert_eq!(from_disk2.obsidian_vault_path, None);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_config_obsidian_vault_path_missing_or_null_defaults_to_none() {
        let json_missing = r#"{
            "openrouterApiKeys": ["sk-or-test"],
            "groqApiKeys": ["gsk-test"]
        }"#;
        let parsed_missing: Config = serde_json::from_str(json_missing).unwrap();
        assert_eq!(parsed_missing.obsidian_vault_path, None);

        let json_null = r#"{
            "openrouterApiKeys": ["sk-or-test"],
            "groqApiKeys": ["gsk-test"],
            "obsidianVaultPath": null
        }"#;
        let parsed_null: Config = serde_json::from_str(json_null).unwrap();
        assert_eq!(parsed_null.obsidian_vault_path, None);

        let json_with_val = r#"{
            "openrouterApiKeys": ["sk-or-test"],
            "groqApiKeys": ["gsk-test"],
            "obsidianVaultPath": "D:\\project\\personal-vault"
        }"#;
        let parsed_val: Config = serde_json::from_str(json_with_val).unwrap();
        assert_eq!(
            parsed_val.obsidian_vault_path,
            Some("D:\\project\\personal-vault".to_string())
        );
    }
}
