//! Criação de arquivos compactados.
//!
//! Onde o RustZIP ganha do WinRAR/7-Zip em velocidade sem perder compactação:
//! - ZIP: cada entrada é compactada numa thread (o ZIP é independente por entrada), e o
//!   arquivo final só junta os blocos prontos, em ordem.
//! - 7z: os arquivos viram vários blocos sólidos compactados em paralelo; arquivos parecidos
//!   ficam juntos (ordem por extensão), executáveis passam pelo filtro BCJ x86, e o que já está
//!   compactado (fotos, vídeos, outros arquivos) vai num bloco COPY sem gastar CPU.
//! - TAR: zstd e xz multithread, com correspondências longas no zstd nos níveis altos.

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::SystemTime;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::format::Format;
use crate::fsutil::{unix_secs, unix_to_local};
use crate::progress::{CountingReader, PackedWriter, Progress};
use crate::smart::{Kind, classify, sort_key};
use crate::streams::{Tuning, encoder, zstd_level};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZipMethod {
    #[default]
    Deflate,
    /// Deflate com Zopfli: ~5% menor e 100% compatível, mas lento.
    Zopfli,
    Zstd,
    Xz,
    Bzip2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CreateOptions {
    pub format: Format,
    /// 0 = só armazenar, 1 = mais rápido ... 9 = melhor.
    pub level: u32,
    pub zip_method: ZipMethod,
    pub password: Option<String>,
    /// 7z: criptografar também a lista de nomes.
    pub encrypt_names: bool,
    /// 0 = todos os núcleos.
    pub threads: u32,
    pub solid: bool,
    /// Dicionário (7z/xz) ou janela (zstd) em MiB; 0 = padrão do nível.
    pub dictionary_mb: u32,
    /// Dividir em volumes deste tamanho (bytes); 0 = não dividir.
    pub volume_size: u64,
    /// Mandar os originais para a Lixeira depois de compactar (e testar, se pedido).
    pub delete_sources: bool,
    /// Não recompactar o que já é compactado; filtro BCJ para executáveis.
    pub smart: bool,
    pub comment: Option<String>,
    /// Padrões de nomes a ignorar (`*.tmp`, `node_modules`, `cache/*.bin`).
    pub exclude: Vec<String>,
    pub test_after: bool,
}

impl Default for CreateOptions {
    fn default() -> Self {
        Self {
            format: Format::SevenZ,
            level: 5,
            zip_method: ZipMethod::Deflate,
            password: None,
            encrypt_names: false,
            threads: 0,
            solid: true,
            dictionary_mb: 0,
            volume_size: 0,
            delete_sources: false,
            smart: true,
            comment: None,
            exclude: Vec::new(),
            test_after: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReport {
    pub outputs: Vec<String>,
    pub files: u64,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub elapsed_ms: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Item {
    pub abs: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: Option<SystemTime>,
    pub kind: Kind,
}

/// Junta os itens das origens: cada origem entra pelo próprio nome (como o WinRAR faz ao
/// compactar uma seleção do Explorer), pastas com todo o conteúdo.
pub(crate) fn collect(sources: &[PathBuf], exclude: &[String], skip: &[PathBuf], smart: bool) -> Result<Vec<Item>> {
    let skip: Vec<PathBuf> = skip.iter().filter_map(|p| fs::canonicalize(p).ok()).collect();
    let mut items = Vec::new();
    for src in sources {
        let meta = fs::metadata(src).map_err(|e| Error::other(format!("{}: {e}", src.display())))?;
        let base = src.parent().unwrap_or(Path::new(""));
        if meta.is_file() {
            let name = rel_name(base, src);
            items.push(item(src.clone(), name, false, meta.len(), meta.modified().ok(), smart));
            continue;
        }
        let walker = walkdir::WalkDir::new(src).follow_links(false).into_iter().filter_entry(|e| {
            let rel = rel_name(base, e.path());
            !excluded(&rel, exclude)
        });
        for entry in walker {
            let entry = entry.map_err(|e| Error::other(e.to_string()))?;
            let ft = entry.file_type();
            if !(ft.is_file() || ft.is_dir()) {
                continue;
            }
            // Só compara caminhos canônicos quando o nome bate (canonicalize abre o arquivo).
            if ft.is_file()
                && skip.iter().any(|s| s.file_name() == Some(entry.file_name()) && fs::canonicalize(entry.path()).is_ok_and(|c| &c == s))
            {
                continue;
            }
            let m = entry.metadata().map_err(|e| Error::other(e.to_string()))?;
            let name = rel_name(base, entry.path());
            items.push(item(entry.path().to_path_buf(), name, ft.is_dir(), if ft.is_file() { m.len() } else { 0 }, m.modified().ok(), smart));
        }
    }
    let excluded_files = |i: &Item| !i.is_dir && excluded(&i.name, exclude);
    items.retain(|i| !excluded_files(i));
    // Nomes repetidos (duas origens com o mesmo nome) quebrariam o arquivo: mantém o primeiro.
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.name.to_lowercase()));
    Ok(items)
}

fn item(abs: PathBuf, name: String, is_dir: bool, size: u64, mtime: Option<SystemTime>, smart: bool) -> Item {
    let kind = if is_dir || !smart { Kind::Normal } else { classify(&abs, size) };
    Item { abs, name, is_dir, size, mtime, kind }
}

fn rel_name(base: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(base).unwrap_or(path);
    rel.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Curinga simples (`*` e `?`). Sem "/" compara com o nome do arquivo; com "/", com o caminho.
pub fn excluded(rel: &str, patterns: &[String]) -> bool {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    patterns.iter().filter(|p| !p.trim().is_empty()).any(|p| {
        let p = p.trim().replace('\\', "/").to_lowercase();
        if p.contains('/') { wildcard(&p, &rel.to_lowercase()) } else { wildcard(&p, &file.to_lowercase()) }
    })
}

fn wildcard(p: &str, s: &str) -> bool {
    let (p, s): (Vec<char>, Vec<char>) = (p.chars().collect(), s.chars().collect());
    let (mut pi, mut si, mut star, mut mark) = (0, 0, None, 0);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = si;
            pi += 1;
        } else if let Some(st) = star {
            pi = st + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn create(sources: &[PathBuf], out: &Path, opts: &CreateOptions, p: &Progress) -> Result<CreateReport> {
    let items = collect(sources, &opts.exclude, &[out.to_path_buf()], opts.smart)?;
    let report = create_items(items, out, opts, p)?;
    if opts.delete_sources {
        crate::sys::recycle(sources)?;
    }
    Ok(report)
}

pub(crate) fn create_items(mut items: Vec<Item>, out: &Path, opts: &CreateOptions, p: &Progress) -> Result<CreateReport> {
    if !opts.format.writable() {
        return Err(Error::Unsupported(format!(
            "criar arquivos {} (o formato RAR só pode ser extraído; use 7Z para compactar mais)",
            opts.format.label()
        )));
    }
    if opts.password.is_some() && !matches!(opts.format, Format::Zip | Format::SevenZ) {
        return Err(Error::Unsupported("senha só em ZIP e 7Z".into()));
    }
    let files: Vec<&Item> = items.iter().filter(|i| !i.is_dir).collect();
    if opts.format.is_single_stream() && (files.len() != 1 || items.len() != 1) {
        return Err(Error::Unsupported(format!(
            "{} guarda um arquivo só; para pastas e vários arquivos use TAR.{}",
            opts.format.label(),
            opts.format.label().replace("GZIP", "GZ").replace("ZSTD", "ZST").replace("BZIP2", "BZ2").replace("BROTLI", "BR")
        )));
    }
    p.total.store(files.iter().map(|i| i.size).sum(), Relaxed);
    p.files_total.store(files.len() as u64, Relaxed);
    let input_bytes = p.total.load(Relaxed);

    if opts.solid || opts.format.is_tar() {
        // Pastas antes, depois arquivos parecidos lado a lado.
        items.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| sort_key(&a.name).cmp(&sort_key(&b.name))).then_with(|| a.name.cmp(&b.name)));
    }

    let dir = out.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp = tempfile::Builder::new().prefix(".rustzip-").suffix(".tmp").tempfile_in(dir)?;
    let file = tmp.reopen()?;
    let threads = crate::sys::threads(opts.threads);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().map_err(|e| Error::other(e.to_string()))?;

    let written = pool.install(|| -> Result<()> {
        match opts.format {
            Format::Zip => write_zip(&items, file, opts, p),
            Format::SevenZ => write_7z(&items, file, opts, threads, p),
            f if f.is_tar() => write_tar(&items, file, opts, threads, p),
            _ => write_single(&items[0], file, opts, threads, p),
        }
    });
    if p.is_cancelled() {
        return Err(Error::Cancelled);
    }
    written?;
    tmp.persist(out).map_err(|e| Error::Io(e.error))?;

    if opts.test_after {
        let t = Progress::default();
        crate::extract::test(out, opts.password.as_deref(), &t)?;
    }
    let output_bytes = fs::metadata(out)?.len();
    let outputs = if opts.volume_size > 0 { crate::volumes::split(out, opts.volume_size)? } else { vec![out.to_path_buf()] };
    Ok(CreateReport {
        outputs: outputs.iter().map(|o| o.to_string_lossy().into_owned()).collect(),
        files: p.files_done.load(Relaxed),
        input_bytes,
        output_bytes,
        elapsed_ms: p.elapsed().as_millis() as u64,
        warnings: p.warnings(),
    })
}

/// Arquivo aberto só na primeira leitura e fechado no fim: um bloco sólido com 50 mil arquivos
/// não segura 50 mil handles.
struct LazyFile<'a> {
    path: &'a Path,
    name: &'a str,
    file: Option<File>,
    done: bool,
    p: &'a Progress,
}

impl<'a> LazyFile<'a> {
    fn new(item: &'a Item, p: &'a Progress) -> Self {
        Self { path: &item.abs, name: &item.name, file: None, done: false, p }
    }
}

impl Read for LazyFile<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done {
            return Ok(0);
        }
        if self.file.is_none() {
            self.p.set_current(self.name);
            self.file = Some(File::open(self.path).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", self.path.display())))?);
        }
        let n = self.file.as_mut().unwrap().read(buf)?;
        if n == 0 {
            self.file = None;
            self.done = true;
            self.p.file_done();
        }
        Ok(n)
    }
}

// ---------------------------------------------------------------- ZIP

fn zip_time(t: Option<SystemTime>) -> zip::DateTime {
    t.and_then(|t| unix_to_local(unix_secs(t)))
        .and_then(|dt| zip::DateTime::try_from(dt).ok())
        .unwrap_or_else(zip::DateTime::default_for_write)
}

fn zip_options(item: &Item, opts: &CreateOptions) -> zip::write::SimpleFileOptions {
    use zip::CompressionMethod as M;
    let level = opts.level.min(9);
    let stored = level == 0 || (opts.smart && item.kind == Kind::Incompressible);
    let (method, lvl) = if stored {
        (M::Stored, None)
    } else {
        match opts.zip_method {
            ZipMethod::Deflate => (M::Deflated, Some(level as i64)),
            ZipMethod::Zopfli => (M::Deflated, Some(9 + level as i64)),
            ZipMethod::Zstd => (M::Zstd, Some(zstd_level(level) as i64)),
            ZipMethod::Xz => (M::Xz, Some(level as i64)),
            ZipMethod::Bzip2 => (M::Bzip2, Some(level as i64)),
        }
    };
    zip::write::SimpleFileOptions::default()
        .compression_method(method)
        .compression_level(lvl)
        .last_modified_time(zip_time(item.mtime))
        .large_file(item.size >= 0xFFFF_0000)
}

/// Até este tamanho a entrada é compactada inteira na memória por uma thread do pool.
const ZIP_PARALLEL_MAX: u64 = 64 << 20;
/// Memória máxima de entradas prontas esperando a vez de ir para o disco.
const ZIP_WAVE_BYTES: u64 = 768 << 20;

pub(crate) type ZipOut<'a> = BufWriter<PackedWriter<'a, File>>;

fn write_zip(items: &[Item], file: File, opts: &CreateOptions, p: &Progress) -> Result<()> {
    let mut zw = zip_open(file, opts, p)?;
    zip_add(&mut zw, items, opts, p)?;
    zip_finish(zw)
}

pub(crate) fn zip_open<'a>(file: File, opts: &CreateOptions, p: &'a Progress) -> Result<zip::ZipWriter<ZipOut<'a>>> {
    let out = BufWriter::with_capacity(1 << 20, PackedWriter::new(file, p));
    let mut zw = zip::ZipWriter::new(out);
    if let Some(c) = opts.comment.as_deref().filter(|c| !c.is_empty()) {
        zw.set_comment(c)?;
    }
    Ok(zw)
}

pub(crate) fn zip_finish(zw: zip::ZipWriter<ZipOut>) -> Result<()> {
    let out = zw.finish()?;
    out.into_inner().map_err(|e| e.into_error())?.into_inner().sync_all()?;
    Ok(())
}

/// Grava pastas e arquivos num ZIP aberto (em paralelo quando não há senha).
pub(crate) fn zip_add(zw: &mut zip::ZipWriter<ZipOut>, items: &[Item], opts: &CreateOptions, p: &Progress) -> Result<()> {
    for d in items.iter().filter(|i| i.is_dir) {
        zw.add_directory(format!("{}/", d.name), zip_options(d, opts))?;
    }
    let files: Vec<&Item> = items.iter().filter(|i| !i.is_dir).collect();
    let password = opts.password.as_deref().filter(|s| !s.is_empty());

    let stream_one = |zw: &mut zip::ZipWriter<_>, item: &Item| -> Result<()> {
        p.checkpoint()?;
        p.set_current(&item.name);
        let o = zip_options(item, opts);
        match password {
            Some(pw) => zw.start_file(item.name.as_str(), o.with_aes_encryption(zip::AesMode::Aes256, pw))?,
            None => zw.start_file(item.name.as_str(), o)?,
        }
        let mut r = CountingReader::new(File::open(&item.abs)?, p);
        io::copy(&mut r, zw).map_err(crate::error::io_or_cancel)?;
        p.file_done();
        Ok(())
    };

    if password.is_some() {
        // O zip só criptografa dentro do ZipWriter: sequencial.
        for item in files {
            stream_one(zw, item)?;
        }
    } else {
        let (small, big): (Vec<&Item>, Vec<&Item>) = files.into_iter().partition(|i| i.size <= ZIP_PARALLEL_MAX);
        let mut start = 0;
        while start < small.len() {
            let mut end = start;
            let mut bytes = 0;
            while end < small.len() && (end == start || bytes + small[end].size <= ZIP_WAVE_BYTES) {
                bytes += small[end].size;
                end += 1;
            }
            let prepared: Vec<Result<zip::write::PreparedZipFile>> = small[start..end]
                .par_iter()
                .map(|item| -> Result<_> {
                    p.checkpoint()?;
                    p.set_current(&item.name);
                    let mut b = zip::write::ZipFileBuilder::new(&item.name, zip_options(item, opts))?;
                    let mut r = CountingReader::new(File::open(&item.abs)?, p);
                    io::copy(&mut r, &mut b).map_err(crate::error::io_or_cancel)?;
                    p.file_done();
                    Ok(b.finish()?)
                })
                .collect();
            for f in prepared {
                zw.add_prepared_file(f?)?;
            }
            start = end;
        }
        for item in big {
            stream_one(zw, item)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- 7z

use sevenz_rust2::{ArchiveEntry, ArchiveWriter, EncoderConfiguration, EncoderMethod, NtTime, SourceReader, encoder_options};

fn seven_dict(level: u32, dictionary_mb: u32) -> u64 {
    if dictionary_mb > 0 {
        return (dictionary_mb as u64) << 20;
    }
    // Presets do xz/7-Zip: 0-1 = 256K/1M ... 8 = 32M, 9 = 64M.
    [256 << 10, 1 << 20, 2 << 20, 4 << 20, 4 << 20, 8 << 20, 8 << 20, 16 << 20, 32 << 20, 64 << 20][level.min(9) as usize]
}

fn lzma2(level: u32, dictionary_mb: u32) -> encoder_options::Lzma2Options {
    let mut o = encoder_options::Lzma2Options::from_level(level);
    if dictionary_mb > 0 {
        o.set_dictionary_size((dictionary_mb as u32).saturating_mul(1 << 20));
    }
    o
}

fn methods_for(kind: Kind, opts: &CreateOptions, mt: Option<(u32, u64)>) -> Vec<EncoderConfiguration> {
    let mut m: Vec<EncoderConfiguration> = Vec::new();
    if let Some(pw) = opts.password.as_deref().filter(|s| !s.is_empty()) {
        m.push(encoder_options::AesEncoderOptions::new(sevenz_rust2::Password::new(pw)).into());
    }
    if opts.level == 0 || kind == Kind::Incompressible {
        m.push(EncoderMethod::COPY.into());
        return m;
    }
    let mut lz = match mt {
        Some((threads, chunk)) => encoder_options::Lzma2Options::from_level_mt(opts.level, threads, chunk),
        None => lzma2(opts.level, opts.dictionary_mb),
    };
    if mt.is_some() && opts.dictionary_mb > 0 {
        lz.set_dictionary_size((opts.dictionary_mb as u32).saturating_mul(1 << 20));
    }
    m.push(lz.into());
    if kind == Kind::Executable {
        m.push(EncoderMethod::BCJ_X86_FILTER.into());
    }
    m
}

fn seven_entry(item: &Item) -> ArchiveEntry {
    let mut e = if item.is_dir { ArchiveEntry::new_directory(&item.name) } else { ArchiveEntry::new_file(&item.name) };
    if let Some(t) = item.mtime.and_then(|t| NtTime::try_from(t).ok()) {
        e.last_modified_date = t;
        e.has_last_modified_date = true;
    }
    e
}

/// Arquivos maiores que isto não entram em bloco paralelo: vão sozinhos com LZMA2 multithread.
const SEVEN_BIG: u64 = 512 << 20;
/// Dados de entrada por leva de blocos paralelos (os blocos prontos esperam na memória).
const SEVEN_WINDOW: u64 = 768 << 20;

pub(crate) type SevenOut<'a> = BufWriter<PackedWriter<'a, File>>;

fn write_7z(items: &[Item], file: File, opts: &CreateOptions, threads: usize, p: &Progress) -> Result<()> {
    let mut w = seven_open(file, opts, p)?;
    seven_add(&mut w, items, opts, threads, p)?;
    seven_finish(w, opts)
}

pub(crate) fn seven_open<'a>(file: File, opts: &CreateOptions, p: &'a Progress) -> Result<ArchiveWriter<SevenOut<'a>>> {
    let out = BufWriter::with_capacity(1 << 20, PackedWriter::new(file, p));
    let mut w = ArchiveWriter::new(out)?;
    let has_pw = opts.password.as_deref().is_some_and(|s| !s.is_empty());
    w.set_encrypt_header(has_pw && opts.encrypt_names);
    w.set_content_methods(methods_for(Kind::Normal, opts, None));
    Ok(w)
}

pub(crate) fn seven_finish(mut w: ArchiveWriter<SevenOut>, opts: &CreateOptions) -> Result<()> {
    // O cabeçalho é compactado (e criptografado) com os métodos atuais.
    w.set_content_methods(methods_for(Kind::Normal, opts, None));
    let out = w.finish()?;
    out.into_inner().map_err(|e| e.into_error())?.into_inner().sync_all()?;
    Ok(())
}

pub(crate) fn seven_add(w: &mut ArchiveWriter<SevenOut>, items: &[Item], opts: &CreateOptions, threads: usize, p: &Progress) -> Result<()> {
    for item in items.iter().filter(|i| i.is_dir || i.size == 0) {
        w.push_archive_entry::<&[u8]>(seven_entry(item), None)?;
        if !item.is_dir {
            p.file_done();
        }
    }
    let files: Vec<&Item> = items.iter().filter(|i| !i.is_dir && i.size > 0).collect();

    // LZMA usa ~11x o dicionário por thread: limita as threads ao que cabe na memória livre.
    let dict = seven_dict(opts.level, opts.dictionary_mb);
    let per_thread = dict * 12 + (32 << 20);
    let mem_threads = ((crate::sys::available_memory() as f64 * 0.6) as u64 / per_thread).max(1) as usize;
    let workers = threads.min(mem_threads).max(1);
    if workers < threads {
        p.warn(format!("Memória livre limita a compactação a {workers} threads com dicionário de {} MB", dict >> 20));
    }

    // Cópia pura do que já é compactado: um bloco em fluxo direto para o disco.
    let copy: Vec<&Item> = files.iter().copied().filter(|i| opts.level == 0 || (opts.smart && i.kind == Kind::Incompressible)).collect();
    if !copy.is_empty() {
        w.set_content_methods(methods_for(Kind::Incompressible, opts, None));
        let entries = copy.iter().map(|i| seven_entry(i)).collect();
        let readers = copy.iter().map(|i| SourceReader::new(CountingReader::new(LazyFile::new(i, p), p))).collect();
        w.push_archive_entries(entries, readers)?;
    }
    if opts.level == 0 {
        return Ok(());
    }

    // Todos os grupos (texto, executáveis com BCJ) planejados juntos e compactados no mesmo pool:
    // um grupo não espera o outro terminar.
    let rest: Vec<&Item> = files.iter().copied().filter(|i| !(opts.smart && i.kind == Kind::Incompressible)).collect();
    let (big, rest): (Vec<&Item>, Vec<&Item>) = rest.into_iter().partition(|i| i.size > SEVEN_BIG);
    let total: u64 = rest.iter().map(|i| i.size).sum();
    let target = total.div_ceil(workers as u64).max(dict * 2).max(4 << 20);
    let mut jobs: Vec<(Kind, Vec<&Item>)> = Vec::new();
    for kind in [Kind::Normal, Kind::Executable] {
        let group: Vec<&Item> = rest.iter().copied().filter(|i| if opts.smart { i.kind == kind } else { kind == Kind::Normal }).collect();
        jobs.extend(plan_blocks(&group, opts.solid, target).into_iter().map(|b| (kind, b)));
    }
    // Sobrando threads (poucos blocos), o LZMA2 de cada bloco também se divide em pedaços.
    let per_block = (workers / jobs.len().max(1)).max(1) as u32;

    // Levas grandes (em bytes) para as threads não ficarem esperando o bloco mais lento.
    let mut start = 0;
    while start < jobs.len() {
        let mut end = start;
        let mut bytes = 0u64;
        while end < jobs.len() && (end == start || (bytes < SEVEN_WINDOW && end - start < workers * 64)) {
            bytes += jobs[end].1.iter().map(|i| i.size).sum::<u64>();
            end += 1;
        }
        let prepared: Vec<Result<sevenz_rust2::PreparedBlock>> = jobs[start..end]
            .par_iter()
            .map(|(kind, block)| {
                p.checkpoint()?;
                let bytes: u64 = block.iter().map(|i| i.size).sum();
                let methods = Arc::new(methods_sized(*kind, opts, bytes, per_block, dict));
                let entries = block.iter().map(|i| seven_entry(i)).collect();
                let readers = block.iter().map(|i| SourceReader::new(CountingReader::new(LazyFile::new(i, p), p))).collect();
                Ok(sevenz_rust2::prepare_block(methods, entries, readers)?)
            })
            .collect();
        for b in prepared {
            w.push_prepared_block(b?)?;
        }
        start = end;
    }

    // Arquivos enormes: um bloco cada, com o LZMA2 dividindo o trabalho entre as threads.
    for item in big {
        p.checkpoint()?;
        let chunk = (dict * 4).max(64 << 20);
        w.set_content_methods(methods_for(if opts.smart { item.kind } else { Kind::Normal }, opts, Some((workers as u32, chunk))));
        let r = CountingReader::new(LazyFile::new(item, p), p);
        w.push_archive_entry(seven_entry(item), Some(r))?;
    }
    Ok(())
}

/// Métodos de um bloco com o dicionário do tamanho do bloco (como o 7-Zip faz): um bloco de
/// 20 KB não precisa alocar e zerar as tabelas de um dicionário de 64 MB.
fn methods_sized(kind: Kind, opts: &CreateOptions, bytes: u64, threads: u32, dict: u64) -> Vec<EncoderConfiguration> {
    let fit = bytes.next_power_of_two().clamp(64 << 10, dict);
    let tuned = CreateOptions { dictionary_mb: 0, ..opts.clone() };
    let mut m = methods_for(kind, &tuned, (threads > 1).then_some((threads, dict * 4)));
    for c in &mut m {
        if c.method.id() == EncoderMethod::ID_LZMA2 {
            let mut lz = if threads > 1 {
                encoder_options::Lzma2Options::from_level_mt(opts.level, threads, (fit * 4).max(1 << 20))
            } else {
                encoder_options::Lzma2Options::from_level(opts.level)
            };
            lz.set_dictionary_size(fit as u32);
            *c = EncoderConfiguration::from(lz);
        }
    }
    m
}

/// Divide em blocos sólidos de até `target` bytes, mantendo a ordem (vizinhos parecidos juntos).
fn plan_blocks<'a>(files: &[&'a Item], solid: bool, target: u64) -> Vec<Vec<&'a Item>> {
    if !solid {
        return files.iter().map(|f| vec![*f]).collect();
    }
    let mut blocks = Vec::new();
    let mut cur = Vec::new();
    let mut size = 0;
    for f in files {
        if size > 0 && size + f.size > target {
            blocks.push(std::mem::take(&mut cur));
            size = 0;
        }
        size += f.size;
        cur.push(*f);
    }
    if !cur.is_empty() {
        blocks.push(cur);
    }
    blocks
}

// ---------------------------------------------------------------- TAR e fluxos

fn tuning(opts: &CreateOptions, threads: usize) -> Tuning {
    Tuning { level: opts.level.max(1), threads: threads as u32, dictionary_mb: opts.dictionary_mb }
}

fn write_tar(items: &[Item], file: File, opts: &CreateOptions, threads: usize, p: &Progress) -> Result<()> {
    let out = BufWriter::with_capacity(1 << 20, PackedWriter::new(file, p));
    let enc = encoder(opts.format.stream(), out, &tuning(opts, threads))?;
    let mut b = tar::Builder::new(enc);
    b.mode(tar::HeaderMode::Complete);
    for item in items {
        p.checkpoint()?;
        if item.is_dir {
            b.append_dir(&item.name, &item.abs)?;
            continue;
        }
        p.set_current(&item.name);
        let f = File::open(&item.abs)?;
        let meta = f.metadata()?;
        let mut h = tar::Header::new_gnu();
        h.set_metadata_in_mode(&meta, tar::HeaderMode::Complete);
        h.set_size(meta.len());
        b.append_data(&mut h, &item.name, CountingReader::new(f, p)).map_err(crate::error::io_or_cancel)?;
        p.file_done();
    }
    b.into_inner()?.finish_box()?;
    Ok(())
}

fn write_single(item: &Item, file: File, opts: &CreateOptions, threads: usize, p: &Progress) -> Result<()> {
    p.set_current(&item.name);
    let out = BufWriter::with_capacity(1 << 20, PackedWriter::new(file, p));
    let mut src = CountingReader::new(File::open(&item.abs)?, p);
    if opts.format == Format::Gz {
        // Guarda nome e data originais no cabeçalho gzip.
        let mut gz = flate2::GzBuilder::new()
            .filename(item.name.rsplit('/').next().unwrap_or(&item.name).as_bytes())
            .mtime(item.mtime.map(|t| unix_secs(t).max(0) as u32).unwrap_or(0))
            .write(out, flate2::Compression::new(opts.level.clamp(1, 9)));
        io::copy(&mut src, &mut gz).map_err(crate::error::io_or_cancel)?;
        gz.finish()?.flush()?;
    } else {
        let mut enc = encoder(opts.format.stream(), out, &tuning(opts, threads))?;
        io::copy(&mut src, &mut enc).map_err(crate::error::io_or_cancel)?;
        enc.finish_box()?;
    }
    p.file_done();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(excluded("a/b/x.tmp", &["*.tmp".into()]));
        assert!(excluded("proj/node_modules", &["node_modules".into()]));
        assert!(excluded("cache/x.bin", &["cache/*.bin".into()]));
        assert!(!excluded("a/b/x.txt", &["*.tmp".into()]));
        assert!(excluded("a/IMG_01.JPG", &["img_??.jpg".into()]));
    }
}
