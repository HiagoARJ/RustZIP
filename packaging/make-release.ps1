# Monta o pacote de release: packaging\out\release\RustZIP-<versão>-windows-x64.zip e SHA256SUMS.txt.
# Compila tudo antes (use -SkipBuild para empacotar o que já está em target\release).
param([string]$Version, [switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
if (-not $Version) {
    $Version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
}
$name = "RustZIP-$Version-windows-x64"
$out = Join-Path $root 'packaging\out\release'
$stage = Join-Path $out $name

if (-not $SkipBuild) {
    Push-Location $root
    try {
        pnpm --dir ui install --frozen-lockfile; if ($LASTEXITCODE) { throw 'pnpm install falhou' }
        pnpm --dir ui build; if ($LASTEXITCODE) { throw 'pnpm build falhou' }
        cargo build --release -p rustzip-app --features tauri/custom-protocol; if ($LASTEXITCODE) { throw 'build do app falhou' }
        cargo build --release -p rustzip-shell -p rustzip-cli; if ($LASTEXITCODE) { throw 'build da DLL/CLI falhou' }
        & "$root\packaging\build-menu.ps1"
    } finally { Pop-Location }
}

Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$stage\Assets" | Out-Null
$files = @(
    "target\release\RustZIP.exe", "target\release\rz.exe", "target\release\rustzip_shell.dll",
    "assets\rustzip.ico", "assets\rustzip.avif",
    "packaging\install-menu.ps1", "packaging\out\RustZIP.ShellMenu.msix", "packaging\out\RustZIP.ShellMenu.cer",
    "LICENSE", "THIRD-PARTY-NOTICES.md"
)
foreach ($f in $files) { Copy-Item (Join-Path $root $f) $stage }
Copy-Item "$root\packaging\msix\Assets\*.png" "$stage\Assets"
@"
RustZIP $Version - compactador de arquivos para Windows 10/11 x64

1. Deixe esta pasta num lugar fixo (por exemplo Documentos\RustZIP) e abra RustZIP.exe.
2. Menu de contexto do Explorer: Opções > Menu de contexto do Explorer, ou no PowerShell:
     .\RustZIP.exe --register --modern    (menu do Windows 11; pede administrador uma vez)
     .\RustZIP.exe --register             (só o menu clássico)
     .\RustZIP.exe --unregister           (remove)
3. Linha de comando: rz.exe (rz -h para a ajuda).

Documentação: https://github.com/HiagoARJ/RustZIP
Licença MIT (LICENSE). Bibliotecas de terceiros, incluindo o UnRAR: THIRD-PARTY-NOTICES.md.
"@ | Set-Content "$stage\LEIA-ME.txt" -Encoding UTF8

$zip = Join-Path $out "$name.zip"
Remove-Item $zip -ErrorAction SilentlyContinue
# O próprio rz monta o ZIP (o Compress-Archive do PowerShell 5.1 grava caminhos com "\").
& "$root\target\release\rz.exe" a -m9 $zip $stage
if ($LASTEXITCODE) { throw 'rz falhou ao montar o zip' }
$sums = Get-ChildItem $out -Filter '*.zip' | ForEach-Object { '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name }
$sums | Set-Content (Join-Path $out 'SHA256SUMS.txt') -Encoding ASCII
Write-Output "ok: $zip"
$sums
