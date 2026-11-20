use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::source::Source;

/// Formatos que o app lê. Todos menos RAR também são gravados (o algoritmo de compressão do RAR
/// é proprietário; o código do unrar só pode ser usado para extrair).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "7z")]
    SevenZ,
    #[serde(rename = "rar")]
    Rar,
    #[serde(rename = "tar")]
    Tar,
    #[serde(rename = "tar.gz")]
    TarGz,
    #[serde(rename = "tar.xz")]
    TarXz,
    #[serde(rename = "tar.zst")]
    TarZst,
    #[serde(rename = "tar.bz2")]
    TarBz2,
    #[serde(rename = "tar.lz4")]
    TarLz4,
    #[serde(rename = "tar.br")]
    TarBr,
    #[serde(rename = "gz")]
    Gz,
    #[serde(rename = "xz")]
    Xz,
    #[serde(rename = "zst")]
    Zst,
    #[serde(rename = "bz2")]
    Bz2,
    #[serde(rename = "lz4")]
    Lz4,
    #[serde(rename = "br")]
    Br,
}

/// Compressor de fluxo (sozinho ou por baixo de um TAR).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    None,
    Gz,
    Xz,
    Zst,
    Bz2,
    Lz4,
    Br,
}

impl Format {
    pub const ALL: [Format; 16] = [
        Format::Zip,
        Format::SevenZ,
        Format::Rar,
        Format::Tar,
        Format::TarGz,
        Format::TarXz,
        Format::TarZst,
        Format::TarBz2,
        Format::TarLz4,
        Format::TarBr,
        Format::Gz,
        Format::Xz,
        Format::Zst,
        Format::Bz2,
        Format::Lz4,
        Format::Br,
    ];

    pub fn ext(self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::SevenZ => "7z",
            Format::Rar => "rar",
            Format::Tar => "tar",
            Format::TarGz => "tar.gz",
            Format::TarXz => "tar.xz",
            Format::TarZst => "tar.zst",
            Format::TarBz2 => "tar.bz2",
            Format::TarLz4 => "tar.lz4",
            Format::TarBr => "tar.br",
            Format::Gz => "gz",
            Format::Xz => "xz",
            Format::Zst => "zst",
            Format::Bz2 => "bz2",
            Format::Lz4 => "lz4",
            Format::Br => "br",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Zip => "ZIP",
            Format::SevenZ => "7Z",
            Format::Rar => "RAR",
            Format::Tar => "TAR",
            Format::TarGz => "TAR.GZ",
            Format::TarXz => "TAR.XZ",
            Format::TarZst => "TAR.ZST",
            Format::TarBz2 => "TAR.BZ2",
            Format::TarLz4 => "TAR.LZ4",
            Format::TarBr => "TAR.BR",
            Format::Gz => "GZIP",
            Format::Xz => "XZ",
            Format::Zst => "ZSTD",
            Format::Bz2 => "BZIP2",
            Format::Lz4 => "LZ4",
            Format::Br => "BROTLI",
        }
    }

    pub fn writable(self) -> bool {
        self != Format::Rar
    }

    pub fn is_tar(self) -> bool {
        matches!(
            self,
            Format::Tar
                | Format::TarGz
                | Format::TarXz
                | Format::TarZst
                | Format::TarBz2
                | Format::TarLz4
                | Format::TarBr
        )
    }

    /// Compressor de um arquivo só, sem lista de entradas (`foto.png.zst`).
    pub fn is_single_stream(self) -> bool {
        matches!(
            self,
            Format::Gz | Format::Xz | Format::Zst | Format::Bz2 | Format::Lz4 | Format::Br
        )
    }

    pub fn stream(self) -> Stream {
        match self {
            Format::TarGz | Format::Gz => Stream::Gz,
            Format::TarXz | Format::Xz => Stream::Xz,
            Format::TarZst | Format::Zst => Stream::Zst,
            Format::TarBz2 | Format::Bz2 => Stream::Bz2,
            Format::TarLz4 | Format::Lz4 => Stream::Lz4,
            Format::TarBr | Format::Br => Stream::Br,
            _ => Stream::None,
        }
    }

    fn with_tar(stream: Stream) -> Format {
        match stream {
            Stream::None => Format::Tar,
            Stream::Gz => Format::TarGz,
            Stream::Xz => Format::TarXz,
            Stream::Zst => Format::TarZst,
            Stream::Bz2 => Format::TarBz2,
            Stream::Lz4 => Format::TarLz4,
            Stream::Br => Format::TarBr,
        }
    }

    fn single(stream: Stream) -> Format {
        match stream {
            Stream::None => Format::Tar,
            Stream::Gz => Format::Gz,
            Stream::Xz => Format::Xz,
            Stream::Zst => Format::Zst,
            Stream::Bz2 => Format::Bz2,
            Stream::Lz4 => Format::Lz4,
            Stream::Br => Format::Br,
        }
    }

    /// Formato pelo nome do arquivo (sem olhar o conteúdo).
    pub fn from_name(name: &str) -> Option<Format> {
        let lower = volume_base(&name.to_ascii_lowercase()).to_string();
        let ends = |s: &str| lower.ends_with(s);
        Some(if ends(".tar.gz") || ends(".tgz") {
            Format::TarGz
        } else if ends(".tar.xz") || ends(".txz") {
            Format::TarXz
        } else if ends(".tar.zst") || ends(".tzst") || ends(".tar.zstd") {
            Format::TarZst
        } else if ends(".tar.bz2") || ends(".tbz2") || ends(".tbz") {
            Format::TarBz2
        } else if ends(".tar.lz4") {
            Format::TarLz4
        } else if ends(".tar.br") {
            Format::TarBr
        } else if ends(".tar") {
            Format::Tar
        } else if ends(".7z") {
            Format::SevenZ
        } else if ends(".rar") || ends(".cbr") || is_old_rar_volume(&lower) {
            Format::Rar
        } else if [
            ".zip", ".cbz", ".jar", ".apk", ".xpi", ".epub", ".docx", ".xlsx", ".pptx", ".odt",
            ".ods", ".odp", ".nupkg", ".vsix", ".whl", ".ipa", ".appx", ".msix",
        ]
        .iter()
        .any(|e| ends(e))
        {
            Format::Zip
        } else if ends(".gz") {
            Format::Gz
        } else if ends(".xz") {
            Format::Xz
        } else if ends(".zst") || ends(".zstd") {
            Format::Zst
        } else if ends(".bz2") {
            Format::Bz2
        } else if ends(".lz4") {
            Format::Lz4
        } else if ends(".br") {
            Format::Br
        } else {
            return None;
        })
    }

    /// Formato pelo conteúdo (assinatura), com o nome como desempate. Para compressores de
    /// fluxo, abre o começo do fluxo para ver se há um TAR dentro.
    pub fn detect(path: &Path) -> Result<Format> {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let by_name = Format::from_name(&name);
        let mut src = Source::open(path)?;
        let mut head = [0u8; 600];
        let n = read_up_to(&mut src, &mut head)?;
        let head = &head[..n];

        if head.starts_with(b"Rar!\x1a\x07") {
            return Ok(Format::Rar);
        }
        if head.starts_with(b"7z\xbc\xaf\x27\x1c") {
            return Ok(Format::SevenZ);
        }
        if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") || head.starts_with(b"PK\x07\x08") {
            return Ok(Format::Zip);
        }
        let stream = if head.starts_with(&[0x1f, 0x8b]) {
            Stream::Gz
        } else if head.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0]) {
            Stream::Xz
        } else if head.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
            Stream::Zst
        } else if head.starts_with(b"BZh") {
            Stream::Bz2
        } else if head.starts_with(&[0x04, 0x22, 0x4d, 0x18]) {
            Stream::Lz4
        } else if head.len() >= 262 && &head[257..262] == b"ustar" {
            return Ok(Format::Tar);
        } else if matches!(by_name, Some(Format::Br | Format::TarBr)) {
            Stream::Br
        } else if head.starts_with(b"MZ") {
            // Autoextraível: ZIP e RAR leem com dados antes do arquivo.
            return match by_name {
                Some(f) => Ok(f),
                None => sfx_kind(path),
            };
        } else {
            return by_name.ok_or_else(|| Error::Unsupported(format!("formato de \"{name}\" não reconhecido")));
        };

        // Compressor de fluxo: há um TAR por dentro?
        let src = Source::open(path)?;
        let mut dec = crate::streams::decoder(stream, Box::new(src))?;
        let mut block = [0u8; 512];
        let n = read_up_to(&mut dec, &mut block).unwrap_or(0);
        let is_tar = n >= 262 && &block[257..262] == b"ustar";
        Ok(if is_tar { Format::with_tar(stream) } else { Format::single(stream) })
    }
}

/// Procura a assinatura de um arquivo depois do executável de um autoextraível.
fn sfx_kind(path: &Path) -> Result<Format> {
    let data = {
        let mut f = std::fs::File::open(path)?;
        let mut v = Vec::new();
        f.by_ref().take(16 << 20).read_to_end(&mut v)?;
        v
    };
    let find = |sig: &[u8]| data.windows(sig.len()).any(|w| w == sig);
    if find(b"Rar!\x1a\x07") {
        Ok(Format::Rar)
    } else if find(b"PK\x03\x04") {
        Ok(Format::Zip)
    } else {
        Err(Error::Unsupported("executável sem arquivo compactado".into()))
    }
}

/// `x.7z.001` -> `x.7z`.
pub fn volume_base(name: &str) -> &str {
    if let Some((base, num)) = name.rsplit_once('.')
        && num.len() == 3
        && num.bytes().all(|b| b.is_ascii_digit())
    {
        return base;
    }
    name
}

fn is_old_rar_volume(lower: &str) -> bool {
    // Volumes no esquema antigo: .r00, .r01...
    lower.rsplit_once('.').is_some_and(|(_, e)| {
        e.len() == 3 && e.starts_with('r') && e[1..].bytes().all(|b| b.is_ascii_digit())
    })
}

pub(crate) fn read_up_to(r: &mut dyn Read, buf: &mut [u8]) -> Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(Format::from_name("a.tar.zst"), Some(Format::TarZst));
        assert_eq!(Format::from_name("a.7z.001"), Some(Format::SevenZ));
        assert_eq!(Format::from_name("Foto.PNG.gz"), Some(Format::Gz));
        assert_eq!(Format::from_name("x.part2.rar"), Some(Format::Rar));
        assert_eq!(Format::from_name("x.r01"), Some(Format::Rar));
        assert_eq!(Format::from_name("doc.docx"), Some(Format::Zip));
        assert_eq!(Format::from_name("x.txt"), None);
    }
}
