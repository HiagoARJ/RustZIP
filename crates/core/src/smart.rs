//! Compressão "inteligente": não gastar CPU recompactando o que já está compactado (fotos,
//! vídeos, outros arquivos compactados) e passar executáveis pelo filtro BCJ x86, que deixa os
//! endereços de chamada relativos e melhora a compressão de .exe/.dll em ~10%.

use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Kind {
    #[default]
    Normal,
    Executable,
    Incompressible,
}

const INCOMPRESSIBLE: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "avif", "heic", "heif", "jxl", "mp3", "aac", "m4a", "ogg",
    "opus", "flac", "wma", "mp4", "m4v", "mkv", "webm", "mov", "wmv", "flv", "3gp", "zip", "7z",
    "rar", "gz", "tgz", "xz", "txz", "zst", "bz2", "lz4", "br", "lzma", "cab", "jar", "apk", "aab",
    "ipa", "docx", "xlsx", "pptx", "odt", "ods", "odp", "epub", "cbz", "cbr", "woff", "woff2",
    "dmg", "msix", "appx", "nupkg", "vsix", "whl", "crx", "xpi", "squashfs",
];

const EXECUTABLE: &[&str] = &["exe", "dll", "sys", "ocx", "scr", "cpl", "efi", "drv", "pyd", "node", "mui"];

fn ext(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

pub fn classify(path: &Path, size: u64) -> Kind {
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    let e = ext(&name);
    if INCOMPRESSIBLE.contains(&e.as_str()) {
        return Kind::Incompressible;
    }
    if EXECUTABLE.contains(&e.as_str()) {
        return Kind::Executable;
    }
    // Extensão desconhecida e arquivo grande: amostra o começo.
    if size >= 256 * 1024 && sample_incompressible(path) {
        return Kind::Incompressible;
    }
    Kind::Normal
}

/// Compacta 128 KiB do meio do arquivo com LZ4 (GB/s): se quase não diminui, é aleatório ou já
/// compactado.
fn sample_incompressible(path: &Path) -> bool {
    let Ok(mut f) = File::open(path) else { return false };
    let mut buf = vec![0u8; 128 * 1024];
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(_) => return false,
        }
    }
    buf.truncate(n);
    if buf.len() < 4096 {
        return false;
    }
    let packed = lz4_flex::block::compress(&buf);
    packed.len() as f64 > buf.len() as f64 * 0.97
}

/// Chave de ordenação que agrupa arquivos parecidos (mesma extensão, depois mesmo nome), como o
/// 7-Zip faz: num bloco sólido, vizinhos parecidos comprimem bem melhor.
pub fn sort_key(name: &str) -> (String, String) {
    let base = name.rsplit('/').next().unwrap_or(name);
    (ext(base), base.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_extension() {
        assert_eq!(classify(Path::new("a/FOTO.JPG"), 10), Kind::Incompressible);
        assert_eq!(classify(Path::new("setup.exe"), 10), Kind::Executable);
        assert_eq!(classify(Path::new("notes.txt"), 10), Kind::Normal);
    }

    #[test]
    fn random_data_is_incompressible() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("blob.bin");
        let mut x: u64 = 0x9E3779B97F4A7C15;
        let data: Vec<u8> = (0..300_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect();
        std::fs::write(&p, &data).unwrap();
        assert_eq!(classify(&p, data.len() as u64), Kind::Incompressible);
        let q = dir.path().join("text.bin");
        std::fs::write(&q, "lorem ipsum dolor sit amet ".repeat(20_000)).unwrap();
        assert_eq!(classify(&q, 540_000), Kind::Normal);
    }
}
