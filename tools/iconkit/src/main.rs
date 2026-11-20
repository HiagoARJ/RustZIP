//! Gera os ícones do RustZIP a partir de `assets/source-icon.jxl` (JPEG XL com fundo
//! transparente):
//!
//! 1. recorta num quadrado e grava o mestre em AVIF (`assets/rustzip.avif`), usado pela interface;
//! 2. gera o `.ico` (executável e menu de contexto), os PNGs do Tauri e os logos do pacote MSIX.
//!
//! O menu do Explorer só desenha `.ico`/recursos, então o AVIF não serve ali. E o decodificador
//! de AVIF do Windows (WIC) devolve tudo opaco, sem o canal alfa: tirar o ICO do AVIF decodificado
//! deixava o fundo preto. Por isso os tamanhos saem dos mesmos pixels RGBA que entram no AVIF; o
//! WIC só confere que o Windows abre o arquivo.
//!
//! Uso: `cargo run --release -p iconkit` na raiz do workspace.

use std::path::{Path, PathBuf};

use image::{RgbaImage, imageops::FilterType};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let src = load_source(&root);
    let square = crop_square(&src, 0.02);

    // Mestre AVIF em 1024 px e uma cópia leve para a interface.
    let master = image::imageops::resize(&square, 1024, 1024, FilterType::Lanczos3);
    let avif = root.join("assets/rustzip.avif");
    write_avif(&master, &avif, 90.0);
    let ui_avif = root.join("ui/public/rustzip.avif");
    std::fs::create_dir_all(ui_avif.parent().unwrap()).unwrap();
    write_avif(&image::imageops::resize(&square, 256, 256, FilterType::Lanczos3), &ui_avif, 88.0);
    match wic_decode(&avif) {
        Ok(img) => println!("o Windows abre o AVIF ({}x{})", img.width(), img.height()),
        Err(e) => eprintln!("aviso: o WIC não abriu o AVIF ({e})"),
    }
    let decoded = master;
    // Prévia sobre branco, para conferir a transparência a olho.
    let mut on_white = RgbaImage::from_pixel(512, 512, image::Rgba([255, 255, 255, 255]));
    image::imageops::overlay(&mut on_white, &sized(&decoded, 512), 0, 0);
    on_white.save(root.join("target/icone-sobre-branco.png")).unwrap();

    // .ico: tamanhos do menu (16/20/24/32 em 100-200% de escala), Explorer e barra de tarefas.
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for s in [16u32, 20, 24, 32, 40, 48, 64, 96, 128, 256] {
        let img = sized(&decoded, s);
        let icon = ico::IconImage::from_rgba_data(s, s, img.into_raw());
        dir.add_entry(ico::IconDirEntry::encode(&icon).unwrap());
    }
    for target in ["assets/rustzip.ico", "src-tauri/icons/icon.ico"] {
        let p = root.join(target);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        dir.write(std::fs::File::create(&p).unwrap()).unwrap();
        println!("{}", p.display());
    }

    let pngs: &[(&str, u32)] = &[
        ("src-tauri/icons/32x32.png", 32),
        ("src-tauri/icons/128x128.png", 128),
        ("src-tauri/icons/128x128@2x.png", 256),
        ("src-tauri/icons/icon.png", 512),
        ("packaging/msix/Assets/StoreLogo.png", 50),
        ("packaging/msix/Assets/Square44x44Logo.png", 44),
        ("packaging/msix/Assets/Square150x150Logo.png", 150),
        ("packaging/msix/Assets/Square44x44Logo.targetsize-16_altform-unplated.png", 16),
        ("packaging/msix/Assets/Square44x44Logo.targetsize-24_altform-unplated.png", 24),
        ("packaging/msix/Assets/Square44x44Logo.targetsize-32_altform-unplated.png", 32),
        ("packaging/msix/Assets/Square44x44Logo.targetsize-48_altform-unplated.png", 48),
        ("ui/public/rustzip-32.png", 32),
    ];
    for (rel, s) in pngs {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        sized(&decoded, *s).save(&p).unwrap();
    }
    println!("{} PNGs gerados", pngs.len());
}

/// Origem: `assets/source-icon.jxl` (JPEG XL, decodificado pelo jxl-oxide) ou, sem ele, um PNG.
fn load_source(root: &Path) -> RgbaImage {
    let jxl = root.join("assets/source-icon.jxl");
    if jxl.exists() {
        let img = jxl_oxide::JxlImage::builder().open(&jxl).expect("JXL");
        let render = img.render_frame(0).expect("quadro JXL");
        let mut stream = render.stream();
        let (w, h, ch) = (stream.width(), stream.height(), stream.channels());
        let mut buf = vec![0f32; (w * h * ch) as usize];
        stream.write_to_buffer(&mut buf);
        println!("JXL {w}x{h}, {ch} canais");
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        let px: Vec<u8> = buf
            .chunks(ch as usize)
            .flat_map(|c| match c.len() {
                4 => [to8(c[0]), to8(c[1]), to8(c[2]), to8(c[3])],
                3 => [to8(c[0]), to8(c[1]), to8(c[2]), 255],
                2 => [to8(c[0]), to8(c[0]), to8(c[0]), to8(c[1])],
                _ => [to8(c[0]), to8(c[0]), to8(c[0]), 255],
            })
            .collect();
        return RgbaImage::from_raw(w, h, px).unwrap();
    }
    image::open(root.join("assets/source-icon.png")).expect("assets/source-icon.png").to_rgba8()
}

/// Recorta pelo alfa e centraliza num quadrado com margem.
fn crop_square(img: &RgbaImage, margin: f32) -> RgbaImage {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 8 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let side = (w.max(h) as f32 * (1.0 + 2.0 * margin)).round() as u32;
    let mut out = RgbaImage::new(side, side);
    let crop = image::imageops::crop_imm(img, x0, y0, w, h).to_image();
    image::imageops::overlay(&mut out, &crop, ((side - w) / 2) as i64, ((side - h) / 2) as i64);
    out
}

/// Redução em duas etapas para tamanhos pequenos (mais nítido que um Lanczos direto de 1024).
fn sized(img: &RgbaImage, s: u32) -> RgbaImage {
    if s <= 48 {
        let mid = image::imageops::resize(img, s * 4, s * 4, FilterType::Lanczos3);
        image::imageops::resize(&mid, s, s, FilterType::CatmullRom)
    } else {
        image::imageops::resize(img, s, s, FilterType::Lanczos3)
    }
}

fn write_avif(img: &RgbaImage, path: &Path, quality: f32) {
    let px: Vec<rgb::RGBA8> = img.pixels().map(|p| rgb::RGBA8::new(p[0], p[1], p[2], p[3])).collect();
    let enc = ravif::Encoder::new()
        .with_quality(quality)
        .with_alpha_quality(quality.max(92.0))
        .with_speed(3)
        .with_alpha_color_mode(ravif::AlphaColorMode::UnassociatedClean);
    let out = enc.encode_rgba(ravif::Img::new(&px[..], img.width() as usize, img.height() as usize)).expect("AVIF");
    std::fs::write(path, &out.avif_file).unwrap();
    println!("{} ({} KB)", path.display(), out.avif_file.len() / 1024);
}

#[cfg(windows)]
fn wic_decode(path: &Path) -> windows::core::Result<RgbaImage> {
    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Graphics::Imaging::*;
    use windows::Win32::System::Com::*;
    use windows::core::HSTRING;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = factory.CreateDecoderFromFilename(
            &HSTRING::from(path.as_os_str()),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;
        let conv = factory.CreateFormatConverter()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
        let (mut w, mut h) = (0, 0);
        conv.GetSize(&mut w, &mut h)?;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        conv.CopyPixels(std::ptr::null(), w * 4, &mut buf)?;
        Ok(RgbaImage::from_raw(w, h, buf).unwrap())
    }
}
