//! Integração com o Windows: abrir/mostrar no Explorer, e-mail via MAPI e o menu de contexto.

use std::path::{Path, PathBuf};

/// CLSID do comando do Explorer implementado em `rustzip_shell.dll` (crates/shell).
pub const SHELL_CLSID: &str = "{7A3E91C4-2B6D-4F18-9C05-D84E1F6A2B37}";

pub fn open(path: &Path) {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::{HSTRING, w};
        ShellExecuteW(None, w!("open"), &HSTRING::from(path.as_os_str()), None, None, SW_SHOWNORMAL);
    }
}

pub fn reveal(path: &Path) {
    let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", path.display())).spawn();
}

/// Abre a janela de nova mensagem do cliente de e-mail padrão (MAPI simples) com os anexos.
#[cfg(windows)]
pub fn send_mail(files: &[PathBuf]) -> Result<(), String> {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::System::Mapi::{MapiFileDescW, MapiMessageW};
    use windows::core::{PWSTR, s, w};
    type SendMailW = unsafe extern "system" fn(usize, usize, *const MapiMessageW, u32, u32) -> u32;
    const MAPI_LOGON_UI: u32 = 0x1;
    const MAPI_DIALOG: u32 = 0x8;
    unsafe {
        let lib = LoadLibraryW(w!("mapi32.dll")).map_err(|e| e.to_string())?;
        let f = GetProcAddress(lib, s!("MAPISendMailW")).ok_or("MAPISendMailW indisponível")?;
        let send: SendMailW = std::mem::transmute(f);
        let mut paths: Vec<Vec<u16>> = files.iter().map(|p| p.as_os_str().encode_wide_z()).collect();
        let mut names: Vec<Vec<u16>> = files
            .iter()
            .map(|p| std::ffi::OsStr::new(&p.file_name().unwrap_or_default().to_string_lossy().into_owned()).encode_wide_z())
            .collect();
        let mut descs: Vec<MapiFileDescW> = paths
            .iter_mut()
            .zip(names.iter_mut())
            .map(|(p, n)| MapiFileDescW { nPosition: u32::MAX, lpszPathName: PWSTR(p.as_mut_ptr()), lpszFileName: PWSTR(n.as_mut_ptr()), ..Default::default() })
            .collect();
        let mut subject: Vec<u16> = std::ffi::OsStr::new(&names_label(files)).encode_wide_z();
        let msg = MapiMessageW { lpszSubject: PWSTR(subject.as_mut_ptr()), nFileCount: descs.len() as u32, lpFiles: descs.as_mut_ptr(), ..Default::default() };
        match send(0, 0, &msg, MAPI_LOGON_UI | MAPI_DIALOG, 0) {
            0 | 1 => Ok(()), // 1 = o usuário fechou a mensagem
            code => Err(format!("o cliente de e-mail recusou (MAPI {code}); o arquivo foi mostrado no Explorer")),
        }
    }
}

#[cfg(not(windows))]
pub fn send_mail(_files: &[PathBuf]) -> Result<(), String> {
    Err("e-mail só no Windows".into())
}

fn names_label(files: &[PathBuf]) -> String {
    files.iter().filter_map(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).collect::<Vec<_>>().join(", ")
}

trait WideZ {
    fn encode_wide_z(&self) -> Vec<u16>;
}

impl WideZ for std::ffi::OsStr {
    fn encode_wide_z(&self) -> Vec<u16> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            self.encode_wide().chain([0]).collect()
        }
        #[cfg(not(windows))]
        {
            self.to_string_lossy().encode_utf16().chain([0]).collect()
        }
    }
}

// ------------------------------------------------------------------ menu de contexto

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Integration {
    /// Submenu no menu clássico ("Mostrar mais opções"), registrado em HKCU.
    pub classic: bool,
    /// Pacote esparso que põe o RustZIP no menu principal do Windows 11.
    pub modern: bool,
    pub install_dir: String,
    pub dll_present: bool,
}

pub fn install_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default()
}

#[cfg(windows)]
mod reg {
    use windows::Win32::System::Registry::*;
    use windows::core::HSTRING;

    pub fn set(path: &str, name: Option<&str>, value: &str) -> windows::core::Result<()> {
        unsafe {
            let mut key = HKEY::default();
            RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(path), None, None, REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut key, None).ok()?;
            let data: Vec<u16> = value.encode_utf16().chain([0]).collect();
            let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
            let r = match name {
                Some(n) => RegSetValueExW(key, &HSTRING::from(n), None, REG_SZ, Some(bytes)),
                None => RegSetValueExW(key, None, None, REG_SZ, Some(bytes)),
            };
            let _ = RegCloseKey(key);
            r.ok()
        }
    }

    pub fn exists(path: &str) -> bool {
        unsafe {
            let mut key = HKEY::default();
            let ok = RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(path), None, KEY_READ, &mut key).is_ok();
            if ok {
                let _ = RegCloseKey(key);
            }
            ok
        }
    }

    pub fn delete_tree(path: &str) {
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(path));
        }
    }
}

const ARCHIVE_EXTS: &[&str] = &[".7z", ".zip", ".rar", ".tar", ".gz", ".tgz", ".xz", ".txz", ".zst", ".tzst", ".bz2", ".tbz2", ".lz4", ".br", ".cbz", ".cbr"];

#[cfg(windows)]
pub fn status() -> Integration {
    let dir = install_dir();
    let modern = modern_installed();
    Integration {
        // O pacote do Windows 11 também aparece em "Mostrar mais opções".
        classic: modern || reg::exists(r"Software\Classes\*\shell\RustZIP"),
        modern,
        install_dir: dir.to_string_lossy().into_owned(),
        dll_present: dir.join("rustzip_shell.dll").exists(),
    }
}

/// Registra o comando do Explorer (DLL) no menu clássico e o RustZIP como opção de "Abrir com"
/// dos formatos de arquivo — tudo em HKCU, sem administrador e sem tirar a associação atual.
#[cfg(windows)]
pub fn install_classic() -> Result<(), String> {
    let dir = install_dir();
    let dll = dir.join("rustzip_shell.dll");
    if !dll.exists() {
        return Err(format!("{} não encontrado", dll.display()));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let ico = dir.join("rustzip.ico");
    let e = |r: windows::core::Result<()>| r.map_err(|e| e.to_string());
    let clsid = format!(r"Software\Classes\CLSID\{SHELL_CLSID}");
    e(reg::set(&clsid, None, "RustZIP"))?;
    e(reg::set(&format!(r"{clsid}\InprocServer32"), None, &dll.to_string_lossy()))?;
    e(reg::set(&format!(r"{clsid}\InprocServer32"), Some("ThreadingModel"), "Apartment"))?;
    // Com o pacote do Windows 11 instalado o submenu já está no menu clássico: um verbo aqui
    // faria o RustZIP aparecer duas vezes.
    if !modern_installed() {
        for target in ["*", "Directory"] {
            let key = format!(r"Software\Classes\{target}\shell\RustZIP");
            e(reg::set(&key, Some("MUIVerb"), "RustZIP"))?;
            e(reg::set(&key, Some("Icon"), &ico.to_string_lossy()))?;
            e(reg::set(&key, Some("ExplorerCommandHandler"), SHELL_CLSID))?;
        }
    }
    let prog = r"Software\Classes\RustZIP.Archive";
    e(reg::set(prog, None, "Arquivo compactado"))?;
    e(reg::set(&format!(r"{prog}\DefaultIcon"), None, &ico.to_string_lossy()))?;
    e(reg::set(&format!(r"{prog}\shell\open\command"), None, &format!("\"{}\" \"%1\"", exe.display())))?;
    for ext in ARCHIVE_EXTS {
        e(reg::set(&format!(r"Software\Classes\{ext}\OpenWithProgids"), Some("RustZIP.Archive"), ""))?;
    }
    notify_assoc();
    Ok(())
}

#[cfg(windows)]
pub fn uninstall_classic() {
    reg::delete_tree(&format!(r"Software\Classes\CLSID\{SHELL_CLSID}"));
    remove_classic_verbs();
    reg::delete_tree(r"Software\Classes\RustZIP.Archive");
    for ext in ARCHIVE_EXTS {
        unsafe {
            use windows::Win32::System::Registry::*;
            let mut key = HKEY::default();
            let path = windows::core::HSTRING::from(format!(r"Software\Classes\{ext}\OpenWithProgids"));
            if RegOpenKeyExW(HKEY_CURRENT_USER, &path, None, KEY_WRITE, &mut key).is_ok() {
                let _ = RegDeleteValueW(key, windows::core::w!("RustZIP.Archive"));
                let _ = RegCloseKey(key);
            }
        }
    }
    notify_assoc();
}

#[cfg(windows)]
fn notify_assoc() {
    unsafe {
        use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

/// O pacote esparso (identidade para o menu do Windows 11) aparece para o usuário atual?
#[cfg(windows)]
fn modern_installed() -> bool {
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "if (Get-AppxPackage -Name RustZIP.ShellMenu) { exit 0 } else { exit 1 }"])
        .creation_flags_hidden()
        .status()
        .is_ok_and(|s| s.success())
}

/// Instala o pacote esparso pelo script em `packaging\install-menu.ps1` (pede elevação uma vez
/// para confiar no certificado autoassinado).
#[cfg(windows)]
pub fn install_modern() -> Result<(), String> {
    let script = install_dir().join("install-menu.ps1");
    if !script.exists() {
        return Err(format!("{} não encontrado", script.display()));
    }
    let st = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .creation_flags_hidden()
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err(format!("o script terminou com código {:?}", st.code()));
    }
    remove_classic_verbs();
    notify_assoc();
    Ok(())
}

#[cfg(windows)]
fn remove_classic_verbs() {
    reg::delete_tree(r"Software\Classes\*\shell\RustZIP");
    reg::delete_tree(r"Software\Classes\Directory\shell\RustZIP");
}

#[cfg(windows)]
pub fn uninstall_modern() -> Result<(), String> {
    let st = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", "Get-AppxPackage -Name RustZIP.ShellMenu | Remove-AppxPackage"])
        .creation_flags_hidden()
        .status()
        .map_err(|e| e.to_string())?;
    if st.success() { Ok(()) } else { Err("Remove-AppxPackage falhou".into()) }
}

trait Hidden {
    fn creation_flags_hidden(&mut self) -> &mut Self;
}

impl Hidden for std::process::Command {
    fn creation_flags_hidden(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            self.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        }
        #[cfg(not(windows))]
        self
    }
}
