//! Monta o submenu como o Explorer faria (CoCreateInstance pelo registro, EnumSubCommands,
//! GetTitle/GetState para uma seleção) e imprime os itens visíveis. Não chama Invoke.
//!
//! `cargo run -p rustzip-shell --example menu_probe -- C:\caminho\arquivo [outro...]`

use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree};
use windows::Win32::UI::Shell::{ECS_HIDDEN, IExplorerCommand, IShellItemArray, SHCreateShellItemArrayFromIDLists};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::core::{GUID, HSTRING};

fn main() -> windows::core::Result<()> {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let clsid = GUID::from_u128(0x7A3E91C4_2B6D_4F18_9C05_D84E1F6A2B37);
        let root: IExplorerCommand = CoCreateInstance(&clsid, None, CLSCTX_INPROC_SERVER)?;
        let pidls: Vec<*const ITEMIDLIST> = paths
            .iter()
            .map(|p| windows::Win32::UI::Shell::ILCreateFromPathW(&HSTRING::from(p.as_str())) as *const ITEMIDLIST)
            .collect();
        let items: IShellItemArray = SHCreateShellItemArrayFromIDLists(&pidls)?;
        let title = root.GetTitle(&items)?;
        println!("[{}] ícone: {}", title.to_string().unwrap(), root.GetIcon(&items)?.to_string().unwrap());
        CoTaskMemFree(Some(title.0 as _));
        let subs = root.EnumSubCommands()?;
        loop {
            let mut out = [None];
            let mut got = 0;
            let _ = subs.Next(&mut out, Some(&mut got));
            if got == 0 {
                break;
            }
            let cmd = out[0].take().unwrap();
            let state = cmd.GetState(&items, true.into())?;
            if state & ECS_HIDDEN.0 as u32 != 0 {
                continue;
            }
            println!("  • {}", cmd.GetTitle(&items)?.to_string().unwrap());
        }
    }
    Ok(())
}
