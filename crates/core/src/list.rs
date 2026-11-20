use std::path::Path;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::format::Format;
use crate::fsutil::{dos_to_unix, normalize_entry, unix_secs};
use crate::source::Source;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Caminho com "/" e sem barra final.
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub packed: Option<u64>,
    /// Segundos Unix.
    pub modified: Option<i64>,
    pub crc: Option<u32>,
    pub encrypted: bool,
    pub method: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    pub path: String,
    pub format: Format,
    pub entries: Vec<Entry>,
    pub solid: bool,
    pub encrypted_names: bool,
    pub comment: Option<String>,
    pub size_on_disk: u64,
    pub volumes: u32,
    pub has_recovery: bool,
    pub locked: bool,
    /// Número de blocos 7z (mostra quanto a extração pode paralelizar).
    pub blocks: Option<u32>,
}

pub fn list(path: &Path, password: Option<&str>) -> Result<ArchiveInfo> {
    let format = Format::detect(path)?;
    let src = Source::open(path)?;
    let mut info = ArchiveInfo {
        path: path.to_string_lossy().into_owned(),
        format,
        entries: Vec::new(),
        solid: false,
        encrypted_names: false,
        comment: None,
        size_on_disk: src.len(),
        volumes: src.volumes() as u32,
        has_recovery: false,
        locked: false,
        blocks: None,
    };
    match format {
        Format::Zip => list_zip(src, &mut info)?,
        Format::SevenZ => list_7z(src, password, &mut info)?,
        Format::Rar => list_rar(path, password, &mut info)?,
        f if f.is_tar() => list_tar(src, f, &mut info)?,
        f => list_single(path, src, f, &mut info)?,
    }
    Ok(info)
}

fn list_zip(src: Source, info: &mut ArchiveInfo) -> Result<()> {
    let mut zip = zip::ZipArchive::new(src)?;
    let comment = String::from_utf8_lossy(zip.comment()).trim().to_string();
    if !comment.is_empty() {
        info.comment = Some(comment);
    }
    info.entries.reserve(zip.len());
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i)?;
        let modified = f.last_modified().and_then(|d| dos_to_unix(d.datepart(), d.timepart()));
        info.entries.push(Entry {
            path: normalize_entry(&f.name()?),
            is_dir: f.is_dir(),
            size: f.size(),
            packed: Some(f.compressed_size()),
            modified,
            crc: Some(f.crc32()),
            encrypted: f.encrypted(),
            method: Some(zip_method_name(f.compression())),
        });
    }
    Ok(())
}

pub(crate) fn zip_method_name(m: zip::CompressionMethod) -> String {
    use zip::CompressionMethod as M;
    match m {
        M::Stored => "Store".into(),
        M::Deflated => "Deflate".into(),
        M::Deflate64 => "Deflate64".into(),
        M::Bzip2 => "BZip2".into(),
        M::Zstd => "Zstandard".into(),
        M::Lzma => "LZMA".into(),
        M::Xz => "XZ".into(),
        M::Ppmd => "PPMd".into(),
        M::Aes => "AES".into(),
        other => format!("{other:?}"),
    }
}

fn list_7z(mut src: Source, password: Option<&str>, info: &mut ArchiveInfo) -> Result<()> {
    let pw = sevenz_rust2::Password::new(password.unwrap_or(""));
    let archive = sevenz_rust2::Archive::read(&mut src, &pw)?;
    info.solid = archive.is_solid;
    info.blocks = Some(archive.blocks.len() as u32);
    let block_methods: Vec<String> = archive
        .blocks
        .iter()
        .map(|b| {
            b.ordered_coder_iter()
                .filter_map(|(_, c)| sevenz_rust2::EncoderMethod::by_id(c.encoder_method_id()).map(|m| m.name().to_string()))
                .collect::<Vec<_>>()
                .join(" + ")
        })
        .collect();
    let firsts = archive.stream_map.block_first_pack_stream_index();
    let packed_per_block: Vec<u64> = (0..archive.blocks.len())
        .map(|b| {
            let end = firsts.get(b + 1).copied().unwrap_or(archive.pack_sizes().len());
            archive.pack_sizes()[firsts[b]..end].iter().sum()
        })
        .collect();
    for (i, e) in archive.files.iter().enumerate() {
        let block = archive.stream_map.file_block_index[i];
        let encrypted = block.is_some() && archive.is_file_encrypted(i);
        // No 7z o tamanho compactado é do bloco; só faz sentido por arquivo quando o bloco tem um só.
        let packed = block.and_then(|b| {
            let first = archive.stream_map.block_first_file_index[b];
            let single = archive.stream_map.file_block_index.get(first + 1).copied().flatten() != Some(b);
            single.then(|| packed_per_block[b])
        });
        let modified = e.has_last_modified_date.then(|| unix_secs(e.last_modified_date().into()));
        info.entries.push(Entry {
            path: normalize_entry(e.name()),
            is_dir: e.is_directory(),
            size: e.size(),
            packed: if e.has_stream { packed } else { Some(0) },
            modified,
            crc: e.has_crc.then_some(e.crc as u32),
            encrypted,
            method: block.map(|b| block_methods[b].clone()),
        });
    }
    if password.is_some() && info.entries.iter().all(|e| !e.encrypted) {
        info.encrypted_names = true;
    }
    Ok(())
}

fn list_rar(path: &Path, password: Option<&str>, info: &mut ArchiveInfo) -> Result<()> {
    let first = unrar::Archive::new(path).first_part();
    let archive = match password {
        Some(p) => unrar::Archive::with_password(&first, p),
        None => unrar::Archive::new(&first),
    };
    let open = archive.open_for_listing()?;
    info.solid = open.is_solid();
    info.encrypted_names = open.has_encrypted_headers();
    info.has_recovery = open.has_recovery_record();
    info.locked = open.is_locked();
    if info.volumes <= 1 {
        let probe = unrar::Archive::new(&first);
        info.volumes = (2..1000).take_while(|&n| probe.nth_part(n).is_some_and(|p| p.exists())).count() as u32 + 1;
    }
    for entry in open {
        let e = entry?;
        if e.is_split() && info.entries.last().is_some_and(|l: &Entry| l.path == normalize_entry(&e.filename.to_string_lossy())) {
            continue;
        }
        let modified = dos_to_unix((e.file_time >> 16) as u16, (e.file_time & 0xffff) as u16);
        info.entries.push(Entry {
            path: normalize_entry(&e.filename.to_string_lossy()),
            is_dir: e.is_directory(),
            size: e.unpacked_size,
            packed: None,
            modified,
            crc: Some(e.file_crc),
            encrypted: e.is_encrypted(),
            method: Some(rar_method(e.method)),
        });
    }
    Ok(())
}

fn rar_method(m: u32) -> String {
    match m {
        0x30 => "Store",
        0x31 => "Fastest",
        0x32 => "Fast",
        0x33 => "Normal",
        0x34 => "Good",
        0x35 => "Best",
        _ => "RAR",
    }
    .to_string()
}

fn list_tar(src: Source, format: Format, info: &mut ArchiveInfo) -> Result<()> {
    info.solid = format != Format::Tar;
    let reader = crate::streams::decoder(format.stream(), Box::new(src))?;
    let mut tar = tar::Archive::new(reader);
    for entry in tar.entries()? {
        let e = entry?;
        let h = e.header();
        let kind = h.entry_type();
        if !(kind.is_file() || kind.is_dir() || kind.is_gnu_sparse() || kind.is_contiguous()) {
            continue;
        }
        info.entries.push(Entry {
            path: normalize_entry(&e.path()?.to_string_lossy()),
            is_dir: kind.is_dir(),
            size: e.size(),
            packed: None,
            modified: h.mtime().ok().map(|m| m as i64),
            crc: None,
            encrypted: false,
            method: Some(format.label().to_string()),
        });
    }
    Ok(())
}

/// gz/xz/zst... de um arquivo só: a "entrada" é o nome sem a extensão.
fn list_single(path: &Path, src: Source, format: Format, info: &mut ArchiveInfo) -> Result<()> {
    let packed = src.len();
    let mut name = single_stream_name(path);
    let mut modified = std::fs::metadata(path).and_then(|m| m.modified()).ok().map(unix_secs);
    let mut dec = crate::streams::decoder(format.stream(), Box::new(src.clone()))?;
    if format == Format::Gz {
        // O cabeçalho gzip pode guardar o nome e a data originais.
        let mut gz = flate2::read::GzDecoder::new(src);
        if let Some(h) = gz.header() {
            if let Some(n) = h.filename() {
                name = String::from_utf8_lossy(n).into_owned();
            }
            if h.mtime() > 0 {
                modified = Some(h.mtime() as i64);
            }
        }
        let _ = std::io::Read::read(&mut gz, &mut [0u8; 0]);
    }
    // Tamanho real só lendo o fluxo inteiro; é rápido (descompressão sem gravar).
    let size = std::io::copy(&mut dec, &mut std::io::sink()).map_err(|e| Error::Corrupt(e.to_string()))?;
    info.entries.push(Entry {
        path: normalize_entry(&name),
        is_dir: false,
        size,
        packed: Some(packed),
        modified,
        crc: None,
        encrypted: false,
        method: Some(format.label().to_string()),
    });
    Ok(())
}

pub(crate) fn single_stream_name(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let base = crate::format::volume_base(&name);
    match base.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => format!("{base}.out"),
    }
}
