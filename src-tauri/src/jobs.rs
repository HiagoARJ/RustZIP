//! Operações longas numa thread própria, com progresso em eventos para a janela que pediu.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustzip_core::{CreateOptions, Edit, ExtractOptions, Progress};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JobSpec {
    #[serde(rename_all = "camelCase")]
    Create {
        sources: Vec<PathBuf>,
        out: PathBuf,
        options: CreateOptions,
        #[serde(default)]
        email: bool,
    },
    #[serde(rename_all = "camelCase")]
    Extract {
        archive: PathBuf,
        dest: PathBuf,
        options: ExtractOptions,
        /// Extrair na pasta do arquivo, criando uma subpasta só se a raiz tiver vários itens.
        #[serde(default)]
        smart_here: bool,
        #[serde(default)]
        open_after: bool,
    },
    #[serde(rename_all = "camelCase")]
    Test { archive: PathBuf, password: Option<String> },
    #[serde(rename_all = "camelCase")]
    Modify { archive: PathBuf, edit: Edit, options: CreateOptions },
    #[serde(rename_all = "camelCase")]
    Convert { archive: PathBuf, out: PathBuf, password: Option<String>, options: CreateOptions },
}

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    running: Mutex<HashMap<u64, Arc<Progress>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    id: u64,
    snap: rustzip_core::Snapshot,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    id: u64,
    ok: bool,
    result: Option<serde_json::Value>,
    code: Option<String>,
    message: Option<String>,
    snap: rustzip_core::Snapshot,
}

impl Jobs {
    pub fn start(&self, app: &AppHandle, window: String, spec: JobSpec) -> u64 {
        let id = self.next.fetch_add(1, Relaxed) + 1;
        let progress = Progress::new();
        self.running.lock().unwrap().insert(id, progress.clone());

        let (app2, win2, p2) = (app.clone(), window.clone(), progress.clone());
        let ticker_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let td = ticker_done.clone();
        std::thread::spawn(move || {
            while !td.load(Relaxed) {
                let _ = app2.emit_to(&win2, "job-progress", ProgressEvent { id, snap: p2.snapshot() });
                std::thread::sleep(Duration::from_millis(200));
            }
        });

        let app = app.clone();
        std::thread::spawn(move || {
            let outcome = run(&spec, &progress);
            ticker_done.store(true, Relaxed);
            let state = app.state::<Jobs>();
            state.running.lock().unwrap().remove(&id);
            let snap = progress.snapshot();
            let ev = match outcome {
                Ok(v) => DoneEvent { id, ok: true, result: Some(v), code: None, message: None, snap },
                Err(e) => DoneEvent { id, ok: false, result: None, code: Some(e.code().into()), message: Some(e.to_string()), snap },
            };
            let _ = app.emit_to(&window, "job-done", ev);
        });
        id
    }

    pub fn get(&self, id: u64) -> Option<Arc<Progress>> {
        self.running.lock().unwrap().get(&id).cloned()
    }
}


fn run(spec: &JobSpec, p: &Progress) -> rustzip_core::Result<serde_json::Value> {
    match spec {
        JobSpec::Create { sources, out, options, email } => {
            let r = rustzip_core::create(sources, out, options, p)?;
            if *email {
                let files: Vec<PathBuf> = r.outputs.iter().map(PathBuf::from).collect();
                if let Err(e) = crate::shell::send_mail(&files) {
                    p.warn(format!("E-mail: {e}"));
                    crate::shell::reveal(&files[0]);
                }
            }
            Ok(json(&r))
        }
        JobSpec::Extract { archive, dest, options, smart_here, open_after } => {
            let dest = if *smart_here { smart_dest(archive, options.password.as_deref())? } else { dest.clone() };
            let r = rustzip_core::extract(archive, &dest, options, p)?;
            if *open_after {
                crate::shell::open(&dest);
            }
            let mut v = json(&r);
            v["dest"] = serde_json::Value::String(dest.to_string_lossy().into_owned());
            Ok(v)
        }
        JobSpec::Test { archive, password } => Ok(json(&rustzip_core::test(archive, password.as_deref(), p)?)),
        JobSpec::Modify { archive, edit, options } => Ok(json(&rustzip_core::modify(archive, edit, options, p)?)),
        JobSpec::Convert { archive, out, password, options } => {
            Ok(json(&rustzip_core::convert(archive, out, password.as_deref(), options, p)?))
        }
    }
}

/// "Extrair aqui" sem bagunçar a pasta: se a raiz do arquivo tem um item só, extrai ao lado;
/// se tem vários, cria a pasta com o nome do arquivo.
pub fn smart_dest(archive: &Path, password: Option<&str>) -> rustzip_core::Result<PathBuf> {
    let parent = archive.parent().unwrap_or(Path::new(".")).to_path_buf();
    let info = rustzip_core::list(archive, password)?;
    let mut roots = std::collections::HashSet::new();
    for e in &info.entries {
        roots.insert(e.path.split('/').next().unwrap_or("").to_lowercase());
        if roots.len() > 1 {
            return Ok(rustzip_core::extract::default_folder(archive));
        }
    }
    Ok(parent)
}

pub fn pause(jobs: &Jobs, id: u64, paused: bool) {
    if let Some(p) = jobs.get(id) {
        p.set_paused(paused);
    }
}

pub fn cancel(jobs: &Jobs, id: u64) {
    if let Some(p) = jobs.get(id) {
        p.cancel();
    }
}

fn json<T: Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or(serde_json::Value::Null)
}
