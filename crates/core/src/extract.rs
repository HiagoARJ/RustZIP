//! Extração e teste. ZIP extrai entradas em paralelo; 7z extrai blocos em paralelo (e usa o
//! LZMA2 multithread quando há um bloco só); TAR e fluxos usam os decodificadores paralelos de
//! xz/zstd. O teste é a mesma extração gravando em lugar nenhum.

use std::fs::{self, File};
use std::io::{self, BufWriter, Read};
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::io::Write;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering::Relaxed;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result, cancel_io, is_cancel};
use crate::format::Format;
use crate::fsutil::{Overwrite, dos_to_unix, from_unix, normalize_entry, resolve_target, safe_join};
use crate::progress::{CountingReader, Progress};
use crate::source::Source;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtractOptions {
    pub password: Option<String>,
    pub overwrite: Overwrite,
    /// Entradas (ou pastas) a extrair; vazio = tudo.
    pub selection: Vec<String>,
    /// Pasta do arquivo compactado que vira a raiz do destino (extrair de dentro de uma subpasta).
    pub strip_prefix: Option<String>,
    /// 0 = automático.
    pub threads: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub files: u64,
    pub bytes: u64,
    pub elapsed_ms: u64,
    pub warnings: Vec<String>,
}

enum Target<'a> {
    Disk { dest: &'a Path, overwrite: Overwrite },
    Test,
}

struct Plan<'a> {
    target: Target<'a>,
    selection: Vec<String>,
    strip: Option<String>,
    /// Pastas já criadas: com milhares de arquivos pequenos, cada `create_dir_all` a menos conta.
    made: Mutex<HashSet<PathBuf>>,
    /// Gravação paralela de arquivos pequenos quando a leitura é sequencial (TAR, 7z sólido,
    /// RAR): criar um arquivo no Windows custa ~1 ms (antivírus), e várias threads escondem isso.
    writer: Mutex<Option<SyncSender<Job>>>,
    write_error: Mutex<Option<Error>>,
}

struct Job {
    path: PathBuf,
    data: Vec<u8>,
    mtime: Option<i64>,
}

/// Até este tamanho a entrada é lida para a memória e gravada por uma thread do pool.
const ASYNC_MAX: u64 = 1 << 20;

fn write_file(path: &Path, data: &[u8], mtime: Option<i64>) -> Result<()> {
    let result = (|| -> Result<()> {
        let mut f = File::create(path)?;
        f.write_all(data)?;
        if let Some(t) = mtime {
            let _ = f.set_times(fs::FileTimes::new().set_modified(from_unix(t)));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

impl Plan<'_> {
    /// Nome de saída relativo, ou `None` se a entrada não foi selecionada.
    fn map(&self, entry: &str) -> Option<String> {
        let path = normalize_entry(entry);
        if !self.selection.is_empty()
            && !self.selection.iter().any(|s| path == *s || path.starts_with(&format!("{s}/")))
        {
            return None;
        }
        match &self.strip {
            Some(prefix) if !prefix.is_empty() => {
                let rest = path.strip_prefix(prefix.as_str())?.strip_prefix('/')?;
                (!rest.is_empty()).then(|| rest.to_string())
            }
            _ => (!path.is_empty()).then_some(path),
        }
    }

    /// Grava (ou só lê, no teste) uma entrada. `count` = somar os bytes descompactados no progresso
    /// (falso quando o progresso já conta os bytes compactados lidos).
    fn write(
        &self,
        name: &str,
        is_dir: bool,
        mtime: Option<i64>,
        size: u64,
        reader: &mut dyn Read,
        p: &Progress,
        count: bool,
    ) -> Result<()> {
        p.checkpoint()?;
        let mut sink_reader;
        let mut plain_reader;
        let reader: &mut dyn Read = if count {
            sink_reader = CountingReader::new(reader, p);
            &mut sink_reader
        } else {
            plain_reader = CheckReader { inner: reader, p };
            &mut plain_reader
        };
        match &self.target {
            Target::Test => {
                if !is_dir {
                    p.set_current(name);
                    io::copy(reader, &mut io::sink()).map_err(map_read_err)?;
                    p.file_done();
                }
                Ok(())
            }
            Target::Disk { dest, overwrite } => {
                let path = safe_join(dest, name)?;
                if is_dir {
                    self.make_dir(&path)?;
                    return Ok(());
                }
                if let Some(parent) = path.parent() {
                    self.make_dir(parent)?;
                }
                // Um único stat decide existência, política e somente-leitura.
                let path = match fs::symlink_metadata(&path) {
                    Err(_) => path,
                    Ok(meta) => {
                        let Some(target) = resolve_target(path, *overwrite, mtime) else {
                            p.file_done();
                            return Ok(());
                        };
                        if meta.permissions().readonly() {
                            clear_readonly(&target);
                        }
                        target
                    }
                };
                p.set_current(name);
                if size <= ASYNC_MAX
                    && let Some(tx) = self.writer.lock().unwrap().clone()
                {
                    let mut data = Vec::with_capacity(size as usize);
                    reader.read_to_end(&mut data).map_err(map_read_err)?;
                    if tx.send(Job { path, data, mtime }).is_err() {
                        return Err(self.write_error.lock().unwrap().take().unwrap_or(Error::Cancelled));
                    }
                    return Ok(());
                }
                let cap = size.clamp(4096, 1 << 20) as usize;
                let result = (|| -> Result<()> {
                    let mut out = BufWriter::with_capacity(cap, File::create(&path)?);
                    io::copy(reader, &mut out).map_err(map_read_err)?;
                    let file = out.into_inner().map_err(|e| e.into_error())?;
                    // Data no mesmo handle: reabrir o arquivo só para isso custa outro open/close
                    // (e outra varredura do antivírus).
                    if let Some(t) = mtime {
                        let _ = file.set_times(fs::FileTimes::new().set_modified(from_unix(t)));
                    }
                    Ok(())
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&path);
                }
                result?;
                p.file_done();
                Ok(())
            }
        }
    }
}

impl Plan<'_> {
    fn fail(&self, e: Error, p: &Progress) {
        self.write_error.lock().unwrap().get_or_insert(e);
        p.cancel();
    }

    fn make_dir(&self, dir: &Path) -> Result<()> {
        if self.made.lock().unwrap().contains(dir) {
            return Ok(());
        }
        fs::create_dir_all(dir)?;
        self.made.lock().unwrap().insert(dir.to_path_buf());
        Ok(())
    }
}

struct CheckReader<'a> {
    inner: &'a mut dyn Read,
    p: &'a Progress,
}

impl Read for CheckReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.p.checkpoint().map_err(|_| cancel_io())?;
        self.inner.read(buf)
    }
}

fn map_read_err(e: io::Error) -> Error {
    if is_cancel(&e) {
        Error::Cancelled
    } else if e.kind() == io::ErrorKind::InvalidData || e.to_string().contains("checksum") {
        Error::Corrupt(e.to_string())
    } else {
        Error::Io(e)
    }
}

fn clear_readonly(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut perm = meta.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = fs::set_permissions(path, perm);
        }
    }
}

pub fn extract(path: &Path, dest: &Path, opts: &ExtractOptions, p: &Progress) -> Result<Report> {
    fs::create_dir_all(dest)?;
    run(path, Target::Disk { dest, overwrite: opts.overwrite }, opts, p)
}

pub fn test(path: &Path, password: Option<&str>, p: &Progress) -> Result<Report> {
    let opts = ExtractOptions { password: password.map(str::to_string), ..Default::default() };
    run(path, Target::Test, &opts, p)
}

fn run(path: &Path, target: Target, opts: &ExtractOptions, p: &Progress) -> Result<Report> {
    let plan = Plan {
        target,
        selection: opts.selection.iter().map(|s| normalize_entry(s)).collect(),
        strip: opts.strip_prefix.as_deref().map(normalize_entry),
        made: Mutex::new(HashSet::new()),
        writer: Mutex::new(None),
        write_error: Mutex::new(None),
    };
    let threads = crate::sys::threads(opts.threads);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().map_err(|e| Error::other(e.to_string()))?;
    let format = Format::detect(path)?;
    let password = opts.password.as_deref().filter(|s| !s.is_empty());
    // O ZIP já grava em paralelo pelo próprio pool.
    let pooled = matches!(plan.target, Target::Disk { .. }) && format != Format::Zip;
    let result = std::thread::scope(|s| {
        if pooled {
            let (tx, rx) = sync_channel::<Job>(256);
            *plan.writer.lock().unwrap() = Some(tx);
            let rx = Arc::new(Mutex::new(rx));
            for _ in 0..threads.clamp(2, 16) {
                let (rx, plan) = (rx.clone(), &plan);
                s.spawn(move || {
                    loop {
                        let job = rx.lock().unwrap().recv();
                        let Ok(job) = job else { break };
                        match write_file(&job.path, &job.data, job.mtime) {
                            Ok(()) => p.file_done(),
                            Err(e) => plan.fail(e, p),
                        }
                    }
                });
            }
        }
        let r = match format {
            Format::Zip => pool.install(|| zip(path, &plan, password, p)),
            Format::SevenZ => pool.install(|| seven_z(path, &plan, password, threads as u32, p)),
            Format::Rar => rar(path, &plan, password, p),
            f if f.is_tar() => tar(path, f, &plan, p),
            f => single(path, f, &plan, p),
        };
        // Solta o canal: as threads de gravação terminam a fila e saem antes do fim do escopo.
        plan.writer.lock().unwrap().take();
        r
    });
    if let Some(e) = plan.write_error.lock().unwrap().take() {
        return Err(e);
    }
    if p.is_cancelled() {
        return Err(Error::Cancelled);
    }
    result?;
    Ok(Report {
        files: p.files_done.load(Relaxed),
        bytes: p.done.load(Relaxed),
        elapsed_ms: p.elapsed().as_millis() as u64,
        warnings: p.warnings(),
    })
}

fn zip(path: &Path, plan: &Plan, password: Option<&str>, p: &Progress) -> Result<()> {
    let archive = zip::ZipArchive::new(Source::open(path)?)?;
    let mut jobs = Vec::new();
    {
        let mut a = archive.clone();
        for i in 0..a.len() {
            let f = a.by_index_raw(i)?;
            let Some(name) = plan.map(&f.name()?) else { continue };
            let mtime = f.last_modified().and_then(|d| dos_to_unix(d.datepart(), d.timepart()));
            if f.is_dir() {
                plan.write(&name, true, mtime, 0, &mut io::empty(), p, false)?;
            } else {
                p.total.fetch_add(f.size(), Relaxed);
                jobs.push((i, name, mtime, f.size()));
            }
        }
    }
    p.files_total.store(jobs.len() as u64, Relaxed);
    // Grandes primeiro: evita uma thread sozinha no fim com o maior arquivo.
    jobs.sort_by_key(|j| std::cmp::Reverse(j.3));
    jobs.par_iter().try_for_each(|(i, name, mtime, size)| {
        let mut a = archive.clone();
        let mut f = match password {
            Some(pw) => a.by_index_decrypt(*i, pw.as_bytes())?,
            None => a.by_index(*i)?,
        };
        plan.write(name, false, *mtime, *size, &mut f, p, true)
    })
}

fn seven_z(path: &Path, plan: &Plan, password: Option<&str>, threads: u32, p: &Progress) -> Result<()> {
    let mut src = Source::open(path)?;
    let pw = sevenz_rust2::Password::new(password.unwrap_or(""));
    let archive = sevenz_rust2::Archive::read(&mut src, &pw)?;
    let map = &archive.stream_map;
    let mut blocks: Vec<usize> = Vec::new();
    for (i, e) in archive.files.iter().enumerate() {
        let Some(name) = plan.map(e.name()) else { continue };
        let mtime = e.has_last_modified_date.then(|| crate::fsutil::unix_secs(e.last_modified_date().into()));
        match map.file_block_index[i] {
            Some(b) => {
                p.total.fetch_add(e.size(), Relaxed);
                p.files_total.fetch_add(1, Relaxed);
                if blocks.last() != Some(&b) {
                    blocks.push(b);
                }
            }
            // Pastas e arquivos vazios não têm dados.
            None => plan.write(&name, e.is_directory(), mtime, 0, &mut io::empty(), p, false)?,
        }
    }
    blocks.dedup();
    let per_block_threads = if blocks.len() >= threads as usize { 1 } else { (threads / blocks.len().max(1) as u32).max(1) };
    let encrypted = (0..archive.files.len()).any(|i| archive.is_file_encrypted(i));
    let result = blocks.par_iter().try_for_each(|&b| -> Result<()> {
        let mut s = src.clone();
        let decoder = sevenz_rust2::BlockDecoder::new(per_block_threads, b, &archive, &pw, &mut s);
        let mut failure: Option<Error> = None;
        let outcome = decoder.for_each_entries(&mut |entry, reader| {
            let Some(name) = plan.map(entry.name()) else { return Ok(true) };
            let mtime = entry.has_last_modified_date.then(|| crate::fsutil::unix_secs(entry.last_modified_date().into()));
            match plan.write(&name, entry.is_directory(), mtime, entry.size(), reader, p, true) {
                Ok(()) => Ok(true),
                Err(e) => {
                    failure = Some(e);
                    Ok(false)
                }
            }
        });
        if let Some(e) = failure {
            return Err(e);
        }
        outcome?;
        Ok(())
    });
    // Com a senha errada o AES "decifra" lixo e o LZMA falha de várias formas.
    match result {
        Err(Error::Io(_) | Error::SevenZ(_) | Error::Corrupt(_)) if encrypted && password.is_some() => Err(Error::WrongPassword),
        other => other,
    }
}

fn rar(path: &Path, plan: &Plan, password: Option<&str>, p: &Progress) -> Result<()> {
    let first = unrar::Archive::new(path).first_part();
    // Primeiro passa listando: total para a barra de progresso.
    {
        let a = match password {
            Some(pw) => unrar::Archive::with_password(&first, pw),
            None => unrar::Archive::new(&first),
        };
        for e in a.open_for_listing()?.flatten() {
            if !e.is_directory() && !e.is_split() && plan.map(&e.filename.to_string_lossy()).is_some() {
                p.total.fetch_add(e.unpacked_size, Relaxed);
                p.files_total.fetch_add(1, Relaxed);
            }
        }
    }
    let a = match password {
        Some(pw) => unrar::Archive::with_password(&first, pw),
        None => unrar::Archive::new(&first),
    };
    let mut cursor = a.open_for_processing()?;
    while let Some(header) = cursor.read_header()? {
        p.checkpoint()?;
        let e = header.entry();
        let size = e.unpacked_size;
        let is_dir = e.is_directory();
        let name = plan.map(&e.filename.to_string_lossy());
        let mtime = dos_to_unix((e.file_time >> 16) as u16, (e.file_time & 0xffff) as u16);
        cursor = match (name, &plan.target) {
            (None, _) => header.skip()?,
            (Some(name), Target::Test) => {
                p.set_current(&name);
                let next = header.test()?;
                if !is_dir {
                    p.add(size);
                    p.file_done();
                }
                next
            }
            // Pequenos: o unrar descompacta para a memória e o pool grava em paralelo.
            (Some(name), Target::Disk { .. }) if !is_dir && size <= ASYNC_MAX && !e.is_split() => {
                let (data, next) = header.read()?;
                plan.write(&name, false, mtime, size, &mut data.as_slice(), p, true)?;
                next
            }
            (Some(name), Target::Disk { dest, overwrite }) => {
                let out = safe_join(dest, &name)?;
                if is_dir {
                    plan.make_dir(&out)?;
                    header.skip()?
                } else if let Some(out) = resolve_target(out, *overwrite, mtime) {
                    p.set_current(&name);
                    if let Some(parent) = out.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    clear_readonly(&out);
                    let next = header.extract_to(&out)?;
                    p.add(size);
                    p.file_done();
                    next
                } else {
                    p.add(size);
                    header.skip()?
                }
            }
        };
    }
    Ok(())
}

/// Lê o arquivo compactado contando os bytes compactados (o total descompactado de um TAR
/// comprimido só se sabe no fim).
fn counted_source<'a>(path: &Path, p: &'a Progress) -> Result<impl Read + Send + 'a> {
    let src = Source::open(path)?;
    p.total.store(src.len(), Relaxed);
    Ok(CountingReader::new(src, p))
}

fn tar(path: &Path, format: Format, plan: &Plan, p: &Progress) -> Result<()> {
    let reader = crate::streams::decoder(format.stream(), Box::new(counted_source(path, p)?))?;
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(map_read_err)? {
        let mut e = entry.map_err(map_read_err)?;
        let kind = e.header().entry_type();
        let raw = e.path().map_err(map_read_err)?.to_string_lossy().into_owned();
        let Some(name) = plan.map(&raw) else { continue };
        let mtime = e.header().mtime().ok().map(|m| m as i64);
        if kind.is_dir() {
            plan.write(&name, true, mtime, 0, &mut io::empty(), p, false)?;
        } else if kind.is_file() || kind.is_contiguous() || kind.is_gnu_sparse() {
            p.files_total.fetch_add(1, Relaxed);
            let size = e.size();
            plan.write(&name, false, mtime, size, &mut e, p, false)?;
        } else if kind.is_symlink() || kind.is_hard_link() {
            p.warn(format!("Link ignorado: {name}"));
        }
    }
    Ok(())
}

fn single(path: &Path, format: Format, plan: &Plan, p: &Progress) -> Result<()> {
    let mut name = crate::list::single_stream_name(path);
    if format == Format::Gz
        && let Ok(src) = Source::open(path)
        && let Some(h) = flate2::read::GzDecoder::new(src).header()
        && let Some(n) = h.filename()
    {
        let n = String::from_utf8_lossy(n);
        // Só o nome: o cabeçalho gzip não deve escolher pasta.
        if let Some(base) = n.rsplit(['/', '\\']).next().filter(|b| !b.is_empty()) {
            name = base.to_string();
        }
    }
    let mtime = fs::metadata(path).and_then(|m| m.modified()).ok().map(crate::fsutil::unix_secs);
    let Some(out) = plan.map(&name) else { return Ok(()) };
    p.files_total.store(1, Relaxed);
    let mut reader = crate::streams::decoder(format.stream(), Box::new(counted_source(path, p)?))?;
    plan.write(&out, false, mtime, 1 << 20, &mut reader, p, false)
}

/// Pasta "Extrair para <nome>\" ao lado do arquivo compactado.
pub fn default_folder(archive: &Path) -> PathBuf {
    let name = archive.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = crate::format::volume_base(&name);
    let name = name.strip_suffix(".rar").map(|s| s.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end_matches(".part")).unwrap_or(name);
    let (stem, _) = crate::fsutil::split_archive_name(name);
    archive.parent().unwrap_or(Path::new(".")).join(stem)
}
