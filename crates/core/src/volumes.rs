//! Volumes no esquema do 7-Zip (`x.7z.001`, `x.7z.002`...), que o 7-Zip, o WinRAR e o próprio
//! RustZIP abrem pelo primeiro volume.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::error::Result;

/// Corta `path` em volumes de `size` bytes e apaga o original. Se já cabe em um, fica como está.
pub fn split(path: &Path, size: u64) -> Result<Vec<PathBuf>> {
    let total = std::fs::metadata(path)?.len();
    if size == 0 || total <= size {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut src = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut outs = Vec::new();
    let mut i = 1;
    loop {
        let part = PathBuf::from(format!("{}.{i:03}", path.display()));
        let mut w = BufWriter::with_capacity(1 << 20, File::create(&part)?);
        let n = io::copy(&mut (&mut src).take(size), &mut w)?;
        w.flush()?;
        drop(w);
        if n == 0 {
            std::fs::remove_file(&part)?;
            break;
        }
        outs.push(part);
        i += 1;
        if n < size {
            break;
        }
    }
    drop(src);
    std::fs::remove_file(path)?;
    Ok(outs)
}
