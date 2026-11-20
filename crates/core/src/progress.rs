use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{Error, Result, cancel_io};

/// Estado compartilhado de uma operação longa: lido pela interface, alterado pelos workers.
pub struct Progress {
    /// Bytes descompactados a processar (entrada na compressão, saída na extração).
    pub total: AtomicU64,
    pub done: AtomicU64,
    /// Bytes gravados no arquivo compactado (para a taxa de compressão ao vivo).
    pub packed: AtomicU64,
    pub files_total: AtomicU64,
    pub files_done: AtomicU64,
    current: Mutex<String>,
    cancel: AtomicBool,
    paused: AtomicBool,
    warnings: Mutex<Vec<String>>,
    started: Instant,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub total: u64,
    pub done: u64,
    pub packed: u64,
    pub files_total: u64,
    pub files_done: u64,
    pub current: String,
    pub paused: bool,
    pub elapsed_ms: u64,
    pub warnings: Vec<String>,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            total: AtomicU64::new(0),
            done: AtomicU64::new(0),
            packed: AtomicU64::new(0),
            files_total: AtomicU64::new(0),
            files_done: AtomicU64::new(0),
            current: Mutex::new(String::new()),
            cancel: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            warnings: Mutex::new(Vec::new()),
            started: Instant::now(),
        }
    }
}

impl Progress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_current(&self, name: &str) {
        let mut c = self.current.lock().unwrap();
        c.clear();
        c.push_str(name);
    }

    pub fn add(&self, n: u64) {
        self.done.fetch_add(n, Relaxed);
    }

    pub fn file_done(&self) {
        self.files_done.fetch_add(1, Relaxed);
    }

    pub fn warn(&self, msg: impl Into<String>) {
        self.warnings.lock().unwrap().push(msg.into());
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Relaxed);
        self.paused.store(false, Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Relaxed);
    }

    /// Ponto de parada: segura enquanto pausado e falha se cancelado.
    pub fn checkpoint(&self) -> Result<()> {
        while self.paused.load(Relaxed) && !self.is_cancelled() {
            std::thread::sleep(Duration::from_millis(80));
        }
        if self.is_cancelled() { Err(Error::Cancelled) } else { Ok(()) }
    }

    fn io_checkpoint(&self) -> io::Result<()> {
        self.checkpoint().map_err(|_| cancel_io())
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            total: self.total.load(Relaxed),
            done: self.done.load(Relaxed),
            packed: self.packed.load(Relaxed),
            files_total: self.files_total.load(Relaxed),
            files_done: self.files_done.load(Relaxed),
            current: self.current.lock().unwrap().clone(),
            paused: self.paused.load(Relaxed),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            warnings: self.warnings.lock().unwrap().clone(),
        }
    }

    pub fn warnings(&self) -> Vec<String> {
        self.warnings.lock().unwrap().clone()
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
}

/// Leitor que conta os bytes lidos em `done` e obedece pausa/cancelamento.
pub struct CountingReader<'a, R> {
    inner: R,
    progress: &'a Progress,
}

impl<'a, R> CountingReader<'a, R> {
    pub fn new(inner: R, progress: &'a Progress) -> Self {
        Self { inner, progress }
    }
}

impl<R: Read> Read for CountingReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.progress.io_checkpoint()?;
        let n = self.inner.read(buf)?;
        self.progress.add(n as u64);
        Ok(n)
    }
}

/// Versão dona do `Arc`, para leitores que precisam ser `'static`.
pub struct ArcCountingReader<R> {
    inner: R,
    progress: Arc<Progress>,
}

impl<R> ArcCountingReader<R> {
    pub fn new(inner: R, progress: Arc<Progress>) -> Self {
        Self { inner, progress }
    }
}

impl<R: Read> Read for ArcCountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.progress.io_checkpoint()?;
        let n = self.inner.read(buf)?;
        self.progress.add(n as u64);
        Ok(n)
    }
}

/// Escritor que soma o que passa em `packed` (tamanho do arquivo compactado até agora).
pub struct PackedWriter<'a, W> {
    inner: W,
    progress: &'a Progress,
}

impl<'a, W> PackedWriter<'a, W> {
    pub fn new(inner: W, progress: &'a Progress) -> Self {
        Self { inner, progress }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for PackedWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.progress.packed.fetch_add(n as u64, Relaxed);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: io::Seek> io::Seek for PackedWriter<'_, W> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}
