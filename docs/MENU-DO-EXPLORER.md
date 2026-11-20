# Menu de contexto do Explorer

O submenu **RustZIP** é implementado por `rustzip_shell.dll` (`crates/shell`), um servidor COM em
Rust com um `IExplorerCommand` principal e um subcomando para cada item. A DLL não compacta nada:
ela só monta a linha de comando e chama o `RustZIP.exe` que está na mesma pasta.

## Itens

| Item | Aparece para | Chama |
|---|---|---|
| Abrir com RustZIP | um arquivo compactado | `RustZIP.exe <arquivo>` |
| Extrair arquivos... | um arquivo compactado | `--extract` (diálogo) |
| Extrair aqui | arquivos compactados | `--extract-here` |
| Extrair para "nome\\" | arquivos compactados | `--extract-folder` |
| Testar arquivo | arquivos compactados | `--test` |
| Adicionar para o arquivo... | qualquer seleção | `--add` (diálogo) |
| Adicionar para "nome.7z" | qualquer seleção | `--add-quick` |
| Comprimir para "nome.7z" e enviar por e-mail | qualquer seleção | `--add-quick-email` |
| Comprimir e enviar por e-mail... | qualquer seleção | `--add-email` |

- O formato de "Adicionar para" (7Z por padrão) e quais itens aparecem são escolhidos em **Opções**;
  a DLL lê `%APPDATA%\RustZIP\settings.json`.
- "Extrair aqui" não espalha arquivos: se o arquivo compactado tem vários itens soltos na raiz, ele
  cria uma pasta com o nome do arquivo (dá para desligar em Opções).
- Seleções muito grandes (mais de ~24 mil caracteres de caminhos) vão numa lista temporária em
  `%TEMP%\RustZIP`, passada como `@lista.txt`; o app apaga a lista depois de ler.

## Duas formas de registro

### Menu do Windows 11 (pacote esparso)

O menu principal do Windows 11 só aceita extensões de aplicativos com identidade de pacote. O
RustZIP instala um pacote MSIX "esparso", `RustZIP.ShellMenu`, que só dá identidade à DLL. Os
arquivos continuam na pasta de instalação (`-ExternalLocation`):

- `com:SurrogateServer`: a DLL roda num `dllhost.exe` separado, então um erro nela não derruba o
  Explorer;
- `desktop4:FileExplorerContextMenus` para `*` e `Directory`;
- assinatura com um certificado autoassinado `CN=RustZIP Local`.

O Windows só instala pacotes assinados por um certificado confiável. Na primeira vez,
`install-menu.ps1` pede administrador (UAC) **só** para importar esse certificado em
`LocalMachine\TrustedPeople`, um repositório usado para validar pacotes, não para sites ou drivers.
O registro do pacote em si é por usuário.

Com o pacote instalado, o submenu também aparece em "Mostrar mais opções".

### Menu clássico (registro do usuário)

Sem o pacote, o RustZIP pode se registrar só em `HKEY_CURRENT_USER`, sem administrador:

| Chave em `HKCU\Software\Classes` | Conteúdo |
|---|---|
| `CLSID\{7A3E91C4-2B6D-4F18-9C05-D84E1F6A2B37}\InprocServer32` | caminho da DLL, `ThreadingModel=Apartment` |
| `*\shell\RustZIP`, `Directory\shell\RustZIP` | `MUIVerb`, `Icon`, `ExplorerCommandHandler` = o CLSID |
| `RustZIP.Archive` | ícone e comando "abrir" do RustZIP |
| `.7z`, `.zip`, `.rar`, `.tar`... `\OpenWithProgids` | valor `RustZIP.Archive` (só "Abrir com") |

Quando o pacote do Windows 11 está instalado, os verbos `*\shell\RustZIP` e `Directory\shell\RustZIP`
não são criados (o submenu apareceria duas vezes no menu clássico).

## Comandos

```powershell
.\RustZIP.exe --register --modern   # pacote do Windows 11 + "Abrir com"
.\RustZIP.exe --register            # menu clássico + "Abrir com"
.\RustZIP.exe --unregister          # remove pacote, chaves e associações
.\install-menu.ps1                  # só o pacote do Windows 11
.\install-menu.ps1 -Uninstall
```

O resultado fica em `%APPDATA%\RustZIP\registro.log`. As mesmas ações estão em **Opções → Menu de
contexto do Explorer**.

## Atualizar a DLL

A DLL fica carregada no `dllhost.exe` (pacote) ou no Explorer (menu clássico) e não pode ser
sobrescrita. O `packaging/deploy.ps1` renomeia o arquivo em uso para `.old` (o Windows permite
renomear uma DLL carregada) e copia a nova; a antiga é apagada na próxima instalação.

## Testar sem o Explorer

Dois exemplos montam o menu como o Explorer faz, sem clicar em nada:

```powershell
cargo run --release -p rustzip-shell --example menu_probe -- C:\caminho\arquivo.zip
cargo run --release -p rustzip-shell --example legacy_probe -- C:\caminho\arquivo.zip
```

`menu_probe` cria o `IExplorerCommand` pelo CLSID e lista os subcomandos visíveis com os títulos
da seleção. `legacy_probe` monta o menu clássico (`IContextMenu`) e abre o submenu do RustZIP.
