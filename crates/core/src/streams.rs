//! Compressores de fluxo (gz, xz, zst, bz2, lz4, br), sozinhos ou por baixo de um TAR.

use std::io::{self, BufReader, Write};

use crate::error::Result;
use crate::format::Stream;
use crate::source::BoxRead;

pub fn decoder<'a>(stream: Stream, src: BoxRead<'a>) -> Result<BoxRead<'a>> {
    let src = BufReader::with_capacity(1 << 20, src);
    Ok(match stream {
        Stream::None => Box::new(src),
        Stream::Gz => Box::new(flate2::bufread::MultiGzDecoder::new(src)),
        // Decodifica em paralelo os blocos de um .xz multithread.
        Stream::Xz => Box::new(liblzma::bufread::XzDecoder::new_parallel(src)),
        Stream::Zst => {
            let mut d = zstd::stream::read::Decoder::with_buffer(src)?;
            // Aceita as janelas grandes (--long) que o modo "Melhor" grava.
            d.window_log_max(31)?;
            Box::new(d)
        }
        Stream::Bz2 => Box::new(bzip2::bufread::MultiBzDecoder::new(src)),
        Stream::Lz4 => Box::new(lz4_flex::frame::FrameDecoder::new(src)),
        Stream::Br => Box::new(brotli::Decompressor::new(src, 1 << 16)),
    })
}

/// Nível 1..=9 da interface convertido para cada compressor.
pub struct Tuning {
    pub level: u32,
    pub threads: u32,
    /// Janela/dicionário em MiB (0 = padrão do nível).
    pub dictionary_mb: u32,
}

pub type BoxWrite<'a> = Box<dyn FinishWrite + 'a>;

/// Escritor que precisa ser finalizado (gravar rodapé) antes de soltar o destino.
pub trait FinishWrite: Write {
    fn finish_box(self: Box<Self>) -> io::Result<()>;
}

struct Plain<W: Write>(W);
impl<W: Write> Write for Plain<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
impl<W: Write> FinishWrite for Plain<W> {
    fn finish_box(mut self: Box<Self>) -> io::Result<()> {
        self.0.flush()
    }
}

macro_rules! finish_impl {
    ($t:ty, $s:ident => $body:expr) => {
        impl<W: Write> FinishWrite for $t {
            fn finish_box(self: Box<Self>) -> io::Result<()> {
                let $s = *self;
                let mut w = $body?;
                w.flush()
            }
        }
    };
}

finish_impl!(flate2::write::GzEncoder<W>, e => e.finish());
finish_impl!(liblzma::write::XzEncoder<W>, e => e.finish());
finish_impl!(zstd::stream::write::Encoder<'static, W>, e => e.finish());
finish_impl!(bzip2::write::BzEncoder<W>, e => e.finish());
finish_impl!(lz4_flex::frame::FrameEncoder<W>, e => e.finish().map_err(io::Error::other));

struct Brotli<W: Write>(brotli::CompressorWriter<W>);
impl<W: Write> Write for Brotli<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
impl<W: Write> FinishWrite for Brotli<W> {
    fn finish_box(self: Box<Self>) -> io::Result<()> {
        let mut w = self.0.into_inner();
        w.flush()
    }
}

pub fn zstd_level(level: u32) -> i32 {
    [1, 1, 2, 3, 5, 7, 9, 12, 16, 19][level.min(9) as usize]
}

pub fn encoder<'a, W: Write + 'a>(stream: Stream, out: W, t: &Tuning) -> Result<BoxWrite<'a>> {
    let level = t.level.clamp(1, 9);
    let threads = t.threads.max(1);
    Ok(match stream {
        Stream::None => Box::new(Plain(out)),
        Stream::Gz => Box::new(flate2::write::GzEncoder::new(out, flate2::Compression::new(level))),
        Stream::Xz => {
            let mut b = liblzma::stream::MtStreamBuilder::new();
            let preset = if level == 9 { 9 | liblzma::stream::PRESET_EXTREME } else { level };
            b.preset(preset).threads(threads).check(liblzma::stream::Check::Crc64);
            if t.dictionary_mb > 0 {
                // Bloco = 3x o dicionário, como o xz faz; menor dá mais paralelismo.
                b.block_size(t.dictionary_mb as u64 * 3 << 20);
            }
            Box::new(liblzma::write::XzEncoder::new_stream(out, b.encoder().map_err(io::Error::other)?))
        }
        Stream::Zst => {
            let mut e = zstd::stream::write::Encoder::new(out, zstd_level(level))?;
            if threads > 1 {
                e.multithread(threads)?;
            }
            e.include_checksum(true)?;
            if level >= 7 {
                // Correspondências longas: ganha muito em dados repetidos (VMs, backups, logs).
                e.long_distance_matching(true)?;
                let wlog = if t.dictionary_mb > 0 {
                    (t.dictionary_mb as u64 * (1 << 20)).ilog2().clamp(20, 31)
                } else {
                    27
                };
                e.window_log(wlog)?;
            }
            Box::new(e)
        }
        Stream::Bz2 => Box::new(bzip2::write::BzEncoder::new(out, bzip2::Compression::new(level))),
        Stream::Lz4 => Box::new(lz4_flex::frame::FrameEncoder::new(out)),
        Stream::Br => {
            let quality = [0, 1, 3, 4, 5, 6, 8, 9, 10, 11][level as usize];
            let lgwin = if level >= 7 { 24 } else { 22 };
            Box::new(Brotli(brotli::CompressorWriter::new(out, 1 << 16, quality, lgwin)))
        }
    })
}
