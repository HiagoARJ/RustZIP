use std::fs;
use std::path::{Path, PathBuf};

use rustzip_core::{CreateOptions, Error, ExtractOptions, Format, Overwrite, Progress, ZipMethod, create, extract, list, test};

/// Árvore com texto, binário aleatório (incompressível), "executável", vazio e pasta vazia.
fn sample_tree(root: &Path) -> PathBuf {
    let src = root.join("Projeto Ação");
    fs::create_dir_all(src.join("docs/sub")).unwrap();
    fs::create_dir_all(src.join("vazia")).unwrap();
    fs::write(src.join("leia-me.txt"), "olá mundo\n".repeat(5000)).unwrap();
    fs::write(src.join("docs/sub/dados.csv"), (0..20000).map(|i| format!("{i};{}\n", i * 3)).collect::<String>()).unwrap();
    let mut x: u64 = 0x2545F4914F6CDD1D;
    let rnd: Vec<u8> = (0..400_000)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect();
    fs::write(src.join("docs/aleatorio.bin"), &rnd).unwrap();
    fs::write(src.join("foto.jpg"), &rnd[..50_000]).unwrap();
    let exe: Vec<u8> = (0..200_000u32).flat_map(|i| [0xE8, (i % 256) as u8, 0, 0, 0, 0x90, 0x90]).collect();
    fs::write(src.join("app.exe"), exe).unwrap();
    fs::write(src.join("vazio.txt"), "").unwrap();
    src
}

fn assert_same(a: &Path, b: &Path) {
    for e in walkdir::WalkDir::new(a) {
        let e = e.unwrap();
        let rel = e.path().strip_prefix(a).unwrap();
        let other = b.join(rel);
        assert!(other.exists(), "falta {}", other.display());
        if e.file_type().is_file() {
            assert_eq!(fs::read(e.path()).unwrap(), fs::read(&other).unwrap(), "conteúdo de {}", rel.display());
        }
    }
}

fn roundtrip(format: Format, tweak: impl Fn(&mut CreateOptions)) {
    let dir = tempfile::tempdir().unwrap();
    let src = sample_tree(dir.path());
    let out = dir.path().join(format!("saida.{}", format.ext()));
    let mut opts = CreateOptions { format, level: 5, ..Default::default() };
    tweak(&mut opts);
    let p = Progress::new();
    let report = create(&[src.clone()], &out, &opts, &p).unwrap_or_else(|e| panic!("{format:?}: {e}"));
    assert!(report.output_bytes > 0);
    assert_eq!(Format::detect(&out).unwrap(), format);

    let info = list(&out, opts.password.as_deref()).unwrap();
    assert!(info.entries.iter().any(|e| e.path == "Projeto Ação/docs/sub/dados.csv"), "{format:?}: {:?}", info.entries.iter().map(|e| &e.path).collect::<Vec<_>>());

    test(&out, opts.password.as_deref(), &Progress::new()).unwrap();

    let dest = dir.path().join("x");
    let eo = ExtractOptions { password: opts.password.clone(), ..Default::default() };
    extract(&out, &dest, &eo, &Progress::new()).unwrap();
    assert_same(&src, &dest.join("Projeto Ação"));
}

#[test]
fn zip_all_methods() {
    for m in [ZipMethod::Deflate, ZipMethod::Zstd, ZipMethod::Bzip2, ZipMethod::Xz] {
        roundtrip(Format::Zip, |o| o.zip_method = m);
    }
    roundtrip(Format::Zip, |o| {
        o.zip_method = ZipMethod::Zopfli;
        o.level = 1;
    });
    roundtrip(Format::Zip, |o| o.level = 0);
}

#[test]
fn zip_password() {
    roundtrip(Format::Zip, |o| o.password = Some("s3nh@".into()));
}

#[test]
fn seven_z_variants() {
    roundtrip(Format::SevenZ, |_| {});
    roundtrip(Format::SevenZ, |o| o.solid = false);
    roundtrip(Format::SevenZ, |o| {
        o.level = 9;
        o.threads = 1;
    });
    roundtrip(Format::SevenZ, |o| o.smart = false);
    roundtrip(Format::SevenZ, |o| o.level = 0);
}

#[test]
fn seven_z_password_and_names() {
    roundtrip(Format::SevenZ, |o| {
        o.password = Some("abc".into());
        o.encrypt_names = true;
    });
}

#[test]
fn tar_family() {
    for f in [Format::Tar, Format::TarGz, Format::TarXz, Format::TarZst, Format::TarBz2, Format::TarLz4, Format::TarBr] {
        roundtrip(f, |_| {});
    }
    roundtrip(Format::TarZst, |o| o.level = 9);
}

#[test]
fn single_streams() {
    for f in [Format::Gz, Format::Xz, Format::Zst, Format::Bz2, Format::Lz4, Format::Br] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("log.txt");
        fs::write(&file, "linha de log repetida\n".repeat(30000)).unwrap();
        let out = dir.path().join(format!("log.txt.{}", f.ext()));
        create(&[file.clone()], &out, &CreateOptions { format: f, ..Default::default() }, &Progress::new()).unwrap();
        assert_eq!(Format::detect(&out).unwrap(), f);
        let info = list(&out, None).unwrap();
        assert_eq!(info.entries[0].path, "log.txt");
        assert_eq!(info.entries[0].size, fs::metadata(&file).unwrap().len());
        let dest = dir.path().join("x");
        extract(&out, &dest, &ExtractOptions::default(), &Progress::new()).unwrap();
        assert_eq!(fs::read(dest.join("log.txt")).unwrap(), fs::read(&file).unwrap());
    }
}

#[test]
fn wrong_and_missing_password() {
    let dir = tempfile::tempdir().unwrap();
    let src = sample_tree(dir.path());
    for f in [Format::Zip, Format::SevenZ] {
        let out = dir.path().join(format!("p.{}", f.ext()));
        create(&[src.clone()], &out, &CreateOptions { format: f, password: Some("certa".into()), ..Default::default() }, &Progress::new()).unwrap();
        let none = extract(&out, &dir.path().join("n"), &ExtractOptions::default(), &Progress::new()).unwrap_err();
        assert!(matches!(none, Error::PasswordRequired | Error::WrongPassword), "{f:?}: {none:?}");
        let wrong = ExtractOptions { password: Some("errada".into()), ..Default::default() };
        let e = extract(&out, &dir.path().join("w"), &wrong, &Progress::new()).unwrap_err();
        assert!(matches!(e, Error::WrongPassword | Error::Corrupt(_)), "{f:?}: {e:?}");
    }
}

#[test]
fn selection_strip_and_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let src = sample_tree(dir.path());
    let out = dir.path().join("s.7z");
    create(&[src.clone()], &out, &CreateOptions::default(), &Progress::new()).unwrap();
    let dest = dir.path().join("sel");
    let eo = ExtractOptions {
        selection: vec!["Projeto Ação/docs".into()],
        strip_prefix: Some("Projeto Ação".into()),
        ..Default::default()
    };
    extract(&out, &dest, &eo, &Progress::new()).unwrap();
    assert!(dest.join("docs/sub/dados.csv").exists());
    assert!(!dest.join("leia-me.txt").exists());

    fs::write(dest.join("docs/sub/dados.csv"), "meu").unwrap();
    let skip = ExtractOptions { overwrite: Overwrite::Skip, ..eo.clone() };
    extract(&out, &dest, &skip, &Progress::new()).unwrap();
    assert_eq!(fs::read_to_string(dest.join("docs/sub/dados.csv")).unwrap(), "meu");
    let rename = ExtractOptions { overwrite: Overwrite::Rename, ..eo };
    extract(&out, &dest, &rename, &Progress::new()).unwrap();
    assert!(dest.join("docs/sub/dados (2).csv").exists());
}

#[test]
fn volumes_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let src = sample_tree(dir.path());
    let out = dir.path().join("v.7z");
    let r = create(&[src.clone()], &out, &CreateOptions { volume_size: 64 * 1024, smart: false, level: 1, ..Default::default() }, &Progress::new()).unwrap();
    assert!(r.outputs.len() > 2, "{:?}", r.outputs);
    let first = PathBuf::from(&r.outputs[0]);
    assert_eq!(Format::detect(&first).unwrap(), Format::SevenZ);
    let dest = dir.path().join("x");
    extract(&first, &dest, &ExtractOptions::default(), &Progress::new()).unwrap();
    assert_same(&src, &dest.join("Projeto Ação"));
}

#[test]
fn cancel_stops_and_cleans() {
    let dir = tempfile::tempdir().unwrap();
    let src = sample_tree(dir.path());
    let out = dir.path().join("c.zip");
    let p = Progress::new();
    p.cancel();
    let e = create(&[src], &out, &CreateOptions { format: Format::Zip, ..Default::default() }, &p).unwrap_err();
    assert!(matches!(e, Error::Cancelled));
    assert!(!out.exists());
    assert!(fs::read_dir(dir.path()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".rustzip-")));
}

/// RAR (só leitura): gera o arquivo com o Rar.exe do WinRAR instalado, se existir.
#[test]
fn rar_extract() {
    let rar = Path::new(r"C:\Program Files\WinRAR\Rar.exe");
    if !rar.exists() {
        eprintln!("Rar.exe ausente; teste pulado");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = sample_tree(dir.path());
    for (name, extra) in [("a.rar", vec![]), ("p.rar", vec!["-psenha", "-hp"]), ("v.rar", vec!["-v100k", "-m5", "-s"])] {
        let out = dir.path().join(name);
        let st = std::process::Command::new(rar)
            .arg("a")
            .args(["-r", "-ep1", "-idq", "-ma5"])
            .args(&extra)
            .arg(&out)
            .arg(&src)
            .status()
            .unwrap();
        assert!(st.success());
        let first = if name == "v.rar" { dir.path().join("v.part01.rar") } else { out.clone() };
        let pw = extra.iter().find_map(|a| a.strip_prefix("-p")).map(str::to_string);
        let info = list(&first, pw.as_deref()).unwrap_or_else(|e| {
            let names: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
            panic!("{name}: {e:?} em {names:?}")
        });
        assert_eq!(info.format, Format::Rar);
        assert!(info.entries.iter().any(|e| e.path.ends_with("docs/sub/dados.csv")));
        test(&first, pw.as_deref(), &Progress::new()).unwrap();
        let dest = dir.path().join(format!("x-{name}"));
        extract(&first, &dest, &ExtractOptions { password: pw, ..Default::default() }, &Progress::new()).unwrap();
        assert_same(&src, &dest.join("Projeto Ação"));
    }
}

fn names(path: &Path, pw: Option<&str>) -> Vec<String> {
    let mut v: Vec<String> = list(path, pw).unwrap().entries.into_iter().filter(|e| !e.is_dir).map(|e| e.path).collect();
    v.sort();
    v
}

#[test]
fn modify_add_delete_rename() {
    for f in [Format::Zip, Format::SevenZ, Format::TarZst] {
        let dir = tempfile::tempdir().unwrap();
        let src = sample_tree(dir.path());
        let out = dir.path().join(format!("m.{}", f.ext()));
        create(&[src.clone()], &out, &CreateOptions { format: f, ..Default::default() }, &Progress::new()).unwrap();

        let extra = dir.path().join("novo.txt");
        fs::write(&extra, "conteúdo novo").unwrap();
        let edit = rustzip_core::Edit {
            delete: vec!["Projeto Ação/docs/aleatorio.bin".into()],
            rename: vec![("Projeto Ação/docs/sub".into(), "Projeto Ação/docs/renomeada".into())],
            add: vec![extra.clone()],
            add_to: "Projeto Ação/docs".into(),
        };
        rustzip_core::modify(&out, &edit, &CreateOptions::default(), &Progress::new()).unwrap_or_else(|e| panic!("{f:?}: {e}"));
        let n = names(&out, None);
        assert!(n.contains(&"Projeto Ação/docs/novo.txt".to_string()), "{f:?} {n:?}");
        assert!(n.contains(&"Projeto Ação/docs/renomeada/dados.csv".to_string()), "{f:?} {n:?}");
        assert!(!n.iter().any(|x| x.ends_with("aleatorio.bin")), "{f:?} {n:?}");
        assert!(n.contains(&"Projeto Ação/app.exe".to_string()));
        let dest = dir.path().join("x");
        extract(&out, &dest, &ExtractOptions::default(), &Progress::new()).unwrap();
        assert_eq!(fs::read_to_string(dest.join("Projeto Ação/docs/novo.txt")).unwrap(), "conteúdo novo");
        assert_eq!(fs::read(dest.join("Projeto Ação/docs/renomeada/dados.csv")).unwrap(), fs::read(src.join("docs/sub/dados.csv")).unwrap());
        assert_eq!(fs::read(dest.join("Projeto Ação/app.exe")).unwrap(), fs::read(src.join("app.exe")).unwrap());
    }
}

#[test]
fn convert_rar_to_7z() {
    let rar = Path::new(r"C:\Program Files\WinRAR\Rar.exe");
    if !rar.exists() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = sample_tree(dir.path());
    let r = dir.path().join("c.rar");
    assert!(std::process::Command::new(rar).args(["a", "-r", "-ep1", "-idq"]).arg(&r).arg(&src).status().unwrap().success());
    let out = dir.path().join("c.7z");
    rustzip_core::convert(&r, &out, None, &CreateOptions::default(), &Progress::new()).unwrap();
    let dest = dir.path().join("x");
    extract(&out, &dest, &ExtractOptions::default(), &Progress::new()).unwrap();
    assert_same(&src, &dest.join("Projeto Ação"));
}
