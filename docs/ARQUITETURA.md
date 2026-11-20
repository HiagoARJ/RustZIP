# Arquitetura

```text
                    ┌────────────────────────── Explorer ──────────────────────────┐
                    │  clique direito → rustzip_shell.dll (COM, IExplorerCommand)   │
                    │  roda em dllhost.exe (pacote) ou no Explorer (menu clássico)  │
                    └───────────────┬───────────────────────────────────────────────┘
                                    │ RustZIP.exe --add-quick "C:\pasta"
                                    ▼
┌──────────────── RustZIP.exe (src-tauri) ────────────────┐       ┌──── rz.exe (crates/cli) ────┐
│ instância única: 2ª execução repassa os argumentos       │       │ mesma sintaxe do Rar.exe     │
│ janelas: "main" (gerenciador) e "task-N" (tarefas)        │       └──────────────┬──────────────┘
│ jobs em threads → eventos job-progress / job-done         │                      │
│ comandos: list_dir, archive_info, job_start, ...          │                      │
│ ┌────────── WebView2: ui/ (React 19 + Vite) ──────────┐  │                      │
│ │ Manager, AddForm, ExtractForm, JobCard, Opções, temas │  │                      │
│ └──────────────────────────────────────────────────────┘  │                      │
└───────────────────────────┬───────────────────────────────┘                      │
                            ▼                                                       ▼
                ┌──────────────────── rustzip-core (crates/core) ────────────────────┐
                │ format   detecção por assinatura e nome, volumes .001              │
                │ list     entradas de ZIP, 7z, RAR, TAR e fluxos                    │
                │ extract  ZIP/7z em paralelo, RAR (unrar), TAR, pool de gravação    │
                │ create   ZIP paralelo, 7z em blocos paralelos, TAR + zstd/xz MT    │
                │ update   adicionar/excluir/renomear sem recompactar, converter     │
                │ smart    incompressíveis em cópia pura, BCJ x86 para executáveis   │
                │ progress contadores atômicos, pausa e cancelamento                 │
                └────────────────────────────────────────────────────────────────────┘
```

## `crates/core`: o motor

- **`format`**: identifica o formato pelos primeiros bytes (assinaturas de ZIP, 7z, RAR, gzip, xz,
  zstd, bzip2, LZ4, `ustar`) e, nos compressores de fluxo, abre o começo para saber se há um TAR
  dentro. O nome serve de desempate (Brotli não tem assinatura) e para autoextraíveis.
- **`source`**: leitor por posição (`seek_read`) que trata `x.7z.001`, `.002`... como um fluxo só.
  Um clone compartilha os handles e tem cursor e buffer próprios; é assim que várias threads extraem
  do mesmo ZIP sem reabri-lo.
- **`extract`**: um `Plan` decide o que extrair e para onde (seleção, pasta de origem, política
  para arquivos existentes). `safe_join` bloqueia `..`, raízes, unidades e nomes reservados do
  Windows (`CON`, `NUL`...). Teste é a mesma extração gravando em lugar nenhum.
- **`create`**: coleta os itens (respeitando exclusões), ordena por extensão e nome para formatos
  sólidos e grava num arquivo temporário na pasta de destino, que só substitui o final quando tudo
  dá certo. Volumes são cortados no fim, no esquema `.001` do 7-Zip.
- **`update`**: ZIP e 7z copiam o que não mudou byte a byte (`raw_copy_file`, `push_raw_block`); só
  o bloco 7z que perdeu ou trocou uma entrada é recompactado. TAR e fluxos são refeitos por uma pasta
  temporária.
- **`progress`**: `Progress` com contadores atômicos (bytes, arquivos, compactado), item atual,
  pausa e cancelamento. Os leitores com contagem conferem pausa/cancelamento a cada leitura.

Erros têm um `code()` estável (`password_required`, `wrong_password`, `cancelled`, `corrupt`...)
que a interface usa para decidir o que fazer, como pedir a senha e repetir.

## `src-tauri`: o aplicativo

- **Janelas**: a `main` é o gerenciador; cada ação vinda do menu abre uma `task-N` (diálogo de
  adicionar/extrair, ou só o progresso). A interface pergunta a tarefa da janela com `task_get`.
- **Jobs**: `job_start` roda a operação numa thread e emite `job-progress` a cada 200 ms e
  `job-done` no fim para a janela que pediu. Pausar e cancelar mexem no `Progress` do job.
- **Integração**: `fsview` (pastas, unidades, ícones e nomes de tipo do Windows via
  `SHGetFileInfoW`), `shell` (abrir, mostrar no Explorer, e-mail via MAPI, registro do menu) e
  `settings` (`%APPDATA%\RustZIP\settings.json`).

## `crates/shell`: o menu do Explorer

Um `IExplorerCommand` raiz com `ECF_HASSUBCOMMANDS` e um subcomando por item. O título de cada
subcomando é calculado a partir da seleção (`GetTitle`) e a visibilidade (`GetState`) depende de
serem arquivos compactados ou não. `Invoke` só chama o `RustZIP.exe`. Veja
[MENU-DO-EXPLORER.md](MENU-DO-EXPLORER.md).

## `ui`: a interface

React 19, Vite 7 e Tailwind 4. `api.ts` concentra as chamadas ao backend e, fora do Tauri, responde
com dados de exemplo; é isso que permite fotografar e testar as telas num navegador comum
(`?mode=add`, `?path=...`, `?theme=...`, `?dialog=settings`).

## Ícone

`tools/iconkit` lê `assets/source-icon.jxl` (JPEG XL com transparência), grava o mestre em AVIF
(usado pela interface) e gera o `.ico` e os PNGs a partir dos mesmos pixels. O ICO não sai do AVIF
decodificado porque o decodificador de AVIF do Windows (WIC) descarta o canal alfa, o que deixava o
fundo preto no Explorer.

## Testes

- `cargo test -p rustzip-core`: ida e volta em todos os formatos e métodos, senha certa e errada,
  seleção e pasta de origem, volumes, cancelamento, alteração (adicionar/excluir/renomear) e
  conversão. Se o WinRAR estiver instalado, também gera RAR com o `Rar.exe` (simples, com senha e
  nomes criptografados, e em volumes) e testa a extração.
- `cargo test -p rustzip-shell`: nomes sugeridos e corte de nomes longos.
- Exemplos `menu_probe` e `legacy_probe`: o menu como o Explorer o monta.
