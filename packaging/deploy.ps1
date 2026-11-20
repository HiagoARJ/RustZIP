# Copia o build para a pasta de instalação (padrão: Documentos\RustZIP). Arquivos em uso
# (RustZIP.exe aberto, rustzip_shell.dll carregada pelo Explorer) são renomeados para .old antes —
# o Windows deixa renomear um executável/DLL em uso, só não sobrescrever.
param([string]$Dest = (Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'RustZIP'))
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
New-Item -ItemType Directory -Force $Dest | Out-Null
Get-ChildItem $Dest -Filter '*.old*' -ErrorAction SilentlyContinue | ForEach-Object { Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue }
$files = @(
    "$root\target\release\RustZIP.exe",
    "$root\target\release\rz.exe",
    "$root\target\release\rustzip_shell.dll",
    "$root\assets\rustzip.ico",
    "$root\assets\rustzip.avif",
    "$root\packaging\install-menu.ps1",
    "$root\packaging\out\RustZIP.ShellMenu.msix",
    "$root\packaging\out\RustZIP.ShellMenu.cer"
)
foreach ($f in $files) {
    $target = Join-Path $Dest (Split-Path -Leaf $f)
    if (Test-Path $target) {
        try { [IO.File]::Open($target, 'Open', 'ReadWrite', 'None').Close() }
        catch { Rename-Item $target ("{0}.{1}.old" -f (Split-Path -Leaf $target), (Get-Date -Format 'HHmmss')) }
    }
    Copy-Item $f $target -Force
}
New-Item -ItemType Directory -Force "$Dest\Assets" | Out-Null
Copy-Item "$root\packaging\msix\Assets\*.png" "$Dest\Assets" -Force
Write-Output "Instalado em $Dest"
