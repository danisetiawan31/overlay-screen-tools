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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeysConfig {
    pub push_to_talk: String,
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
    pub openrouter_api_key: String,
    pub groq_api_key: String,
    pub window_bounds: WindowBounds,
    pub font_size: u32,
    pub last_opened_notes_path: Option<String>,
    pub hotkeys: HotkeysConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            openrouter_api_key: String::new(),
            groq_api_key: String::new(),
            window_bounds: WindowBounds::default(),
            font_size: DEFAULT_FONT_SIZE,
            last_opened_notes_path: None,
            hotkeys: HotkeysConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, tauri_specta::Event)]
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
/// - `openrouterApiKey` atau `groqApiKey` kosong
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
    let config: Config = serde_json::from_str(raw_json).map_err(|err| {
        format!(
            "Format JSON di config.json tidak valid / malformed: {}",
            err
        )
    })?;

    if config.openrouter_api_key.trim().is_empty() || config.groq_api_key.trim().is_empty() {
        return Err("openrouterApiKey atau groqApiKey di config.json masih kosong. Silakan isi kedua API key tersebut.".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_config_parsed_correctly() {
        let valid_json = r#"{
            "openrouterApiKey": "sk-or-v1-abc123test",
            "groqApiKey": "gsk_test456key",
            "windowBounds": { "x": 120, "y": 80, "width": 600, "height": 400 },
            "fontSize": 16,
            "lastOpenedNotesPath": "C:\\notes\\session.md",
            "hotkeys": { "pushToTalk": "F8", "toggleVisibility": "F9" }
        }"#;

        let result = parse_and_validate_config(valid_json);
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.openrouter_api_key, "sk-or-v1-abc123test");
        assert_eq!(config.groq_api_key, "gsk_test456key");
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
            "openrouterApiKey": "sk-or-123",
            "groqApiKey": "gsk-456",
            "fontSize": "bukan_angka",
        }"#;

        let result = parse_and_validate_config(malformed_json);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("tidak valid / malformed"));
    }

    #[test]
    fn test_empty_api_keys() {
        // Kasus 1: OpenRouter key kosong
        let empty_openrouter = r#"{
            "openrouterApiKey": "   ",
            "groqApiKey": "gsk_valid_key",
            "windowBounds": { "x": 0, "y": 0, "width": 400, "height": 600 },
            "fontSize": 14,
            "lastOpenedNotesPath": null,
            "hotkeys": { "pushToTalk": "F8", "toggleVisibility": "F9" }
        }"#;
        let res1 = parse_and_validate_config(empty_openrouter);
        assert!(res1.is_err());
        assert!(res1.unwrap_err().contains("masih kosong"));

        // Kasus 2: Groq key kosong
        let empty_groq = r#"{
            "openrouterApiKey": "sk-or-valid",
            "groqApiKey": "",
            "windowBounds": { "x": 0, "y": 0, "width": 400, "height": 600 },
            "fontSize": 14,
            "lastOpenedNotesPath": null,
            "hotkeys": { "pushToTalk": "F8", "toggleVisibility": "F9" }
        }"#;
        let res2 = parse_and_validate_config(empty_groq);
        assert!(res2.is_err());
        assert!(res2.unwrap_err().contains("masih kosong"));
    }
}
