# Avisos de terceiros

O código do RustZIP é licenciado sob a [MIT](LICENSE). Os executáveis distribuídos nos
releases também contêm bibliotecas de terceiros, cada uma sob a sua própria licença.

## UnRAR (leitura de arquivos RAR)

A extração de RAR usa o código-fonte oficial do UnRAR, de Alexander Roshal, embutido pela crate
[`unrar_sys`](https://crates.io/crates/unrar_sys) (a crate [`unrar`](https://crates.io/crates/unrar)
que o envolve é MIT/Apache-2.0). Esse código **não** está sob a licença MIT: ele permite ler e
extrair arquivos RAR, mas proíbe usá-lo para criar um compactador compatível com RAR ou recriar o
algoritmo de compressão do RAR, que é proprietário. É por isso que o RustZIP extrai RAR, mas grava
em 7Z, ZIP, TAR.* e outros formatos abertos.

Texto integral da licença, como ela exige:

```text
 ******    *****   ******   UnRAR - free utility for RAR archives
 **   **  **   **  **   **  ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
 ******   *******  ******    License for use and distribution of
 **   **  **   **  **   **   ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
 **   **  **   **  **   **         FREE portable version
                                   ~~~~~~~~~~~~~~~~~~~~~

      The source code of UnRAR utility is freeware. This means:

   1. All copyrights to RAR and the utility UnRAR are exclusively
      owned by the author - Alexander Roshal.

   2. UnRAR source code may be used in any software to handle
      RAR archives without limitations free of charge, but cannot be
      used to develop RAR (WinRAR) compatible archiver and to
      re-create RAR compression algorithm, which is proprietary.
      Distribution of modified UnRAR source code in separate form
      or as a part of other software is permitted, provided that
      full text of this paragraph, starting from "UnRAR source code"
      words, is included in license, or in documentation if license
      is not available, and in source code comments of resulting package.

   3. The UnRAR utility may be freely distributed. It is allowed
      to distribute UnRAR inside of other software packages.

   4. THE RAR ARCHIVER AND THE UnRAR UTILITY ARE DISTRIBUTED "AS IS".
      NO WARRANTY OF ANY KIND IS EXPRESSED OR IMPLIED.  YOU USE AT 
      YOUR OWN RISK. THE AUTHOR WILL NOT BE LIABLE FOR DATA LOSS, 
      DAMAGES, LOSS OF PROFITS OR ANY OTHER KIND OF LOSS WHILE USING
      OR MISUSING THIS SOFTWARE.

   5. Installing and using the UnRAR utility signifies acceptance of
      these terms and conditions of the license.

   6. If you don't agree with terms of the license you must remove
      UnRAR files from your storage devices and cease to use the
      utility.

      Thank you for your interest in RAR and UnRAR.


                                            Alexander L. Roshal
```

## Bibliotecas Rust

| Biblioteca | Uso | Licença |
|---|---|---|
| [sevenz-rust2](https://crates.io/crates/sevenz-rust2) + [lzma-rust2](https://crates.io/crates/lzma-rust2) | 7z, LZMA/LZMA2, AES-256 | Apache-2.0 |
| [zip](https://crates.io/crates/zip) | ZIP, Deflate64, AES | MIT |
| [zstd](https://crates.io/crates/zstd) (libzstd) | Zstandard | MIT/Apache-2.0 (libzstd: BSD-3-Clause) |
| [liblzma](https://crates.io/crates/liblzma) (xz-utils) | XZ multithread | MIT/Apache-2.0 (liblzma: 0BSD) |
| [flate2](https://crates.io/crates/flate2) + [zlib-rs](https://crates.io/crates/zlib-rs) | Deflate/GZIP | MIT/Apache-2.0, Zlib |
| [bzip2](https://crates.io/crates/bzip2) (libbz2-rs) | BZip2 | MIT/Apache-2.0 |
| [lz4_flex](https://crates.io/crates/lz4_flex) | LZ4 | MIT |
| [brotli](https://crates.io/crates/brotli) | Brotli | BSD-3-Clause/MIT |
| [tar](https://crates.io/crates/tar) | TAR | MIT/Apache-2.0 |
| [rayon](https://crates.io/crates/rayon) | paralelismo | MIT/Apache-2.0 |
| [tauri](https://crates.io/crates/tauri) | janela/WebView2 | MIT/Apache-2.0 |
| [windows](https://crates.io/crates/windows) | APIs do Windows, COM | MIT/Apache-2.0 |
| [ravif](https://crates.io/crates/ravif), [jxl-oxide](https://crates.io/crates/jxl-oxide), [image](https://crates.io/crates/image), [ico](https://crates.io/crates/ico) | só na ferramenta de ícones (`tools/iconkit`) | BSD-3-Clause, MIT/Apache-2.0 |

A lista completa e as versões exatas estão no `Cargo.lock`.

## Interface

| Pacote | Licença |
|---|---|
| React, React DOM | MIT |
| @tauri-apps/api, @tauri-apps/plugin-dialog | MIT/Apache-2.0 |
| lucide-react (ícones) | ISC |
| Tailwind CSS, Vite | MIT |
| Fontes Fontsource (Manrope, Inter, Space Grotesk, Sora, Instrument Sans, Plus Jakarta Sans, Unbounded, Shippori Mincho, Zen Maru Gothic) | SIL Open Font License 1.1 |

## Marcas

WinRAR e RAR são marcas da win.rar GmbH. 7-Zip é de Igor Pavlov. O RustZIP é um projeto
independente, sem vínculo com essas empresas ou autores, e não contém código do WinRAR.
