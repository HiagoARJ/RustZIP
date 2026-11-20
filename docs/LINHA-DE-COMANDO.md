# Linha de comando: `rz.exe`

`rz` usa o mesmo motor do aplicativo, sem janela. A sintaxe lembra a do `Rar.exe`.

```text
rz <comando> [opções] arquivo [arquivos...]
```

## Comandos

| Comando | Faz | Exemplo |
|---|---|---|
| `a` | compacta; o formato vem da extensão do destino | `rz a projeto.7z .\src .\docs` |
| `x` | extrai mantendo as pastas | `rz x arquivo.rar D:\saida` |
| `e` | extrai tudo numa pasta só, sem subpastas | `rz e fotos.zip D:\fotos` |
| `l` | lista as entradas (tamanho, compactado, método) | `rz l arquivo.7z` |
| `t` | testa (descompacta sem gravar e confere CRC) | `rz t backup.tar.zst` |
| `d` | exclui entradas ou pastas de um ZIP/7Z/TAR | `rz d arquivo.7z pasta/velho.txt` |
| `rn` | renomeia uma entrada ou pasta | `rz rn arquivo.zip docs documentos` |
| `c` | converte para outro formato | `rz c antigo.rar novo.7z` |

Extensões que escolhem o formato em `a` e `c`: `.7z`, `.zip`, `.tar`, `.tar.gz`/`.tgz`,
`.tar.xz`/`.txz`, `.tar.zst`/`.tzst`, `.tar.bz2`, `.tar.lz4`, `.tar.br` e, para um arquivo só,
`.gz`, `.xz`, `.zst`, `.bz2`, `.lz4`, `.br`. Sem extensão conhecida, usa 7Z.

## Opções

| Opção | Efeito |
|---|---|
| `-m0` ... `-m9` | nível: 0 armazenar, 1 mais rápida, 3 rápida, 5 normal (padrão), 7 boa, 9 melhor |
| `-md<MB>` | dicionário (7Z/XZ) ou janela (zstd) em MB, por exemplo `-md64` |
| `-mt<n>` | número de threads (padrão: todos os núcleos) |
| `-s-` | 7Z não sólido (cada arquivo num bloco) |
| `-p<senha>` | senha AES-256 (ZIP e 7Z); também serve para extrair |
| `-hp` | criptografa também os nomes (7Z) |
| `-v<tam>[k\|m\|g]` | divide em volumes, por exemplo `-v700m`; gera `nome.7z.001`, `.002`... |
| `-x<padrão>` | não inclui; pode repetir: `-x*.tmp -xnode_modules -xcache/*.bin` |
| `-zm<método>` | método do ZIP: `deflate` (padrão), `zopfli`, `zstd`, `xz`, `bzip2` |
| `-t` | testa o arquivo depois de compactar |
| `-df` | manda os originais para a Lixeira depois de compactar |
| `-smart-` | desliga a compactação inteligente (recompacta fotos/vídeos, sem BCJ em executáveis) |
| `-o+` / `-o-` / `-or` / `-on` | na extração: substituir / pular / manter os dois / só se for mais novo |

## Códigos de saída

| Código | Significado |
|---|---|
| 0 | sucesso |
| 2 | erro (arquivo corrompido, sem espaço, formato não suportado...) |
| 11 | senha necessária ou incorreta |
| 255 | cancelado |

## Exemplos

```powershell
# Backup com a melhor compactação, senha nos dados e nos nomes, em volumes de 4 GB
rz a -m9 -p"minha senha" -hp -v4g D:\backup\fotos.7z D:\Fotos

# Pasta de código rápida, sem dependências nem lixo de build
rz a -m7 projeto.tar.zst .\projeto -xnode_modules -xtarget -x*.log

# ZIP que abre em qualquer lugar, um pouco menor (Zopfli)
rz a -zmzopfli entrega.zip .\entrega

# Extrair um RAR em partes, mantendo os arquivos que já existem
rz x -o- filme.part01.rar D:\Filmes
```

## O aplicativo também aceita argumentos

O `RustZIP.exe` recebe os mesmos comandos que o menu do Explorer usa (veja
[MENU-DO-EXPLORER.md](MENU-DO-EXPLORER.md)): `--add`, `--add-quick`, `--extract`,
`--extract-here`, `--extract-folder`, `--test`, além de `--register` e `--unregister`. Um caminho
solto abre o gerenciador nessa pasta ou dentro desse arquivo compactado.
