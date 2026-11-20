# Documentação do RustZIP

| Página | Conteúdo |
|---|---|
| [ARQUITETURA.md](ARQUITETURA.md) | como o motor, o aplicativo, a interface e a DLL do menu se encaixam |
| [DESEMPENHO.md](DESEMPENHO.md) | comparativo com o WinRAR e de onde vem a velocidade |
| [MENU-DO-EXPLORER.md](MENU-DO-EXPLORER.md) | itens do menu, pacote do Windows 11, registro clássico e o que é gravado no sistema |
| [LINHA-DE-COMANDO.md](LINHA-DE-COMANDO.md) | `rz.exe`: comandos, opções e códigos de saída |
| [TEMAS-E-ACESSIBILIDADE.md](TEMAS-E-ACESSIBILIDADE.md) | os 9 temas, a verificação de contraste e todas as telas em todos os temas |
| [COMPILAR.md](COMPILAR.md) | requisitos, compilação, pacote do menu, instalação local e ícone |
| [PUBLICAR.md](PUBLICAR.md) | como gerar e publicar uma versão |

## Formatos

| Formato | Ler/extrair | Criar | Senha | Observações |
|---|:---:|:---:|:---:|---|
| 7Z | ✓ | ✓ | AES-256 (dados e nomes) | LZMA, LZMA2, PPMd, BZip2, filtros BCJ; sólido; blocos paralelos |
| ZIP | ✓ | ✓ | AES-256 (leitura também ZipCrypto) | Store, Deflate, Deflate64, Zopfli, BZip2, LZMA, XZ, Zstd, PPMd; ZIP64 |
| RAR / RAR5 | ✓ | — | ✓ (inclusive nomes) | volumes `.partN.rar` e `.rNN`, sólido; somente leitura |
| TAR | ✓ | ✓ | — | GNU, nomes longos |
| TAR.GZ, .XZ, .ZST, .BZ2, .LZ4, .BR | ✓ | ✓ | — | xz e zstd multithread |
| GZ, XZ, ZST, BZ2, LZ4, BR | ✓ | ✓ | — | um arquivo só |
| Volumes `.001`, `.002`... | ✓ | ✓ | — | qualquer formato acima |
| Autoextraível (EXE) ZIP/RAR | ✓ | — | — | |
| DOCX, XLSX, APK, JAR, EPUB, CBZ... | ✓ | — | — | baseados em ZIP |
| CBR | ✓ | — | — | baseado em RAR |
