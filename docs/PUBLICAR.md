# Publicar uma versão

1. **Versão.** Atualize `version` em `Cargo.toml` (seção `[workspace.package]`),
   `src-tauri/tauri.conf.json` e `ui/package.json`, e acrescente a seção da versão no
   [CHANGELOG.md](../CHANGELOG.md).
2. **Testes.**

   ```powershell
   cargo test -p rustzip-core -p rustzip-shell
   pnpm --dir ui typecheck
   ```

3. **Pacote.**

   ```powershell
   powershell -ExecutionPolicy Bypass -File packaging\make-release.ps1
   ```

   Compila a interface, o aplicativo, a DLL e a CLI, gera e assina o pacote do menu e cria em
   `packaging\out\release`:

   - `RustZIP-<versão>-windows-x64.zip`: `RustZIP.exe`, `rz.exe`, `rustzip_shell.dll`, ícones,
     `install-menu.ps1`, o pacote `RustZIP.ShellMenu.msix` com o certificado público `.cer`,
     `LICENSE`, `THIRD-PARTY-NOTICES.md` e `LEIA-ME.txt`;
   - `SHA256SUMS.txt`.

   `-SkipBuild` só empacota o que já está em `target\release`.

4. **Tag e release.**

   ```powershell
   git tag -a v<versão> -m "RustZIP <versão>"
   git push origin v<versão>
   ```

   No GitHub, em **Releases → Draft a new release**, escolha a tag, cole a seção do CHANGELOG e
   anexe o `.zip` e o `SHA256SUMS.txt`.

## Sobre o certificado do pacote do menu

O pacote do menu do Windows 11 é assinado com um certificado autoassinado (`CN=RustZIP Local`)
criado pelo `build-menu.ps1` em `CurrentUser\My` de quem gera o release. Só a parte pública
(`.cer`) vai no release; a chave privada nunca deve ser exportada nem versionada. Quem instala
confia nesse certificado uma vez (o `install-menu.ps1` pede administrador para isso). Se o
certificado for recriado, o próximo release terá outra assinatura e o `install-menu.ps1` confiará no
novo na instalação seguinte.
