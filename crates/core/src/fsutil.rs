use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// O que fazer quando o arquivo extraído já existe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Overwrite {
    #[default]
    Overwrite,
    Skip,
    /// Grava como "nome (2).ext".
    Rename,
    /// Só substitui se o do arquivo compactado for mais novo.
    Newer,
}

/// Junta `dest` com um nome vindo do arquivo compactado sem deixar escapar para fora de `dest`
/// (`..`, raiz, unidade) e trocando caracteres que o Windows não aceita.
pub fn safe_join(dest: &Path, name: &str) -> Result<PathBuf> {
    let mut out = dest.to_path_buf();
    let mut pushed = false;
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return Err(Error::Corrupt(format!("caminho inseguro no arquivo: {name}"))),
            p => {
                let clean = sanitize_component(p);
                if Path::new(&clean).components().any(|c| !matches!(c, Component::Normal(_))) {
                    return Err(Error::Corrupt(format!("caminho inseguro no arquivo: {name}")));
                }
                out.push(clean);
                pushed = true;
            }
        }
    }
    if !pushed {
        return Err(Error::Corrupt(format!("nome vazio no arquivo: {name:?}")));
    }
    Ok(out)
}

fn sanitize_component(p: &str) -> String {
    let mut s: String = p
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c if (c as u32) < 32 => '_',
            c => c,
        })
        .collect();
    // O Windows ignora ponto e espaço no fim; "a." e "a" seriam o mesmo arquivo.
    while s.ends_with(['.', ' ']) && s.len() > 1 {
        s.pop();
        s.push('_');
    }
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        s.insert(0, '_');
    }
    s
}

/// Caminho de saída já resolvido conforme a política; `None` = pular.
pub fn resolve_target(path: PathBuf, policy: Overwrite, entry_mtime: Option<i64>) -> Option<PathBuf> {
    let Ok(meta) = std::fs::symlink_metadata(&path) else { return Some(path) };
    match policy {
        Overwrite::Overwrite => Some(path),
        Overwrite::Skip => None,
        Overwrite::Newer => {
            let existing = meta.modified().ok().map(unix_secs);
            match (entry_mtime, existing) {
                (Some(a), Some(b)) if a <= b => None,
                _ => Some(path),
            }
        }
        Overwrite::Rename => Some(unique_name(&path)),
    }
}

pub fn unique_name(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or(Path::new(""));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, ext) = split_archive_name(&name);
    for i in 2.. {
        let candidate = parent.join(format!("{stem} ({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// "fotos.tar.gz" -> ("fotos", ".tar.gz"); "a.txt" -> ("a", ".txt").
pub fn split_archive_name(name: &str) -> (&str, &str) {
    let lower = name.to_ascii_lowercase();
    for double in [".tar.gz", ".tar.xz", ".tar.zst", ".tar.bz2", ".tar.lz4", ".tar.br"] {
        if lower.ends_with(double) {
            let at = name.len() - double.len();
            return (&name[..at], &name[at..]);
        }
    }
    match name.rfind('.') {
        Some(0) | None => (name, ""),
        Some(i) => (&name[..i], &name[i..]),
    }
}

pub fn unix_secs(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

pub fn from_unix(secs: i64) -> SystemTime {
    if secs >= 0 {
        UNIX_EPOCH + std::time::Duration::from_secs(secs as u64)
    } else {
        UNIX_EPOCH - std::time::Duration::from_secs(secs.unsigned_abs())
    }
}

fn local_offset() -> time::UtcOffset {
    static OFFSET: OnceLock<time::UtcOffset> = OnceLock::new();
    *OFFSET.get_or_init(|| time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC))
}

/// Hora DOS (ZIP/RAR, em hora local) -> Unix.
pub fn dos_to_unix(date: u16, time_: u16) -> Option<i64> {
    let year = 1980 + (date >> 9) as i32;
    let month = time::Month::try_from(((date >> 5) & 0xf) as u8).ok()?;
    let day = (date & 0x1f) as u8;
    let d = time::Date::from_calendar_date(year, month, day).ok()?;
    let t = time::Time::from_hms(
        (time_ >> 11) as u8,
        ((time_ >> 5) & 0x3f) as u8,
        ((time_ & 0x1f) * 2).min(59) as u8,
    )
    .ok()?;
    Some(time::PrimitiveDateTime::new(d, t).assume_offset(local_offset()).unix_timestamp())
}

/// Unix -> data/hora local "civil" (para os campos de data DOS do ZIP).
pub fn unix_to_local(secs: i64) -> Option<time::PrimitiveDateTime> {
    let dt = time::OffsetDateTime::from_unix_timestamp(secs).ok()?.to_offset(local_offset());
    Some(time::PrimitiveDateTime::new(dt.date(), dt.time()))
}

pub fn set_mtime(path: &Path, secs: Option<i64>) {
    if let Some(s) = secs {
        if let Ok(f) = std::fs::File::options().write(true).open(path) {
            let _ = f.set_times(std::fs::FileTimes::new().set_modified(from_unix(s)));
        }
    }
}

/// Normaliza um nome de entrada: barras normais, sem "./" nem barra final.
pub fn normalize_entry(name: &str) -> String {
    let s = name.replace('\\', "/");
    let s = s.trim_start_matches("./").trim_start_matches('/');
    s.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_traversal() {
        let d = Path::new("C:/out");
        assert!(safe_join(d, "../x").is_err());
        assert!(safe_join(d, "a/../../x").is_err());
        assert_eq!(safe_join(d, "/abs/x.txt").unwrap(), d.join("abs").join("x.txt"));
        assert_eq!(safe_join(d, "C:/x").unwrap(), d.join("C_").join("x"));
        assert_eq!(safe_join(d, "a\\b?.txt").unwrap(), d.join("a").join("b_.txt"));
        assert_eq!(safe_join(d, "con.txt").unwrap(), d.join("_con.txt"));
    }

    #[test]
    fn names() {
        assert_eq!(split_archive_name("a.tar.gz"), ("a", ".tar.gz"));
        assert_eq!(split_archive_name(".bashrc"), (".bashrc", ""));
        assert_eq!(normalize_entry("./a/b/"), "a/b");
    }

    #[test]
    fn dos_time() {
        // 2024-03-15 10:20:30 local
        let date = ((2024 - 1980) << 9) | (3 << 5) | 15;
        let tm = (10 << 11) | (20 << 5) | 15;
        let secs = dos_to_unix(date, tm).unwrap();
        let back = unix_to_local(secs).unwrap();
        assert_eq!((back.year(), back.hour(), back.second()), (2024, 10, 30));
    }
}
