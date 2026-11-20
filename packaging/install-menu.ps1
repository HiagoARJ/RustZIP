# Instala (ou remove, com -Uninstall) o pacote esparso que põe o RustZIP no menu de contexto do
# Windows 11. Fica na pasta do RustZIP.exe; o app chama este script em Opções > Menu do Windows 11.
# Na primeira vez pede administrador (UAC) só para confiar no certificado autoassinado do pacote
# em LocalMachine\TrustedPeople; o registro do pacote em si é por usuário.
param([switch]$Uninstall)
$ErrorActionPreference = 'Stop'
$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$name = 'RustZIP.ShellMenu'

Get-AppxPackage -Name $name | Remove-AppxPackage
if ($Uninstall) { exit 0 }

$msix = Join-Path $dir "$name.msix"
$cer = Join-Path $dir "$name.cer"
$cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2 $cer
if (-not (Test-Path "Cert:\LocalMachine\TrustedPeople\$($cert.Thumbprint)")) {
    # Na mesma elevação: confia no certificado novo e tira o do nome antigo (RustRAR), se houver.
    $cmd = "Import-Certificate -FilePath '$cer' -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null; " +
        "Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object { `$_.Subject -eq 'CN=RustRAR Local' } | Remove-Item"
    $p = Start-Process powershell.exe -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList '-NoProfile', '-Command', $cmd
    if ($p.ExitCode -ne 0) { throw "Não foi possível confiar no certificado (código $($p.ExitCode))." }
}
Add-AppxPackage -Path $msix -ExternalLocation $dir
