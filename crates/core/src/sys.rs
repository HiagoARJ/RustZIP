/// Threads a usar: o pedido, ou todos os núcleos lógicos.
pub fn threads(requested: u32) -> usize {
    if requested > 0 {
        requested as usize
    } else {
        std::thread::available_parallelism().map_or(4, |n| n.get())
    }
}

/// Memória física livre agora, em bytes (para não abrir mais threads LZMA do que cabe).
#[cfg(windows)]
pub fn available_memory() -> u64 {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut st: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    st.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut st) } != 0 { st.ullAvailPhys } else { 4 << 30 }
}

#[cfg(not(windows))]
pub fn available_memory() -> u64 {
    4 << 30
}

/// Manda arquivos e pastas para a Lixeira (o WinRAR apaga direto; aqui dá para desfazer).
#[cfg(windows)]
pub fn recycle(paths: &[std::path::PathBuf]) -> crate::error::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW, SHFileOperationW,
    };
    if paths.is_empty() {
        return Ok(());
    }
    // Lista dupla-nula de caminhos absolutos.
    let mut from: Vec<u16> = Vec::new();
    for p in paths {
        let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        let s = abs.as_os_str().to_string_lossy();
        let s = s.strip_prefix(r"\?\").unwrap_or(&s).to_string();
        from.extend(std::ffi::OsStr::new(&s).encode_wide());
        from.push(0);
    }
    from.push(0);
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = FO_DELETE;
    op.pFrom = from.as_ptr();
    op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16;
    let rc = unsafe { SHFileOperationW(&mut op) };
    if rc != 0 || op.fAnyOperationsAborted != 0 {
        return Err(crate::error::Error::other(format!("não foi possível mover os originais para a Lixeira (código {rc})")));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn recycle(paths: &[std::path::PathBuf]) -> crate::error::Result<()> {
    for p in paths {
        if p.is_dir() { std::fs::remove_dir_all(p)?; } else { std::fs::remove_file(p)?; }
    }
    Ok(())
}
