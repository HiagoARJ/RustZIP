//! Submenu "RustZIP" do menu de contexto do Explorer.
//!
//! Um único `IExplorerCommand` serve aos dois menus: no do Windows 11 ele entra pelo pacote
//! esparso (`packaging/msix`: servidor COM num `SurrogateServer` com
//! `desktop4:FileExplorerContextMenus`); no menu clássico ("Mostrar mais opções") entra por
//! `ExplorerCommandHandler` em HKCU.
//!
//! Os itens só montam a linha de comando e chamam `RustZIP.exe`, que fica na mesma pasta: nada
//! de compactação roda dentro do Explorer.

#![allow(non_snake_case)]

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_NOTIMPL, E_POINTER, HINSTANCE, S_FALSE, S_OK};
use windows::Win32::System::Com::{IBindCtx, IClassFactory, IClassFactory_Impl};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows::Win32::UI::Shell::{
    ECF_DEFAULT, ECF_HASSUBCOMMANDS, ECS_ENABLED, ECS_HIDDEN, IEnumExplorerCommand, IEnumExplorerCommand_Impl, IExplorerCommand,
    IExplorerCommand_Impl, IShellItemArray, SHStrDupW, SIGDN_FILESYSPATH,
};
use windows::core::{BOOL, GUID, HRESULT, HSTRING, Interface, PWSTR, Ref, Result};
use windows_core::implement;

/// Igual a `SHELL_CLSID` em src-tauri/src/shell.rs e ao manifesto do pacote.
pub const CLSID_RUSTZIP: GUID = GUID::from_u128(0x7A3E91C4_2B6D_4F18_9C05_D84E1F6A2B37);

static MODULE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

#[unsafe(no_mangle)]
extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        MODULE.store(hinst.0, Ordering::Relaxed);
    }
    BOOL(1)
}

fn module_dir() -> PathBuf {
    let mut buf = [0u16; 1024];
    let n = unsafe { GetModuleFileNameW(Some(windows::Win32::Foundation::HMODULE(MODULE.load(Ordering::Relaxed))), &mut buf) } as usize;
    let p = PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    p.parent().map(Path::to_path_buf).unwrap_or_default()
}

fn co_str(s: &str) -> Result<PWSTR> {
    unsafe { SHStrDupW(&HSTRING::from(s)) }
}

// ------------------------------------------------------------------ preferências e seleção

struct Prefs {
    ext: String,
    add_dialog: bool,
    add_quick: bool,
    email: bool,
    extract: bool,
    test: bool,
}

/// Lê `%APPDATA%\RustZIP\settings.json` (o app grava; aqui só alguns campos).
fn prefs() -> Prefs {
    let v: serde_json::Value = std::env::var_os("APPDATA")
        .map(|d| PathBuf::from(d).join("RustZIP").join("settings.json"))
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let flag = |k: &str| v.get(k).and_then(|x| x.as_bool()).unwrap_or(true);
    Prefs {
        ext: v.get("defaultFormat").and_then(|x| x.as_str()).unwrap_or("7z").to_string(),
        add_dialog: flag("menuAddDialog"),
        add_quick: flag("menuAddQuick"),
        email: flag("menuEmail"),
        extract: flag("menuExtract"),
        test: flag("menuTest"),
    }
}

fn paths(items: Ref<IShellItemArray>) -> Vec<PathBuf> {
    let Some(items) = items.as_ref() else { return Vec::new() };
    let mut out = Vec::new();
    unsafe {
        let n = items.GetCount().unwrap_or(0);
        for i in 0..n {
            let Ok(item) = items.GetItemAt(i) else { continue };
            if let Ok(name) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                out.push(PathBuf::from(name.to_string().unwrap_or_default()));
                windows::Win32::System::Com::CoTaskMemFree(Some(name.0 as *const c_void));
            }
        }
    }
    out
}

const ARCHIVE_EXTS: &[&str] = &[
    ".7z", ".zip", ".rar", ".tar", ".gz", ".tgz", ".xz", ".txz", ".zst", ".tzst", ".zstd", ".bz2", ".tbz2", ".tbz", ".lz4",
    ".br", ".cbz", ".cbr", ".001",
];

fn is_archive(p: &Path) -> bool {
    let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    p.is_file() && ARCHIVE_EXTS.iter().any(|e| n.ends_with(e))
}

/// "foto.tar.gz" -> "foto"; "a.png" -> "a".
fn stem(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    for d in [".tar.gz", ".tar.xz", ".tar.zst", ".tar.bz2", ".tar.lz4", ".tar.br"] {
        if lower.ends_with(d) {
            return &name[..name.len() - d.len()];
        }
    }
    match name.rfind('.') {
        Some(0) | None => name,
        Some(i) => &name[..i],
    }
}

fn quick_name(sel: &[PathBuf], ext: &str) -> String {
    let base = if sel.len() == 1 {
        let n = sel[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if sel[0].is_dir() { n } else { stem(&n).to_string() }
    } else {
        sel[0].parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Arquivo".into())
    };
    format!("{base}.{ext}")
}

/// Como o WinRAR: nomes longos cortados no meio ("Ícone 3D de arquivo...loridos.rar").
fn ellipsize(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let head = max * 3 / 5;
    let tail = max - head - 3;
    format!("{}...{}", chars[..head].iter().collect::<String>(), chars[chars.len() - tail..].iter().collect::<String>())
}

// ------------------------------------------------------------------ comandos

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Open,
    AddDialog,
    AddQuick,
    AddQuickEmail,
    AddEmail,
    ExtractDialog,
    ExtractHere,
    ExtractFolder,
    Test,
}

const ALL: [Kind; 9] = [
    Kind::Open,
    Kind::ExtractDialog,
    Kind::ExtractHere,
    Kind::ExtractFolder,
    Kind::Test,
    Kind::AddDialog,
    Kind::AddQuick,
    Kind::AddQuickEmail,
    Kind::AddEmail,
];

impl Kind {
    fn title(self, sel: &[PathBuf], p: &Prefs) -> String {
        let quick = || ellipsize(&quick_name(sel, &p.ext), 34);
        match self {
            Kind::Open => "Abrir com RustZIP".into(),
            Kind::AddDialog => "Adicionar para o arquivo...".into(),
            Kind::AddQuick => format!("Adicionar para \"{}\"", ellipsize(&quick_name(sel, &p.ext), 72)),
            Kind::AddQuickEmail => format!("Comprimir para \"{}\" e enviar por e-mail", quick()),
            Kind::AddEmail => "Comprimir e enviar por e-mail...".into(),
            Kind::ExtractDialog => "Extrair arquivos...".into(),
            Kind::ExtractHere => "Extrair aqui".into(),
            Kind::ExtractFolder => {
                let name = if sel.len() == 1 {
                    let n = sel[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    format!("{}\\", ellipsize(stem(&n), 40))
                } else {
                    "*\\".into()
                };
                format!("Extrair para \"{name}\"")
            }
            Kind::Test => "Testar arquivo".into(),
        }
    }

    fn visible(self, sel: &[PathBuf], p: &Prefs) -> bool {
        if sel.is_empty() {
            return false;
        }
        let archives = sel.iter().all(|s| is_archive(s));
        match self {
            Kind::Open => archives && sel.len() == 1,
            Kind::ExtractDialog => archives && p.extract && sel.len() == 1,
            Kind::ExtractHere | Kind::ExtractFolder => archives && p.extract,
            Kind::Test => archives && p.test,
            Kind::AddDialog => p.add_dialog,
            Kind::AddQuick => p.add_quick,
            Kind::AddQuickEmail | Kind::AddEmail => p.email,
        }
    }

    fn arg(self) -> Option<&'static str> {
        Some(match self {
            Kind::Open => return None,
            Kind::AddDialog => "--add",
            Kind::AddQuick => "--add-quick",
            Kind::AddQuickEmail => "--add-quick-email",
            Kind::AddEmail => "--add-email",
            Kind::ExtractDialog => "--extract",
            Kind::ExtractHere => "--extract-here",
            Kind::ExtractFolder => "--extract-folder",
            Kind::Test => "--test",
        })
    }
}

fn launch(kind: Kind, sel: &[PathBuf]) -> Result<()> {
    let exe = module_dir().join("RustZIP.exe");
    let mut cmd = std::process::Command::new(&exe);
    if let Some(a) = kind.arg() {
        cmd.arg(a);
    }
    let total: usize = sel.iter().map(|p| p.as_os_str().len() + 3).sum();
    if total > 24_000 {
        // Seleção grande: lista num arquivo temporário (o app apaga depois de ler).
        let dir = std::env::temp_dir().join("RustZIP");
        let _ = std::fs::create_dir_all(&dir);
        let list = dir.join(format!("lista-{}-{}.txt", std::process::id(), sel.len()));
        let text: String = sel.iter().map(|p| format!("{}\n", p.display())).collect();
        std::fs::write(&list, text).map_err(|_| windows::core::Error::from(E_POINTER))?;
        cmd.arg(format!("@{}", list.display()));
    } else {
        cmd.args(sel);
    }
    cmd.spawn().map(|_| ()).map_err(|_| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))
}

#[implement(IExplorerCommand)]
struct Root;

impl IExplorerCommand_Impl for Root_Impl {
    fn GetTitle(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        co_str("RustZIP")
    }
    fn GetIcon(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        co_str(&module_dir().join("rustzip.ico").to_string_lossy())
    }
    fn GetToolTip(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(CLSID_RUSTZIP)
    }
    fn GetState(&self, items: Ref<IShellItemArray>, _: BOOL) -> Result<u32> {
        Ok(if paths(items).is_empty() { ECS_HIDDEN.0 as u32 } else { ECS_ENABLED.0 as u32 })
    }
    fn Invoke(&self, _: Ref<IShellItemArray>, _: Ref<IBindCtx>) -> Result<()> {
        Ok(())
    }
    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_HASSUBCOMMANDS.0 as u32)
    }
    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        let cmds: Vec<IExplorerCommand> = ALL.iter().map(|&k| Sub { kind: k }.into()).collect();
        Ok(Enum { cmds, pos: AtomicUsize::new(0) }.into())
    }
}

#[implement(IExplorerCommand)]
struct Sub {
    kind: Kind,
}

impl IExplorerCommand_Impl for Sub_Impl {
    fn GetTitle(&self, items: Ref<IShellItemArray>) -> Result<PWSTR> {
        co_str(&self.kind.title(&paths(items), &prefs()))
    }
    fn GetIcon(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        co_str(&module_dir().join("rustzip.ico").to_string_lossy())
    }
    fn GetToolTip(&self, _: Ref<IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }
    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(GUID::from_u128(CLSID_RUSTZIP.to_u128() ^ (self.kind as u128 + 1)))
    }
    fn GetState(&self, items: Ref<IShellItemArray>, _: BOOL) -> Result<u32> {
        let visible = self.kind.visible(&paths(items), &prefs());
        Ok(if visible { ECS_ENABLED.0 as u32 } else { ECS_HIDDEN.0 as u32 })
    }
    fn Invoke(&self, items: Ref<IShellItemArray>, _: Ref<IBindCtx>) -> Result<()> {
        launch(self.kind, &paths(items))
    }
    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }
    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

#[implement(IEnumExplorerCommand)]
struct Enum {
    cmds: Vec<IExplorerCommand>,
    pos: AtomicUsize,
}

impl IEnumExplorerCommand_Impl for Enum_Impl {
    fn Next(&self, celt: u32, out: *mut Option<IExplorerCommand>, fetched: *mut u32) -> HRESULT {
        if out.is_null() {
            return E_POINTER;
        }
        let mut n = 0;
        while n < celt as usize {
            let i = self.pos.fetch_add(1, Ordering::Relaxed);
            let Some(c) = self.cmds.get(i) else {
                self.pos.store(self.cmds.len(), Ordering::Relaxed);
                break;
            };
            unsafe { out.add(n).write(Some(c.clone())) };
            n += 1;
        }
        if !fetched.is_null() {
            unsafe { *fetched = n as u32 };
        }
        if n == celt as usize { S_OK } else { S_FALSE }
    }
    fn Skip(&self, celt: u32) -> Result<()> {
        self.pos.fetch_add(celt as usize, Ordering::Relaxed);
        Ok(())
    }
    fn Reset(&self) -> Result<()> {
        self.pos.store(0, Ordering::Relaxed);
        Ok(())
    }
    fn Clone(&self) -> Result<IEnumExplorerCommand> {
        Ok(Enum { cmds: self.cmds.clone(), pos: AtomicUsize::new(self.pos.load(Ordering::Relaxed)) }.into())
    }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<windows_core::IUnknown>, riid: *const GUID, ppv: *mut *mut c_void) -> Result<()> {
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let cmd: IExplorerCommand = Root.into();
        unsafe { cmd.query(riid, ppv).ok() }
    }
    fn LockServer(&self, _: BOOL) -> Result<()> {
        Ok(())
    }
}

#[unsafe(no_mangle)]
extern "system" fn DllGetClassObject(clsid: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    if ppv.is_null() {
        return E_POINTER;
    }
    unsafe { *ppv = std::ptr::null_mut() };
    if unsafe { *clsid } != CLSID_RUSTZIP {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = Factory.into();
    unsafe { factory.query(riid, ppv) }
}

/// O Explorer pode descarregar a DLL quando quiser; os objetos não guardam estado global.
#[unsafe(no_mangle)]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(quick_name(&[PathBuf::from(r"C:\x\Ícone 3D.png")], "7z"), "Ícone 3D.7z");
        assert_eq!(stem("a.tar.gz"), "a");
        let long = "Ícone 3D de arquivos compactados multicoloridos.rar";
        let e = ellipsize(long, 34);
        assert_eq!(e.chars().count(), 34);
        assert!(e.starts_with("Ícone 3D de arquivo") && e.ends_with("loridos.rar"), "{e}");
    }
}
