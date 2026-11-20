# Changelog

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e as versões seguem o
[SemVer](https://semver.org/lang/pt-BR/).

## [0.1.0] - 2026-11-20

Primeira versão.

### Compactação e extração

- Cria 7Z, ZIP, TAR, TAR.GZ/XZ/ZST/BZ2/LZ4/BR e GZ/XZ/ZST/BZ2/LZ4/BR de um arquivo só.
- Extrai e testa todos esses formatos, mais RAR/RAR5 (com senha, nomes criptografados e volumes),
  volumes `.001`, autoextraíveis ZIP/RAR e formatos baseados em ZIP.
- ZIP com entradas compactadas em paralelo (6,9× mais rápido que o ZIP do WinRAR no teste).
- 7Z em blocos sólidos paralelos, ordenados por tipo, com LZMA2 multithread quando há poucos blocos.
- Compactação inteligente: cópia pura para fotos, vídeos e arquivos já compactados; filtro BCJ x86
  para executáveis.
- TAR com zstd (correspondências longas nos níveis altos) e xz multithread.
- Extração paralela (entradas de ZIP, blocos de 7Z) e gravação de arquivos pequenos por um pool de
  threads.
- Senha AES-256 em 7Z (também nos nomes) e ZIP; divisão em volumes; comentário de ZIP; exclusão por
  padrões; testar depois de compactar; mandar os originais para a Lixeira.
- Adicionar, excluir e renomear entradas dentro de ZIP e 7Z sem recompactar o resto; converter entre
  formatos.
- Bloqueio de caminhos perigosos na extração (`..`, unidades, nomes reservados do Windows).

### Aplicativo

- Gerenciador de arquivos: pastas, unidades, dentro de arquivos compactados, busca, ícones e nomes de
  tipo do Windows, arrastar e soltar, visualizar com duplo clique.
- Diálogos de adicionar, extrair, converter, informações e analisar formatos (estimativa de tamanho e
  tempo antes de compactar).
- Progresso com velocidade, tempo restante, taxa, pausa e cancelamento; pedido de senha que repete a
  operação.
- 9 temas (Padrão do Windows 11, Liquid Glass, Aurora, Ferrugem, Bento Claro, Holográfico, Mesh,
  Sakura e Constelação), verificados quanto ao contraste de textos e ícones.
- Linha de comando `rz.exe`.

### Integração com o Windows

- Submenu RustZIP no menu de contexto do Windows 11 (pacote esparso com servidor COM fora do
  Explorer) e no menu clássico.
- "Abrir com RustZIP" para os formatos de arquivo compactado, sem trocar o programa padrão.
- Envio por e-mail pelo cliente padrão (MAPI simples).
- `--register`, `--register --modern` e `--unregister` para instalar por script.

[0.1.0]: https://github.com/HiagoARJ/RustZIP/releases/tag/v0.1.0
