# Desempenho

## Resultado

Mesmo computador, 16 threads, Windows 11. Conjunto de teste: 178 MB em 8.775 arquivos (um
`node_modules` real, código Rust e os executáveis do próprio WinRAR), tudo em cache de disco.

| Ferramenta e modo | Tempo | Tamanho | vs. WinRAR RAR Normal |
|---|---:|---:|---|
| WinRAR · RAR Normal (`-m3`) | 4,72 s | 60,4 MB | — |
| WinRAR · RAR Melhor, sólido (`-m5 -s`) | 7,25 s | 48,5 MB | — |
| WinRAR · ZIP Normal | 5,30 s | 66,9 MB | — |
| RustZIP · ZIP Normal (`-m5`) | **0,77 s** | 67,7 MB | 6,1× mais rápido |
| RustZIP · 7Z Mais rápida (`-m1`) | **1,78 s** | **54,6 MB** | 2,6× mais rápido, 10% menor |
| RustZIP · 7Z Rápida (`-m3`) | 3,84 s | 51,6 MB | 1,2× mais rápido, 15% menor |
| RustZIP · TAR.ZST Boa (`-m7`) | **2,27 s** | 49,5 MB | 2,1× mais rápido, 18% menor |
| RustZIP · 7Z Normal (`-m5`) | 9,03 s | 48,7 MB | ≈ RAR Melhor em tamanho |
| RustZIP · 7Z Boa (`-m7`) | 16,3 s | **47,1 MB** | 3% menor que o RAR Melhor |
| RustZIP · TAR.XZ (`-m6`) | 13,0 s | 48,6 MB | |

**Extração** (mesmo conjunto): RAR sólido 7,3 s no WinRAR; 7Z, ZIP e TAR.ZST entre 7,5 e 11 s no
RustZIP (RAR até 12 s), dependendo da rodada. O tempo é quase todo do Windows criando os arquivos: medido
diretamente, cada `CreateFile` custa ~1 ms com o antivírus ativo, em qualquer programa.

## De onde vem a velocidade

- **ZIP em paralelo.** No ZIP cada entrada é independente. O RustZIP compacta várias ao mesmo
  tempo, em memória e em levas de até 768 MB, e só grava os blocos prontos em ordem. Arquivos maiores
  que 64 MB vão direto para o disco.
- **7Z em blocos sólidos paralelos.** Os arquivos são ordenados por extensão e nome (parecidos
  ficam juntos, como no 7-Zip) e divididos em blocos de tamanho parecido, compactados ao mesmo tempo.
  Com poucos blocos, o LZMA2 de cada bloco também se divide entre threads. O número de threads é
  limitado pela memória livre (o LZMA usa ~11× o dicionário por thread).
- **Dicionário do tamanho do bloco.** Um bloco de 20 KB não aloca as tabelas de um dicionário de
  64 MB.
- **Compactação inteligente.** Fotos, vídeos e arquivos já compactados (detectados pela extensão ou
  por uma amostra de 128 KB comprimida com LZ4) vão num bloco de cópia pura, sem gastar CPU.
  Executáveis passam pelo filtro BCJ x86, que deixa os endereços relativos e melhora a compactação de
  `.exe`/`.dll`.
- **zstd e xz multithread** no TAR, com correspondências longas no zstd a partir do nível Boa.
- **Extração paralela**: entradas de ZIP e blocos de 7Z em paralelo; em TAR, 7Z sólido e RAR, os
  arquivos pequenos são gravados por um pool de threads enquanto o próximo é descompactado.
- **Leitura por posição** (`seek_read`) com buffer adaptativo: várias threads leem o mesmo arquivo
  compactado sem reabri-lo, e uma entrada de 2 KB não puxa 256 KB do disco.

## Como medir

```powershell
cargo build --release -p rustzip-cli
$rz = ".\target\release\rz.exe"
Measure-Command { & $rz a -m5 teste.7z D:\pasta } | Select-Object TotalSeconds
Measure-Command { & $rz x teste.7z D:\saida } | Select-Object TotalSeconds
```

Para comparar com o WinRAR, use o `Rar.exe` na linha de comando (`Rar.exe a -m3 -r teste.rar D:\pasta`)
e meça do mesmo jeito. Atenção: o `Rar.exe` segue links e junções (o `node_modules` do pnpm tem
muitas) e pode compactar o mesmo conteúdo várias vezes; use uma pasta sem links para uma comparação
justa. Rode cada medição mais de uma vez: o antivírus faz os tempos de extração variarem bastante.
