# SPDX-License-Identifier: AGPL-3.0-only
# Build del client Merlin-RustDesk sul PC Windows di build (andrea@192.168.40.118).
# Dal Mac: tar del repo (senza target/.git/node_modules) copiato come
# merlin-rustdesk-new.tgz nella home insieme a questo script, poi
#   powershell -NoProfile -ExecutionPolicy Bypass -File build-client-windows.ps1 -Variant pubblico|privato
# Exe portable: merlin-rustdesk\target\release\merlin-rustdesk-client.exe
# -Variant pubblico: nessun server compilato (MERLIN_RD_PUBLIC=1, anche se c'è
# client\.env); privato: server, chiave e URL da client\.env (mai pubblicato).
param([Parameter(Mandatory = $true)][ValidateSet('pubblico', 'privato')][string]$Variant)
if ($Variant -eq 'pubblico') { $env:MERLIN_RD_PUBLIC = '1' } else { Remove-Item Env:MERLIN_RD_PUBLIC -ErrorAction SilentlyContinue }
$env:VCPKG_ROOT = 'C:\vcpkg'
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\Program Files\LLVM\bin;$env:PATH"
$log = "$env:USERPROFILE\client-build.log"
Set-Location $env:USERPROFILE
if (Test-Path merlin-rustdesk-new.tgz) {
  tar -xzf merlin-rustdesk-new.tgz *> $log
  Remove-Item -Force merlin-rustdesk-new.tgz
} else { "sorgente già estratto" | Out-File $log }
Set-Location "$env:USERPROFILE\merlin-rustdesk\client"
npm ci *>> $log
npx tauri build --bundles nsis *>> $log
"CLIENT_EXIT=$LASTEXITCODE VARIANT=$Variant" | Out-File -Append $log
