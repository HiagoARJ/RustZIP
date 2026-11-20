# Gera e assina packaging\out\RustZIP.ShellMenu.msix (+ .cer). Usa um certificado autoassinado
# "CN=RustZIP Local" em CurrentUser\My (criado na primeira vez; não precisa de administrador).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$kit = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64\makeappx.exe" | Sort-Object FullName | Select-Object -Last 1
$makeappx = $kit.FullName
$signtool = Join-Path $kit.DirectoryName 'signtool.exe'
$out = Join-Path $root 'out'
$stage = Join-Path $out 'stage'
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$stage\Assets" | Out-Null
Copy-Item "$root\msix\AppxManifest.xml" $stage
Copy-Item "$root\msix\Assets\*.png" "$stage\Assets"

$subject = 'CN=RustZIP Local'
$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $subject -and $_.NotAfter -gt (Get-Date).AddDays(30) } | Select-Object -First 1
if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type Custom -Subject $subject -KeyUsage DigitalSignature -FriendlyName 'RustZIP (pacote do menu)' `
        -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(10) `
        -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
}
$msix = Join-Path $out 'RustZIP.ShellMenu.msix'
& $makeappx pack /d $stage /p $msix /nv /o | Out-Null
if ($LASTEXITCODE) { throw "makeappx falhou ($LASTEXITCODE)" }
& $signtool sign /fd SHA256 /sha1 $cert.Thumbprint /s My $msix | Out-Null
if ($LASTEXITCODE) { throw "signtool falhou ($LASTEXITCODE)" }
Export-Certificate -Cert $cert -FilePath (Join-Path $out 'RustZIP.ShellMenu.cer') | Out-Null
Write-Output "ok: $msix"
