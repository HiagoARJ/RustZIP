//! Preferências em `%APPDATA%\RustZIP\settings.json`.

use std::path::PathBuf;

use rustzip_core::{Format, ZipMethod};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Formato de "Adicionar para <nome>.<ext>" no menu do Explorer.
    pub default_format: Format,
    pub default_level: u32,
    pub zip_method: ZipMethod,
    pub solid: bool,
    pub smart: bool,
    /// "Extrair aqui" cria uma pasta quando o arquivo tem vários itens soltos na raiz.
    pub smart_extract_here: bool,
    pub open_folder_after_extract: bool,
    pub theme: String,
    pub last_folder: Option<String>,
    /// Itens do submenu do Explorer (o DLL lê este arquivo).
    pub menu_add_dialog: bool,
    pub menu_add_quick: bool,
    pub menu_email: bool,
    pub menu_extract: bool,
    pub menu_test: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_format: Format::SevenZ,
            default_level: 5,
            zip_method: ZipMethod::Deflate,
            solid: true,
            smart: true,
            smart_extract_here: true,
            open_folder_after_extract: false,
            theme: "padrao".into(),
            last_folder: None,
            menu_add_dialog: true,
            menu_add_quick: true,
            menu_email: true,
            menu_extract: true,
            menu_test: true,
        }
    }
}

pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("RustZIP")
}

fn path() -> PathBuf {
    dir().join("settings.json")
}

pub fn load() -> Settings {
    std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(s: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(dir())?;
    let tmp = path().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    std::fs::rename(tmp, path())
}
