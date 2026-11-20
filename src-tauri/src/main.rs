#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! RustZIP: janela do gerenciador (navegar em pastas e dentro de arquivos compactados) e janelas
//! de tarefa abertas pelo menu do Explorer (`--add`, `--extract-here`...). Uma segunda execução
//! só repassa os argumentos para a primeira, que abre outra janela.

mod fsview;
mod jobs;
mod settings;
mod shell;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use rustzip_core::{CreateOptions, ExtractOptions, Format};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// O que cada janela mostra; a interface pede com `task_get` ao montar.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
enum Task {
    #[serde(rename_all = "camelCase")]
    Browse { path: String },
    #[serde(rename_all = "camelCase")]
    Add { sources: Vec<String>, email: bool, suggested: String },
    #[serde(rename_all = "camelCase")]
    Extract { archive: String, dest: String },
    /// Operações já decididas (itens diretos do menu): a janela só mostra o progresso.
    #[serde(rename_all = "camelCase")]
    Run { title: String, jobs: Vec<serde_json::Value> },
}

#[derive(Default)]
struct Tasks {
    next: AtomicU64,
    by_window: Mutex<HashMap<String, Task>>,
}

fn open_window(app: &AppHandle, task: Task) {
    if let Task::Browse { path } = &task
        && let Some(main) = app.get_webview_window("main")
    {
        app.state::<Tasks>().by_window.lock().unwrap().insert("main".into(), task.clone());
        let _ = tauri::Emitter::emit_to(app, "main", "navigate", path.clone());
        let _ = main.unminimize();
        let _ = main.set_focus();
        return;
    }
    let (label, title, w, h) = match &task {
        Task::Browse { .. } => ("main".to_string(), "RustZIP".to_string(), 1120.0, 700.0),
        Task::Add { .. } => (next_label(app), "Nome e parâmetros do arquivo".to_string(), 680.0, 640.0),
        Task::Extract { .. } => (next_label(app), "Caminho e opções de extração".to_string(), 640.0, 540.0),
        // Cada cartão de operação tem ~175 px; mais o botão Fechar e as margens.
        Task::Run { title, jobs } => (next_label(app), title.clone(), 560.0, (95 + 178 * jobs.len().min(3)) as f64),
    };
    app.state::<Tasks>().by_window.lock().unwrap().insert(label.clone(), task.clone());
    let url = WebviewUrl::App(format!("index.html?w={label}").into());
    let mut b = WebviewWindowBuilder::new(app, &label, url)
        .title(title)
        .inner_size(w, h)
        .min_inner_size(if label == "main" { 760.0 } else { 480.0 }, if label == "main" { 460.0 } else { 220.0 })
        .center()
        .visible(false)
        .drag_and_drop(label == "main");
    if label != "main" {
        b = b.maximizable(false);
    }
    match b.build() {
        Ok(win) => {
            // Rede de segurança: se a interface não pedir para mostrar, mostra assim mesmo.
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(2500));
                let _ = win.show();
            });
        }
        Err(e) => eprintln!("janela: {e}"),
    }
}

fn next_label(app: &AppHandle) -> String {
    format!("task-{}", app.state::<Tasks>().next.fetch_add(1, Relaxed) + 1)
}

/// Expande `@lista.txt` (um caminho por linha, escrito pelo menu do Explorer quando a seleção é
/// grande demais para a linha de comando) e resolve caminhos relativos.
fn expand_args(args: &[String], cwd: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for a in args {
        if let Some(list) = a.strip_prefix('@') {
            let list = PathBuf::from(list);
            if let Ok(text) = std::fs::read_to_string(&list) {
                out.extend(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from));
                if list.parent().is_some_and(|p| p.ends_with("RustZIP")) {
                    let _ = std::fs::remove_file(&list);
                }
                continue;
            }
        }
        let p = Path::new(a);
        out.push(if p.is_absolute() || a.starts_with("--") { a.clone() } else { cwd.join(p).to_string_lossy().into_owned() });
    }
    out
}

/// Nome sugerido para compactar `sources`: o próprio nome (sem extensão) para um item só, o nome
/// da pasta onde estão para vários (como o WinRAR).
fn suggested_out(sources: &[String], format: Format) -> String {
    let first = Path::new(&sources[0]);
    let parent = first.parent().unwrap_or(Path::new("."));
    let stem = if sources.len() == 1 {
        let name = first.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if first.is_dir() { name } else { rustzip_core::fsutil::split_archive_name(&name).0.to_string() }
    } else {
        parent.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Arquivo".into())
    };
    let stem = if stem.is_empty() { "Arquivo".to_string() } else { stem };
    parent.join(format!("{stem}.{}", format.ext())).to_string_lossy().into_owned()
}

fn create_options(s: &settings::Settings, format: Format) -> CreateOptions {
    CreateOptions { format, level: s.default_level, zip_method: s.zip_method, solid: s.solid, smart: s.smart, ..Default::default() }
}

fn handle_args(app: &AppHandle, argv: &[String], cwd: &Path) {
    let args = expand_args(argv.get(1..).unwrap_or_default(), cwd);
    let s = settings::load();
    let (cmd, rest) = match args.split_first() {
        Some((c, r)) if c.starts_with("--") => (c.as_str(), r.to_vec()),
        _ => ("", args.clone()),
    };
    let task = match cmd {
        "--add" | "--add-email" if !rest.is_empty() => {
            Task::Add { suggested: suggested_out(&rest, s.default_format), sources: rest, email: cmd == "--add-email" }
        }
        "--add-quick" | "--add-quick-email" if !rest.is_empty() => {
            let out = suggested_out(&rest, s.default_format);
            let name = Path::new(&out).file_name().unwrap_or_default().to_string_lossy().into_owned();
            Task::Run {
                title: format!("Compactando {name}"),
                jobs: vec![json!({
                    "kind": "create",
                    "sources": rest,
                    "out": out,
                    "options": create_options(&s, s.default_format),
                    "email": cmd == "--add-quick-email",
                })],
            }
        }
        "--extract" if !rest.is_empty() => {
            let dest = rustzip_core::extract::default_folder(Path::new(&rest[0]));
            Task::Extract { archive: rest[0].clone(), dest: dest.to_string_lossy().into_owned() }
        }
        "--extract-here" | "--extract-folder" if !rest.is_empty() => Task::Run {
            title: "Extraindo".into(),
            jobs: rest
                .iter()
                .map(|a| {
                    let here = cmd == "--extract-here";
                    let dest = if here {
                        Path::new(a).parent().unwrap_or(Path::new(".")).to_path_buf()
                    } else {
                        rustzip_core::extract::default_folder(Path::new(a))
                    };
                    json!({
                        "kind": "extract",
                        "archive": a,
                        "dest": dest,
                        "options": ExtractOptions::default_json(),
                        "smartHere": here && s.smart_extract_here,
                        "openAfter": s.open_folder_after_extract,
                    })
                })
                .collect(),
        },
        "--test" if !rest.is_empty() => Task::Run {
            title: "Testando".into(),
            jobs: rest.iter().map(|a| json!({ "kind": "test", "archive": a, "password": null })).collect(),
        },
        _ => {
            let path = rest.first().cloned().unwrap_or_else(|| fsview::home().to_string_lossy().into_owned());
            Task::Browse { path }
        }
    };
    open_window(app, task);
}

trait DefaultJson {
    fn default_json() -> serde_json::Value;
}

impl DefaultJson for ExtractOptions {
    fn default_json() -> serde_json::Value {
        json!({ "overwrite": "overwrite", "selection": [], "threads": 0 })
    }
}

// ------------------------------------------------------------------ comandos

type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CmdError {
    code: String,
    message: String,
}

fn err(e: rustzip_core::Error) -> String {
    serde_json::to_string(&CmdError { code: e.code().into(), message: e.to_string() }).unwrap()
}

#[tauri::command]
fn task_get(window: tauri::WebviewWindow, tasks: tauri::State<Tasks>) -> Option<Task> {
    tasks.by_window.lock().unwrap().get(window.label()).cloned()
}

#[tauri::command]
fn job_start(app: AppHandle, window: tauri::WebviewWindow, jobs: tauri::State<jobs::Jobs>, spec: jobs::JobSpec) -> u64 {
    jobs.start(&app, window.label().to_string(), spec)
}

#[tauri::command]
fn job_pause(jobs: tauri::State<jobs::Jobs>, id: u64, paused: bool) {
    jobs::pause(&jobs, id, paused);
}

#[tauri::command]
fn job_cancel(jobs: tauri::State<jobs::Jobs>, id: u64) {
    jobs::cancel(&jobs, id);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DirListing {
    path: String,
    parent: Option<String>,
    entries: Vec<fsview::FsEntry>,
}

#[tauri::command]
async fn list_dir(path: String) -> CmdResult<DirListing> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = PathBuf::from(&path);
        let entries = fsview::list_dir(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let mut s = settings::load();
        s.last_folder = Some(path.clone());
        let _ = settings::save(&s);
        Ok(DirListing { parent: p.parent().map(|x| x.to_string_lossy().into_owned()), path, entries })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn places() -> Vec<fsview::Place> {
    fsview::places()
}

#[tauri::command]
async fn type_infos(keys: Vec<String>) -> HashMap<String, fsview::TypeInfo> {
    tauri::async_runtime::spawn_blocking(move || fsview::type_infos(&keys)).await.unwrap_or_default()
}

#[tauri::command]
async fn archive_info(path: String, password: Option<String>) -> CmdResult<rustzip_core::ArchiveInfo> {
    tauri::async_runtime::spawn_blocking(move || rustzip_core::list(Path::new(&path), password.as_deref()).map_err(err))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn detect(path: String) -> Option<Format> {
    let p = Path::new(&path);
    if p.is_file() && Format::from_name(&path).is_some() { Format::detect(p).ok() } else { None }
}

#[tauri::command]
fn settings_get() -> settings::Settings {
    settings::load()
}

#[tauri::command]
fn settings_set(value: settings::Settings) -> CmdResult<()> {
    settings::save(&value).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_path(path: String) {
    shell::open(Path::new(&path));
}

#[tauri::command]
fn reveal_path(path: String) {
    shell::reveal(Path::new(&path));
}

#[tauri::command]
fn suggest_out(sources: Vec<String>, format: Format) -> String {
    suggested_out(&sources, format)
}

#[tauri::command]
fn default_extract_dir(archive: String) -> String {
    rustzip_core::extract::default_folder(Path::new(&archive)).to_string_lossy().into_owned()
}

/// Extrai uma entrada para uma pasta temporária e abre com o programa padrão (duplo clique
/// dentro do arquivo compactado, como o WinRAR).
#[tauri::command]
async fn preview_entry(archive: String, entry: String, password: Option<String>) -> CmdResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let dir = std::env::temp_dir().join("RustZIP").join(format!("ver-{stamp}"));
        let parent = entry.rsplit_once('/').map(|(d, _)| d.to_string());
        let opts = ExtractOptions { password, selection: vec![entry.clone()], strip_prefix: parent, ..Default::default() };
        rustzip_core::extract(Path::new(&archive), &dir, &opts, &rustzip_core::Progress::default()).map_err(err)?;
        let file = dir.join(entry.rsplit('/').next().unwrap_or(&entry));
        shell::open(&file);
        Ok(file.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn integration_status() -> shell::Integration {
    tauri::async_runtime::spawn_blocking(shell::status).await.unwrap()
}

#[tauri::command]
async fn integration_set(classic: Option<bool>, modern: Option<bool>) -> CmdResult<shell::Integration> {
    tauri::async_runtime::spawn_blocking(move || {
        match classic {
            Some(true) => shell::install_classic()?,
            Some(false) => shell::uninstall_classic(),
            None => {}
        }
        match modern {
            Some(true) => shell::install_modern()?,
            Some(false) => shell::uninstall_modern()?,
            None => {}
        }
        Ok(shell::status())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Estimate {
    format: Format,
    level: u32,
    label: String,
    ratio: f64,
    mb_per_s: f64,
    estimated_bytes: u64,
    estimated_ms: u64,
}

/// "Analisar": compacta uma amostra dos arquivos (até ~48 MB) em vários formatos e estima
/// tamanho e tempo do conjunto inteiro, para escolher o melhor formato antes de compactar.
#[tauri::command]
async fn estimate_formats(sources: Vec<String>) -> CmdResult<Vec<Estimate>> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut files = Vec::new();
        let mut total = 0u64;
        for s in &sources {
            for e in walk(Path::new(s)) {
                total += e.1;
                files.push(e);
            }
        }
        if total == 0 {
            return Ok(Vec::new());
        }
        // Amostra espalhada: um arquivo a cada `step`, até o limite.
        const LIMIT: u64 = 48 << 20;
        let step = (total / LIMIT).max(1) as usize;
        let mut sample = Vec::new();
        let mut bytes = 0;
        for (i, (p, size)) in files.iter().enumerate() {
            if (i % step == 0 || files.len() < 64) && bytes < LIMIT {
                sample.push(p.clone());
                bytes += size;
            }
        }
        let tmp = std::env::temp_dir().join("RustZIP").join(format!("estimativa-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let candidates: [(Format, u32, &str); 6] = [
            (Format::Zip, 5, "ZIP · Normal (compatível)"),
            (Format::TarZst, 3, "TAR.ZST · Rápido"),
            (Format::TarZst, 7, "TAR.ZST · Bom"),
            (Format::SevenZ, 1, "7Z · Mais rápido"),
            (Format::SevenZ, 5, "7Z · Normal"),
            (Format::SevenZ, 9, "7Z · Melhor"),
        ];
        let mut out = Vec::new();
        for (format, level, label) in candidates {
            let dest = tmp.join(format!("a.{}", format.ext()));
            let opts = CreateOptions { format, level, ..Default::default() };
            let t = std::time::Instant::now();
            let r = rustzip_core::create(&sample, &dest, &opts, &rustzip_core::Progress::default()).map_err(err)?;
            let ms = t.elapsed().as_millis().max(1) as u64;
            let ratio = r.output_bytes as f64 / r.input_bytes.max(1) as f64;
            let speed = r.input_bytes as f64 / 1048576.0 / (ms as f64 / 1000.0);
            out.push(Estimate {
                format,
                level,
                label: label.into(),
                ratio,
                mb_per_s: speed,
                estimated_bytes: (total as f64 * ratio) as u64,
                estimated_ms: (ms as f64 * total as f64 / r.input_bytes.max(1) as f64) as u64,
            });
            let _ = std::fs::remove_file(&dest);
        }
        let _ = std::fs::remove_dir_all(&tmp);
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn walk(p: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![p.to_path_buf()];
    while let Some(x) = stack.pop() {
        let Ok(meta) = std::fs::symlink_metadata(&x) else { continue };
        if meta.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&x) {
                stack.extend(rd.flatten().map(|e| e.path()));
            }
        } else if meta.is_file() {
            out.push((x, meta.len()));
        }
    }
    out
}

/// `RustZIP.exe --register [--modern]` / `--unregister`: integração com o Explorer sem abrir
/// janela (para instalar por script). Resultado no código de saída e em `%APPDATA%\RustZIP\registro.log`.
#[cfg(windows)]
fn register_from_args() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (register, unregister) = (args.iter().any(|a| a == "--register"), args.iter().any(|a| a == "--unregister"));
    if !register && !unregister {
        return None;
    }
    let modern = args.iter().any(|a| a == "--modern");
    let result = if register {
        // Pacote primeiro: com ele instalado, o registro clássico não duplica o submenu.
        (if modern { shell::install_modern() } else { Ok(()) }).and_then(|_| shell::install_classic())
    } else {
        shell::uninstall_classic();
        shell::uninstall_modern()
    };
    let _ = std::fs::create_dir_all(settings::dir());
    let _ = std::fs::write(settings::dir().join("registro.log"), format!("{result:?}\n{:?}\n", shell::status()));
    Some(if result.is_ok() { 0 } else { 1 })
}

fn main() {
    #[cfg(windows)]
    if let Some(code) = register_from_args() {
        std::process::exit(code);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            handle_args(app, &argv, Path::new(&cwd));
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(Tasks::default())
        .manage(jobs::Jobs::default())
        .setup(|app| {
            let argv: Vec<String> = std::env::args().collect();
            let cwd = std::env::current_dir().unwrap_or_default();
            handle_args(app.handle(), &argv, &cwd);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            task_get,
            job_start,
            job_pause,
            job_cancel,
            list_dir,
            places,
            type_infos,
            archive_info,
            detect,
            settings_get,
            settings_set,
            open_path,
            reveal_path,
            suggest_out,
            default_extract_dir,
            preview_entry,
            integration_status,
            integration_set,
            estimate_formats,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o RustZIP");
}
