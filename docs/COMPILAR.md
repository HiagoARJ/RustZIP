# Compilar e instalar a partir do código

## Requisitos

- Windows 10 ou 11, x64.
- [Rust](https://rustup.rs) estável com a edição 2024 (`rustup update`).
- Visual Studio Build Tools com "Desenvolvimento para desktop com C++" (MSVC e Windows SDK). O
  código do UnRAR (C++), o zstd e o liblzma são compilados junto.
- Node.js 20 ou mais novo e [pnpm](https://pnpm.io) 9 ou mais novo.
- WebView2 (já vem no Windows 10/11 atualizados).

## Passo a passo

```powershell
git clone https://github.com/HiagoARJ/RustZIP.git
cd RustZIP

# Interface
pnpm --dir ui install
pnpm --dir ui build

# Aplicativo, DLL do menu e linha de comando
cargo build --release -p rustzip-app --features tauri/custom-protocol
cargo build --release -p rustzip-shell -p rustzip-cli

# Testes do motor
cargo test -p rustzip-core
```

Os executáveis ficam em `target\release`: `RustZIP.exe`, `rz.exe` e `rustzip_shell.dll`.

Durante o desenvolvimento da interface, `pnpm --dir ui dev` abre o Vite em `http://localhost:1460`.
No navegador, a interface usa dados de exemplo; acrescente `?mode=add`, `?mode=extract`,
`?mode=run`, `?path=C:%5CUsers%5CVoce%5CDocuments%5CProjeto%20A%C3%A7%C3%A3o.7z`,
`?theme=aurora` ou `?dialog=settings` para ver cada tela.

## Pacote do menu do Windows 11

Precisa do `makeappx.exe` e do `signtool.exe` do Windows SDK.

```powershell
powershell -ExecutionPolicy Bypass -File packaging\build-menu.ps1
```

O script cria (na primeira vez) um certificado autoassinado `CN=RustZIP Local` em
`CurrentUser\My`, gera e assina `packaging\out\RustZIP.ShellMenu.msix` e exporta a parte pública do
certificado em `RustZIP.ShellMenu.cer`. A chave privada não sai do seu repositório de certificados.

## Instalar localmente

```powershell
powershell -ExecutionPolicy Bypass -File packaging\deploy.ps1            # Documentos\RustZIP
powershell -ExecutionPolicy Bypass -File packaging\deploy.ps1 -Dest D:\Apps\RustZIP
& "$env:USERPROFILE\Documents\RustZIP\RustZIP.exe" --register --modern
```

O `deploy.ps1` copia os executáveis, o ícone, o pacote e o script do menu. Arquivos em uso (o
aplicativo aberto ou a DLL carregada pelo Explorer) são renomeados para `.old` antes da cópia.

## Ícone

Para trocar o ícone, substitua `assets/source-icon.jxl` (JPEG XL com fundo transparente; PNG também
serve, com o nome `source-icon.png`) e rode:

```powershell
cargo run --release -p iconkit
```

Isso regenera `assets/rustzip.avif`, `assets/rustzip.ico`, os ícones do Tauri, os logos do pacote
MSIX e a cópia leve da interface. Uma prévia sobre fundo branco fica em
`target\icone-sobre-branco.png`.

## Gerar um release

Veja [PUBLICAR.md](PUBLICAR.md).
