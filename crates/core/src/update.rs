//! Alterar um arquivo compactado existente: adicionar, excluir, renomear e converter.
//!
//! ZIP e 7z copiam o que não mudou byte a byte (sem descompactar nem recompactar): excluir um
//! arquivo de um ZIP de 10 GB é só uma cópia de disco. No 7z, um bloco sólido só é
//! recompactado se perdeu ou trocou alguma entrada. Os outros formatos (TAR e fluxos) não têm
//! índice e são refeitos por uma pasta temporária.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;

use serde::Deserialize;

use crate::create::{self, CreateOptions, CreateReport, Item, collect};
use crate::error::{Error, Result};
use crate::extract::{self, ExtractOptions};
use crate::format::Format;
use crate::fsutil::normalize_entry;
use crate::progress::Progress;
use crate::smart::classify;
use crate::source::Source;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Edit {
    /// Entradas ou pastas a excluir.
    pub delete: Vec<String>,
    /// (de, para): renomeia entradas e pastas inteiras.
    pub rename: Vec<(String, String)>,
    /// Arquivos e pastas do disco a adicionar (substituem entradas com o mesmo nome).
    pub add: Vec<PathBuf>,
    /// Pasta dentro do arquivo compactado onde os novos itens entram ("" = raiz).
    pub add_to: String,
}

impl Edit {
    fn deleted(&self, name: &str) -> bool {
        self.delete.iter().map(|d| normalize_entry(d)).any(|d| name == d || name.starts_with(&format!("{d}/")))
    }

    fn renamed(&self, name: &str) -> String {
        for (from, to) in &self.rename {
            let (from, to) = (normalize_entry(from), normalize_entry(to));
            if name == from {
                return to;
            }
            if let Some(rest) = name.strip_prefix(&format!("{from}/")) {
                return format!("{to}/{rest}");
            }
        }
        name.to_string()
    }
}

/// Aplica `edit` ao arquivo. `opts` dá nível, senha e métodos dos dados novos (o formato é o do
/// próprio arquivo).
pub fn modify(archive: &Path, edit: &Edit, opts: &CreateOptions, p: &Progress) -> Result<CreateReport> {
    let format = Format::detect(archive)?;
    if !format.writable() {
        return Err(Error::Unsupported("arquivos RAR são somente leitura; converta para 7Z para alterar".into()));
    }
    if Source::open(archive)?.volumes() > 1 {
        return Err(Error::Unsupported("arquivos divididos em volumes não podem ser alterados; extraia ou converta".into()));
    }
    let opts = CreateOptions { format, volume_size: 0, delete_sources: false, ..opts.clone() };
    let prefix = normalize_entry(&edit.add_to);
    let mut new_items = collect(&edit.add, &opts.exclude, &[archive.to_path_buf()], opts.smart)?;
    for i in &mut new_items {
        if !prefix.is_empty() {
            i.name = format!("{prefix}/{}", i.name);
        }
    }
    let replaced: HashSet<String> = new_items.iter().map(|i| i.name.to_lowercase()).collect();
    let keep = |name: &str| !edit.deleted(name) && !replaced.contains(&edit.renamed(name).to_lowercase());

    let dir = archive.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let tmp = tempfile::Builder::new().prefix(".rustzip-").suffix(".tmp").tempfile_in(dir)?;
    p.total.store(new_items.iter().filter(|i| !i.is_dir).map(|i| i.size).sum(), Relaxed);
    let threads = crate::sys::threads(opts.threads);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().map_err(|e| Error::other(e.to_string()))?;

    // Formatos sem índice são refeitos por `create_items`, que grava num arquivo dele.
    let staged = dir.join(format!(".rustzip-{}-{}.new", std::process::id(), p.elapsed().as_nanos()));
    let result = pool.install(|| match format {
        Format::Zip => modify_zip(archive, tmp.reopen()?, edit, &keep, &new_items, &opts, p),
        Format::SevenZ => modify_7z(archive, tmp.reopen()?, edit, &keep, &new_items, &opts, threads, p),
        _ => {
            let work = tempfile::Builder::new().prefix(".rustzip-").tempdir_in(dir)?;
            let eo = ExtractOptions { password: opts.password.clone(), threads: opts.threads, ..Default::default() };
            extract::extract(archive, work.path(), &eo, &Progress::default())?;
            let mut items: Vec<Item> = root_items(work.path(), opts.smart)?
                .into_iter()
                .filter(|i| keep(&i.name))
                .map(|mut i| {
                    i.name = edit.renamed(&i.name);
                    i
                })
                .collect();
            items.extend(new_items.iter().cloned());
            create::create_items(items, &staged, &CreateOptions { test_after: false, ..opts.clone() }, p).map(|_| ())
        }
    });
    if p.is_cancelled() || result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    if p.is_cancelled() {
        return Err(Error::Cancelled);
    }
    result?;
    if staged.exists() {
        drop(tmp);
        fs::rename(&staged, archive)?;
    } else {
        tmp.persist(archive).map_err(|e| Error::Io(e.error))?;
    }
    if opts.test_after {
        extract::test(archive, opts.password.as_deref(), &Progress::default())?;
    }
    let size = fs::metadata(archive)?.len();
    Ok(CreateReport {
        outputs: vec![archive.to_string_lossy().into_owned()],
        files: p.files_done.load(Relaxed),
        input_bytes: p.total.load(Relaxed),
        output_bytes: size,
        elapsed_ms: p.elapsed().as_millis() as u64,
        warnings: p.warnings(),
    })
}

fn modify_zip(
    archive: &Path,
    out: File,
    edit: &Edit,
    keep: &dyn Fn(&str) -> bool,
    new_items: &[Item],
    opts: &CreateOptions,
    p: &Progress,
) -> Result<()> {
    let mut src = zip::ZipArchive::new(Source::open(archive)?)?;
    let mut keep_opts = opts.clone();
    if keep_opts.comment.is_none() {
        let c = String::from_utf8_lossy(src.comment()).into_owned();
        keep_opts.comment = (!c.is_empty()).then_some(c);
    }
    let mut zw = create::zip_open(out, &keep_opts, p)?;
    for i in 0..src.len() {
        p.checkpoint()?;
        let f = src.by_index_raw(i)?;
        let name = normalize_entry(&f.name()?);
        if !keep(&name) {
            continue;
        }
        let new_name = edit.renamed(&name);
        let new_name = if f.is_dir() { format!("{new_name}/") } else { new_name };
        zw.raw_copy_file_rename(f, new_name)?;
    }
    create::zip_add(&mut zw, new_items, opts, p)?;
    create::zip_finish(zw)
}

#[allow(clippy::too_many_arguments)]
fn modify_7z(
    archive: &Path,
    out: File,
    edit: &Edit,
    keep: &dyn Fn(&str) -> bool,
    new_items: &[Item],
    opts: &CreateOptions,
    threads: usize,
    p: &Progress,
) -> Result<()> {
    let mut src = Source::open(archive)?;
    let pw = sevenz_rust2::Password::new(opts.password.as_deref().unwrap_or(""));
    let parsed = sevenz_rust2::Archive::read(&mut src, &pw)?;
    let map = &parsed.stream_map;
    let mut w = create::seven_open(out, opts, p)?;

    // Entradas sem dados (pastas, vazios): só metadados.
    for (i, e) in parsed.files.iter().enumerate() {
        if map.file_block_index[i].is_none() && keep(&normalize_entry(e.name())) {
            let mut e = e.clone();
            e.name = edit.renamed(&normalize_entry(&e.name));
            w.push_archive_entry::<&[u8]>(e, None)?;
        }
    }
    // Blocos intactos vão crus; blocos mexidos têm o que sobrou extraído e recompactado.
    let work = tempfile::Builder::new().prefix(".rustzip-").tempdir_in(archive.parent().unwrap_or(Path::new(".")))?;
    let mut redo: Vec<Item> = Vec::new();
    for b in 0..parsed.blocks.len() {
        p.checkpoint()?;
        let first = map.block_first_file_index[b];
        let names: Vec<String> = (first..parsed.files.len())
            .take_while(|&i| map.file_block_index[i].is_none_or(|x| x == b))
            .filter(|&i| map.file_block_index[i] == Some(b))
            .map(|i| normalize_entry(parsed.files[i].name()))
            .collect();
        if names.iter().all(|n| keep(n)) {
            w.push_raw_block(&parsed, b, &mut src, |e| edit.renamed(&normalize_entry(e.name())))?;
            continue;
        }
        let mut s = src.clone();
        let decoder = sevenz_rust2::BlockDecoder::new(threads as u32, b, &parsed, &pw, &mut s);
        let mut failure = None;
        decoder.for_each_entries(&mut |entry, reader| {
            let name = normalize_entry(entry.name());
            if !keep(&name) {
                return Ok(true);
            }
            let path = work.path().join(format!("{}", redo.len()));
            let r = (|| -> Result<()> {
                let mut f = File::create(&path)?;
                io::copy(reader, &mut f)?;
                Ok(())
            })();
            if let Err(e) = r {
                failure = Some(e);
                return Ok(false);
            }
            let size = entry.size();
            let mtime = entry.has_last_modified_date.then(|| entry.last_modified_date().into());
            redo.push(Item { kind: if opts.smart { classify(Path::new(&name), size) } else { Default::default() }, abs: path, name: edit.renamed(&name), is_dir: false, size, mtime });
            Ok(true)
        })?;
        if let Some(e) = failure {
            return Err(e);
        }
    }
    p.total.fetch_add(redo.iter().map(|i| i.size).sum(), Relaxed);
    redo.extend(new_items.iter().cloned());
    create::seven_add(&mut w, &redo, opts, threads, p)?;
    create::seven_finish(w, opts)
}

/// Itens de uma pasta temporária, com nomes relativos a ela.
fn root_items(root: &Path, smart: bool) -> Result<Vec<Item>> {
    let tops: Vec<PathBuf> = fs::read_dir(root)?.map(|e| e.map(|e| e.path())).collect::<io::Result<_>>()?;
    collect(&tops, &[], &[], smart)
}

/// Converte para outro formato (por exemplo RAR -> 7z), passando por uma pasta temporária ao lado
/// do destino.
pub fn convert(archive: &Path, out: &Path, password: Option<&str>, opts: &CreateOptions, p: &Progress) -> Result<CreateReport> {
    let dir = out.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let work = tempfile::Builder::new().prefix(".rustzip-").tempdir_in(dir)?;
    p.set_current("Extraindo...");
    let eo = ExtractOptions { password: password.map(str::to_string), threads: opts.threads, ..Default::default() };
    extract::extract(archive, work.path(), &eo, p)?;
    p.done.store(0, Relaxed);
    p.files_done.store(0, Relaxed);
    let items = root_items(work.path(), opts.smart)?;
    create::create_items(items, out, opts, p)
}
