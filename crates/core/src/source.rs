//! Leitura de arquivos compactados por posição, com volumes `.001/.002...` vistos como um só
//! fluxo. Um [`Source`] clonado compartilha os handles e tem cursor e buffer próprios, o que deixa
//! várias threads extraírem do mesmo arquivo sem reabrir nem reler o diretório central.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// O buffer começa pequeno (uma entrada de ZIP de 2 KB não deve puxar 256 KB do disco) e dobra
/// enquanto a leitura for sequencial.
const BUF_MIN: usize = 16 * 1024;
const BUF_MAX: usize = 1024 * 1024;

struct Part {
    file: File,
    start: u64,
    len: u64,
}

pub struct Source {
    parts: Arc<Vec<Part>>,
    len: u64,
    pos: u64,
    buf: Vec<u8>,
    buf_start: u64,
    buf_cap: usize,
}

impl Source {
    pub fn open(path: &Path) -> io::Result<Self> {
        let paths = volume_parts(path);
        let mut parts = Vec::with_capacity(paths.len());
        let mut start = 0;
        for p in paths {
            let file = File::open(&p)?;
            let len = file.metadata()?.len();
            parts.push(Part { file, start, len });
            start += len;
        }
        Ok(Self { parts: Arc::new(parts), len: start, pos: 0, buf: Vec::new(), buf_start: 0, buf_cap: BUF_MIN })
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn volumes(&self) -> usize {
        self.parts.len()
    }

    fn read_at(&self, offset: u64, out: &mut [u8]) -> io::Result<usize> {
        if offset >= self.len || out.is_empty() {
            return Ok(0);
        }
        let idx = match self.parts.binary_search_by(|p| p.start.cmp(&offset)) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let part = &self.parts[idx];
        let local = offset - part.start;
        let want = out.len().min((part.len - local) as usize);
        positional_read(&part.file, &mut out[..want], local)
    }
}

impl Clone for Source {
    fn clone(&self) -> Self {
        Self { parts: self.parts.clone(), len: self.len, pos: self.pos, buf: Vec::new(), buf_start: 0, buf_cap: BUF_MIN }
    }
}

impl Read for Source {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let buf_end = self.buf_start + self.buf.len() as u64;
        if self.pos >= self.buf_start && self.pos < buf_end {
            let from = (self.pos - self.buf_start) as usize;
            let n = out.len().min(self.buf.len() - from);
            out[..n].copy_from_slice(&self.buf[from..from + n]);
            self.pos += n as u64;
            return Ok(n);
        }
        if out.len() >= self.buf_cap {
            let n = self.read_at(self.pos, out)?;
            self.pos += n as u64;
            return Ok(n);
        }
        let sequential = !self.buf.is_empty() && self.pos == buf_end;
        self.buf_cap = if sequential { (self.buf_cap * 2).min(BUF_MAX) } else { BUF_MIN };
        let mut buf = std::mem::take(&mut self.buf);
        buf.resize(self.buf_cap, 0);
        let n = self.read_at(self.pos, &mut buf)?;
        buf.truncate(n);
        self.buf = buf;
        self.buf_start = self.pos;
        if n == 0 {
            return Ok(0);
        }
        let k = out.len().min(n);
        out[..k].copy_from_slice(&self.buf[..k]);
        self.pos += k as u64;
        Ok(k)
    }
}

impl Seek for Source {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let new = match to {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(d) => self.len as i128 + d as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if new < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "posição negativa"));
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}

#[cfg(windows)]
fn positional_read(file: &File, out: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, out, offset)
}

#[cfg(unix)]
fn positional_read(file: &File, out: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, out, offset)
}

/// `x.7z.001` -> [`x.7z.001`, `x.7z.002`, ...] enquanto existirem; qualquer outro nome vira ele mesmo.
pub fn volume_parts(path: &Path) -> Vec<PathBuf> {
    let name = path.to_string_lossy();
    if let Some(base) = name.strip_suffix(".001") {
        let mut out = vec![path.to_path_buf()];
        for i in 2..10_000 {
            let p = PathBuf::from(format!("{base}.{i:03}"));
            if !p.exists() {
                break;
            }
            out.push(p);
        }
        return out;
    }
    vec![path.to_path_buf()]
}

/// Leitor que pode atravessar threads (para os decodificadores de fluxo).
pub type BoxRead<'a> = Box<dyn Read + Send + 'a>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_across_volumes() {
        let dir = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0..1_000_000u32).map(|i| (i * 7 % 251) as u8).collect();
        for (i, chunk) in data.chunks(300_001).enumerate() {
            let mut f = File::create(dir.path().join(format!("a.bin.{:03}", i + 1))).unwrap();
            f.write_all(chunk).unwrap();
        }
        let mut s = Source::open(&dir.path().join("a.bin.001")).unwrap();
        assert_eq!(s.volumes(), 4);
        let mut all = Vec::new();
        s.read_to_end(&mut all).unwrap();
        assert_eq!(all, data);
        s.seek(SeekFrom::Start(299_990)).unwrap();
        let mut b = [0u8; 40];
        s.read_exact(&mut b).unwrap();
        assert_eq!(&b[..], &data[299_990..300_030]);
        let mut c = s.clone();
        c.seek(SeekFrom::End(-5)).unwrap();
        let mut e = Vec::new();
        c.read_to_end(&mut e).unwrap();
        assert_eq!(e, &data[data.len() - 5..]);
    }
}
