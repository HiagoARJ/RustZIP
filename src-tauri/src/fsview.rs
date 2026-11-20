//! Navegação no disco para o gerenciador: pastas, unidades, nomes de tipo e ícones do Windows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
    pub hidden: bool,
}

pub fn list_dir(path: &Path) -> std::io::Result<Vec<FsEntry>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(path)? {
        let Ok(e) = e else { continue };
        let Ok(meta) = e.metadata() else { continue };
        #[cfg(windows)]
        let hidden = {
            use std::os::windows::fs::MetadataExt;
            meta.file_attributes() & 0x2 != 0
        };
        #[cfg(not(windows))]
        let hidden = e.file_name().to_string_lossy().starts_with('.');
        out.push(FsEntry {
            name: e.file_name().to_string_lossy().into_owned(),
            path: e.path().to_string_lossy().into_owned(),
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: meta.modified().ok().map(rustzip_core::fsutil::unix_secs),
            hidden,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub label: String,
    pub path: String,
    pub kind: String,
    pub free: Option<u64>,
    pub total: Option<u64>,
}

/// Pastas conhecidas e unidades.
pub fn places() -> Vec<Place> {
    let mut v = Vec::new();
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    for (label, sub, kind) in [
        ("Área de Trabalho", "Desktop", "desktop"),
        ("Documentos", "Documents", "documents"),
        ("Downloads", "Downloads", "downloads"),
        ("Imagens", "Pictures", "pictures"),
    ] {
        let p = Path::new(&home).join(sub);
        if p.is_dir() {
            v.push(Place { label: label.into(), path: p.to_string_lossy().into_owned(), kind: kind.into(), free: None, total: None });
        }
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
        use windows::core::PCWSTR;
        let mask = GetLogicalDrives();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let root = format!("{}:\\", (b'A' + i as u8) as char);
            let w: Vec<u16> = root.encode_utf16().chain([0]).collect();
            let kind = match GetDriveTypeW(PCWSTR(w.as_ptr())) {
                2 => "removable",
                4 => "network",
                5 => "cdrom",
                _ => "drive",
            };
            let mut name = [0u16; 128];
            let _ = GetVolumeInformationW(PCWSTR(w.as_ptr()), Some(&mut name), None, None, None, None);
            let vol = String::from_utf16_lossy(&name[..name.iter().position(|&c| c == 0).unwrap_or(0)]);
            let (mut free, mut total) = (0u64, 0u64);
            let space = GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), None, Some(&mut total), Some(&mut free)).is_ok();
            let label = if vol.is_empty() { format!("Disco Local ({})", &root[..2]) } else { format!("{vol} ({})", &root[..2]) };
            v.push(Place { label, path: root, kind: kind.into(), free: space.then_some(free), total: space.then_some(total) });
        }
    }
    v
}

/// Ícone pequeno (PNG em data URL) e nome do tipo ("Arquivo JPG") como o Explorer mostra,
/// por extensão. Cache por processo.
#[derive(Debug, Clone, Serialize)]
pub struct TypeInfo {
    pub icon: Option<String>,
    pub name: String,
}

static CACHE: Mutex<Option<HashMap<String, TypeInfo>>> = Mutex::new(None);

pub fn type_infos(keys: &[String]) -> HashMap<String, TypeInfo> {
    let mut out = HashMap::new();
    for k in keys {
        if let Some(hit) = CACHE.lock().unwrap().get_or_insert_default().get(k) {
            out.insert(k.clone(), hit.clone());
            continue;
        }
        let info = query_type(k);
        CACHE.lock().unwrap().get_or_insert_default().insert(k.clone(), info.clone());
        out.insert(k.clone(), info);
    }
    out
}

/// `k` é ".ext", "<folder>" ou um caminho absoluto (para .exe/.lnk, que têm ícone próprio).
#[cfg(windows)]
fn query_type(k: &str) -> TypeInfo {
    use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
    use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_TYPENAME, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW};
    use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;
    use windows::core::PCWSTR;
    let (name, attr, use_attr) = if k == "<folder>" {
        ("pasta".to_string(), FILE_ATTRIBUTE_DIRECTORY, true)
    } else if Path::new(k).is_absolute() {
        (k.to_string(), FILE_ATTRIBUTE_NORMAL, false)
    } else {
        (format!("arquivo{k}"), FILE_ATTRIBUTE_NORMAL, true)
    };
    let w: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let mut info = SHFILEINFOW::default();
    let mut flags = SHGFI_ICON | SHGFI_SMALLICON | SHGFI_TYPENAME;
    if use_attr {
        flags |= SHGFI_USEFILEATTRIBUTES;
    }
    let ok = unsafe { SHGetFileInfoW(PCWSTR(w.as_ptr()), attr, Some(&mut info), size_of::<SHFILEINFOW>() as u32, flags) };
    let type_name = String::from_utf16_lossy(&info.szTypeName[..info.szTypeName.iter().position(|&c| c == 0).unwrap_or(0)]);
    let mut icon = None;
    if ok != 0 && !info.hIcon.is_invalid() {
        icon = icon_to_png(info.hIcon).map(|png| format!("data:image/png;base64,{}", b64(&png)));
        unsafe {
            let _ = DestroyIcon(info.hIcon);
        }
    }
    TypeInfo { icon, name: type_name }
}

#[cfg(not(windows))]
fn query_type(k: &str) -> TypeInfo {
    TypeInfo { icon: None, name: k.trim_start_matches('.').to_uppercase() }
}

fn b64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// HICON -> PNG RGBA (desenha num DIB de 32 bits; ícones antigos sem alfa usam a máscara).
#[cfg(windows)]
fn icon_to_png(icon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<Vec<u8>> {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DrawIconEx, GetSystemMetrics, SM_CXSMICON};
    unsafe {
        let size = GetSystemMetrics(SM_CXSMICON).max(16);
        let draw = |bg: u32| -> Option<Vec<u8>> {
            let dc = CreateCompatibleDC(None);
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = size;
            bmi.bmiHeader.biHeight = -size;
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            let old = SelectObject(dc, bmp.into());
            let px = std::slice::from_raw_parts_mut(bits as *mut u32, (size * size) as usize);
            px.fill(bg);
            let _ = DrawIconEx(dc, 0, 0, icon, size, size, 0, None, DI_NORMAL);
            let out: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
            SelectObject(dc, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
            Some(out)
        };
        // Desenha sobre preto e sobre branco: a diferença dá o alfa de qualquer ícone.
        let black = draw(0xFF00_0000)?;
        let white = draw(0xFFFF_FFFF)?;
        let mut rgba = Vec::with_capacity(black.len());
        for (b, w) in black.chunks(4).zip(white.chunks(4)) {
            let a = 255 - (w[0] as i32 - b[0] as i32).clamp(0, 255);
            let un = |c: u8| if a == 0 { 0 } else { ((c as i32 * 255) / a).min(255) as u8 };
            rgba.extend_from_slice(&[un(b[2]), un(b[1]), un(b[0]), a as u8]);
        }
        let mut png_data = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_data, size as u32, size as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().ok()?;
            w.write_image_data(&rgba).ok()?;
        }
        Some(png_data)
    }
}

/// Pasta inicial: a última usada, senão Documentos.
pub fn home() -> PathBuf {
    if let Some(p) = crate::settings::load().last_folder.map(PathBuf::from).filter(|p| p.is_dir()) {
        return p;
    }
    let home = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into()));
    let docs = home.join("Documents");
    if docs.is_dir() { docs } else { home }
}
