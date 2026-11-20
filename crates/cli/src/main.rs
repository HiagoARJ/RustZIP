//! `rz` — RustZIP em linha de comando, no espírito do Rar.exe:
//!
//! ```text
//! rz a [opções] destino.7z arquivos...   compactar (formato pela extensão)
//! rz x [opções] arquivo [pasta]          extrair com caminhos
//! rz e [opções] arquivo [pasta]          extrair sem caminhos
//! rz l arquivo                           listar
//! rz t arquivo                           testar
//! rz d arquivo entradas...               excluir entradas
//! rz rn arquivo de para                  renomear entrada/pasta
//! rz c arquivo destino.7z                converter
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::Duration;

use rustzip_core::{CreateOptions, Edit, Error, ExtractOptions, Format, Overwrite, Progress, ZipMethod};

const HELP: &str = "RustZIP rz - uso: rz <a|x|e|l|t|d|rn|c> [opções] arquivo [arquivos...]
  a   compactar (formato pela extensão: .7z .zip .tar.zst .tar.xz .tar.gz .zst ...)
  x   extrair com caminhos      e   extrair sem caminhos
  l   listar   t   testar   d   excluir entradas   rn  renomear   c   converter
Opções:
  -m0..-m9      nível (0 = armazenar, 9 = melhor)      -md<MB>   dicionário/janela
  -p<senha>     senha (ZIP AES-256 / 7z AES-256)       -hp       criptografar nomes (7z)
  -mt<n>        threads                                 -s-       não sólido
  -v<n>[k|m|g]  volumes                                 -x<pad>   excluir (*.tmp, node_modules)
  -zm<método>   ZIP: deflate|zopfli|zstd|xz|bzip2       -t        testar depois
  -o+ -o- -or -on   sobrescrever / pular / renomear / só mais novos
  -df           mandar os originais para a Lixeira      -smart-   recompactar tudo";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        println!("{HELP}");
        return;
    }
    if let Err(e) = run(&args) {
        eprintln!("ERRO: {e}");
        std::process::exit(match e.code() {
            "password_required" | "wrong_password" => 11,
            "cancelled" => 255,
            _ => 2,
        });
    }
}

fn parse(args: &[String]) -> (CreateOptions, ExtractOptions, Vec<String>) {
    let mut c = CreateOptions::default();
    let mut x = ExtractOptions::default();
    let mut rest = Vec::new();
    for a in args {
        let s = a.as_str();
        match s {
            "-hp" => c.encrypt_names = true,
            "-s-" => c.solid = false,
            "-t" => c.test_after = true,
            "-df" => c.delete_sources = true,
            "-smart-" => c.smart = false,
            "-o+" => x.overwrite = Overwrite::Overwrite,
            "-o-" => x.overwrite = Overwrite::Skip,
            "-or" => x.overwrite = Overwrite::Rename,
            "-on" => x.overwrite = Overwrite::Newer,
            _ if s.starts_with("-mt") => c.threads = s[3..].parse().unwrap_or(0),
            _ if s.starts_with("-md") => c.dictionary_mb = s[3..].trim_end_matches(['m', 'M']).parse().unwrap_or(0),
            _ if s.starts_with("-m") && s.len() == 3 => c.level = s[2..].parse().unwrap_or(5),
            _ if s.starts_with("-p") && s.len() > 2 => {
                c.password = Some(s[2..].to_string());
                x.password = Some(s[2..].to_string());
            }
            _ if s.starts_with("-v") && s.len() > 2 => c.volume_size = size(&s[2..]),
            _ if s.starts_with("-x") && s.len() > 2 => c.exclude.push(s[2..].to_string()),
            _ if s.starts_with("-zm") => {
                c.zip_method = match &s[3..] {
                    "zopfli" => ZipMethod::Zopfli,
                    "zstd" => ZipMethod::Zstd,
                    "xz" => ZipMethod::Xz,
                    "bzip2" => ZipMethod::Bzip2,
                    _ => ZipMethod::Deflate,
                }
            }
            _ => rest.push(a.clone()),
        }
    }
    x.threads = c.threads;
    (c, x, rest)
}

fn size(s: &str) -> u64 {
    let s = s.to_ascii_lowercase();
    let (num, mul) = match s.chars().last() {
        Some('k') => (&s[..s.len() - 1], 1u64 << 10),
        Some('m') => (&s[..s.len() - 1], 1 << 20),
        Some('g') => (&s[..s.len() - 1], 1 << 30),
        _ => (&s[..], 1),
    };
    num.parse::<u64>().unwrap_or(0) * mul
}

/// Roda `f` mostrando o progresso numa linha do stderr.
fn with_progress<T: Send>(f: impl FnOnce(&Progress) -> rustzip_core::Result<T> + Send) -> rustzip_core::Result<T> {
    let p = Arc::new(Progress::default());
    let done = Arc::new(AtomicBool::new(false));
    std::thread::scope(|s| {
        let (p2, d2) = (p.clone(), done.clone());
        s.spawn(move || {
            while !d2.load(Relaxed) {
                let snap = p2.snapshot();
                if snap.total > 0 {
                    let mbs = snap.done as f64 / 1048576.0 / (snap.elapsed_ms.max(1) as f64 / 1000.0);
                    eprint!("\r{:>3}%  {mbs:>8.1} MB/s  ", snap.done * 100 / snap.total.max(1));
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            eprint!("\r                          \r");
        });
        let r = f(&p);
        done.store(true, Relaxed);
        r
    })
}

fn run(args: &[String]) -> rustzip_core::Result<()> {
    let cmd = args[0].as_str();
    let (mut c, x, rest) = parse(&args[1..]);
    let need = |n: usize| if rest.len() < n { Err(Error::other("faltam argumentos (rz -h)")) } else { Ok(()) };
    match cmd {
        "a" => {
            need(2)?;
            let out = PathBuf::from(&rest[0]);
            c.format = Format::from_name(&rest[0]).unwrap_or(Format::SevenZ);
            let sources: Vec<PathBuf> = rest[1..].iter().map(PathBuf::from).collect();
            let r = with_progress(|p| rustzip_core::create(&sources, &out, &c, p))?;
            println!(
                "{} arquivos, {} -> {} ({:.1}%), {:.2}s",
                r.files,
                human(r.input_bytes),
                human(r.output_bytes),
                r.output_bytes as f64 * 100.0 / r.input_bytes.max(1) as f64,
                r.elapsed_ms as f64 / 1000.0
            );
            for w in r.warnings {
                eprintln!("aviso: {w}");
            }
        }
        "x" | "e" => {
            need(1)?;
            let arc = PathBuf::from(&rest[0]);
            let dest = rest.get(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
            if cmd == "e" {
                return extract_flat(&arc, &dest, &x);
            }
            let r = with_progress(|p| rustzip_core::extract(&arc, &dest, &x, p))?;
            println!("{} arquivos, {} em {:.2}s", r.files, human(r.bytes), r.elapsed_ms as f64 / 1000.0);
        }
        "l" => {
            need(1)?;
            let info = rustzip_core::list(Path::new(&rest[0]), x.password.as_deref())?;
            let vols = if info.volumes > 1 { format!(", {} volumes", info.volumes) } else { String::new() };
            println!("{}{}{vols}", info.format.label(), if info.solid { ", sólido" } else { "" });
            for e in &info.entries {
                let packed = e.packed.map(|p| p.to_string()).unwrap_or_default();
                let method = e.method.clone().unwrap_or_default();
                println!("{:>14} {packed:>14} {method:<18} {}{}", e.size, e.path, if e.is_dir { "/" } else { "" });
            }
        }
        "t" => {
            need(1)?;
            let r = with_progress(|p| rustzip_core::test(Path::new(&rest[0]), x.password.as_deref(), p))?;
            println!("Tudo certo: {} arquivos, {}", r.files, human(r.bytes));
        }
        "d" | "rn" => {
            need(2)?;
            let edit = if cmd == "d" {
                Edit { delete: rest[1..].to_vec(), ..Default::default() }
            } else {
                need(3)?;
                Edit { rename: vec![(rest[1].clone(), rest[2].clone())], ..Default::default() }
            };
            with_progress(|p| rustzip_core::modify(Path::new(&rest[0]), &edit, &c, p))?;
            println!("Feito.");
        }
        "c" => {
            need(2)?;
            c.format = Format::from_name(&rest[1]).unwrap_or(Format::SevenZ);
            let pw = x.password.clone();
            let r = with_progress(|p| rustzip_core::convert(Path::new(&rest[0]), Path::new(&rest[1]), pw.as_deref(), &c, p))?;
            println!("{} -> {}", human(r.input_bytes), human(r.output_bytes));
        }
        _ => return Err(Error::other(format!("comando desconhecido: {cmd}"))),
    }
    Ok(())
}

/// `e`: extrai tudo numa pasta só, sem subpastas.
fn extract_flat(arc: &Path, dest: &Path, opts: &ExtractOptions) -> rustzip_core::Result<()> {
    let tmp = std::env::temp_dir().join(format!("rz-e-{}", std::process::id()));
    with_progress(|p| rustzip_core::extract(arc, &tmp, opts, p))?;
    std::fs::create_dir_all(dest)?;
    let mut stack = vec![tmp.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)? {
            let e = e?;
            if e.file_type()?.is_dir() {
                stack.push(e.path());
            } else {
                std::fs::rename(e.path(), rustzip_core::fsutil::unique_name(&dest.join(e.file_name())))?;
            }
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

fn human(b: u64) -> String {
    match b {
        b if b >= 1 << 30 => format!("{:.2} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}
