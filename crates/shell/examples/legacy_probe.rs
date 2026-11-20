//! Lista os itens do menu de contexto clássico ("Mostrar mais opções") de um arquivo, montado
//! pelo IContextMenu do shell como o Explorer faz, e marca as linhas do RustZIP.

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{CMF_EXTENDEDVERBS, CMF_NORMAL, IContextMenu, IShellFolder, SHBindToParent, SHParseDisplayName};
use windows::Win32::UI::WindowsAndMessaging::{CreatePopupMenu, GetMenuItemCount, GetMenuItemInfoW, HMENU, MENUITEMINFOW, MIIM_STRING, MIIM_SUBMENU};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Shell::IContextMenu3;
use windows::Win32::UI::WindowsAndMessaging::WM_INITMENUPOPUP;
use windows::core::{HSTRING, Interface, PWSTR};

static mut CM3: Option<IContextMenu3> = None;

unsafe fn dump(menu: HMENU, depth: usize) {
    unsafe {
        for i in 0..GetMenuItemCount(Some(menu)) {
            let mut buf = [0u16; 256];
            let mut mi = MENUITEMINFOW { cbSize: size_of::<MENUITEMINFOW>() as u32, fMask: MIIM_STRING | MIIM_SUBMENU, dwTypeData: PWSTR(buf.as_mut_ptr()), cch: 255, ..Default::default() };
            if GetMenuItemInfoW(menu, i as u32, true, &mut mi).is_err() {
                continue;
            }
            let text = String::from_utf16_lossy(&buf[..mi.cch as usize]);
            if text.is_empty() {
                continue;
            }
            let mark = if text.contains("RustZIP") { "  <==" } else { "" };
            println!("{}{}{}", "  ".repeat(depth), text.replace('&', ""), mark);
            if !mi.hSubMenu.is_invalid() && text.contains("RustZIP") {
                // O Explorer preenche submenus de IExplorerCommand só ao abrir (WM_INITMENUPOPUP).
                #[allow(static_mut_refs)]
                if let Some(cm3) = CM3.as_ref() {
                    let _ = cm3.HandleMenuMsg2(WM_INITMENUPOPUP, WPARAM(mi.hSubMenu.0 as usize), LPARAM(i as isize), None);
                }
                dump(mi.hSubMenu, depth + 1);
            }
        }
    }
}

fn main() -> windows::core::Result<()> {
    let path = std::env::args().nth(1).expect("caminho");
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        SHParseDisplayName(&HSTRING::from(path.as_str()), None, &mut pidl, 0, None)?;
        let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
        let folder: IShellFolder = SHBindToParent(pidl, Some(&mut child))?;
        let cm: IContextMenu = folder.GetUIObjectOf(windows::Win32::Foundation::HWND::default(), &[child as *const ITEMIDLIST], None)?;
        let menu = CreatePopupMenu()?;
        CM3 = cm.cast::<IContextMenu3>().ok();
        let _ = cm.QueryContextMenu(menu, 0, 1, 0x7FFF, CMF_NORMAL | CMF_EXTENDEDVERBS);
        dump(menu, 0);
    }
    Ok(())
}
