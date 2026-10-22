# SPDX-License-Identifier: AGPL-3.0-only
# Build del client Merlin-RustDesk sul PC Windows di build (andrea@192.168.40.118).
# Dal Mac: tar del repo (senza target/.git/node_modules) copiato come
# merlin-rustdesk-new.tgz nella home, poi questo script con -EncodedCommand.
# Exe portable: merlin-rustdesk\target\release\merlin-rustdesk-client.exe
$env:VCPKG_ROOT = 'C:\vcpkg'
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\Program Files\LLVM\bin;$env:PATH"
$log = "$env:USERPROFILE\client-build.log"
Set-Location $env:USERPROFILE
tar -xzf merlin-rustdesk-new.tgz *> $log
Remove-Item -Force merlin-rustdesk-new.tgz
Set-Location "$env:USERPROFILE\merlin-rustdesk\client"
npm ci *>> $log
npx tauri build --bundles nsis *>> $log
"CLIENT_EXIT=$LASTEXITCODE" | Out-File -Append $log
