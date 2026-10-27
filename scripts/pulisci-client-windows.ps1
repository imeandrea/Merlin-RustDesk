# SPDX-License-Identifier: AGPL-3.0-only
# Rimuove del tutto il client Merlin-RustDesk da un PC Windows, per le prove:
# chiude il client, toglie il servizio, la cartella del programma, i collegamenti,
# le voci di registro e le cartelle dei dati (configurazione con server e chiave
# compresa). Non tocca MerlinHost (il componente di Merlin) né RustDesk originale.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File pulisci-client-windows.ps1 [-Prova]
#
# Va lanciato come amministratore. Il file è in UTF-8 con BOM (per Windows PowerShell 5).
# Con -Prova elenca cosa farebbe senza toccare niente.
param([switch]$Prova)

$ErrorActionPreference = "Continue"
$App = "MerlinRustDesk"           # nome dell'app nel client (APP_NAME)
$Id = "eu.raapp.merlinremote"     # identificatore Tauri del client

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin -and -not $Prova) { Write-Host "Avvialo come amministratore (o con -Prova per vedere cosa farebbe)." -ForegroundColor Red; exit 1 }

function Fai([string]$cosa, [scriptblock]$azione) {
    if ($Prova) { Write-Host "[prova] $cosa" } else { Write-Host $cosa; & $azione }
}

# 1. Processi del client: installato, portatile («<nome>-assistenza.exe») o in avvio.
$proc = Get-Process -ErrorAction SilentlyContinue | Where-Object {
    $_.ProcessName -like "$App*" -or $_.ProcessName -like "merlin-rustdesk*" -or $_.ProcessName -like "*-assistenza" -or
    ($_.Path -and ($_.Path -like "*\$App\*" -or $_.Path -like "*$Id*"))
}
foreach ($p in $proc) { Fai "Chiudo $($p.ProcessName) (pid $($p.Id))" { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }

# 2. Servizio (accesso non presidiato).
if (Get-Service -Name $App -ErrorAction SilentlyContinue) {
    Fai "Fermo ed elimino il servizio $App" { sc.exe stop $App | Out-Null; Start-Sleep -Seconds 2; sc.exe delete $App | Out-Null }
}

# 3. Cartelle del programma e dei dati.
$cartelle = @(
    "$env:ProgramFiles\$App", "${env:ProgramFiles(x86)}\$App",
    "$env:ProgramData\Microsoft\Windows\Start Menu\Programs\$App",
    "$env:WINDIR\ServiceProfiles\LocalService\AppData\Roaming\$App",
    "$env:WINDIR\System32\config\systemprofile\AppData\Roaming\$App",
    "$env:TEMP\$App-install"
)
foreach ($u in Get-ChildItem "$env:SystemDrive\Users" -Directory -ErrorAction SilentlyContinue) {
    $cartelle += "$($u.FullName)\AppData\Roaming\$App", "$($u.FullName)\AppData\Roaming\$Id", "$($u.FullName)\AppData\Local\$Id",
        "$($u.FullName)\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\$App", "$($u.FullName)\AppData\Local\Temp\$App-install"
}
foreach ($c in ($cartelle | Select-Object -Unique)) { if ($c -and (Test-Path $c)) { Fai "Elimino $c" { Remove-Item -LiteralPath $c -Recurse -Force -ErrorAction SilentlyContinue } } }

# 4. Collegamenti sul desktop.
$desktop = @("$env:PUBLIC\Desktop") + (Get-ChildItem "$env:SystemDrive\Users" -Directory -ErrorAction SilentlyContinue | ForEach-Object { "$($_.FullName)\Desktop" })
foreach ($d in $desktop) {
    Get-ChildItem $d -Filter "*.lnk" -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "*$App*" -or $_.Name -like "*Merlin-RustDesk*" } |
        ForEach-Object { $f = $_.FullName; Fai "Elimino il collegamento $f" { Remove-Item -LiteralPath $f -Force -ErrorAction SilentlyContinue } }
}

# 5. Registro: voce di disinstallazione e avvio automatico.
foreach ($k in @("HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$App", "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\$App")) {
    if (Test-Path $k) { Fai "Elimino la chiave $k" { Remove-Item -Path $k -Recurse -Force -ErrorAction SilentlyContinue } }
}
foreach ($run in @("HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run", "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run")) {
    $voci = Get-ItemProperty -Path $run -ErrorAction SilentlyContinue
    if ($voci) {
        foreach ($n in $voci.PSObject.Properties | Where-Object { $_.Name -notlike "PS*" -and ("$($_.Value)" -like "*$App*" -or $_.Name -like "*$App*") }) {
            $nome = $n.Name; Fai "Tolgo l'avvio automatico $nome da $run" { Remove-ItemProperty -Path $run -Name $nome -ErrorAction SilentlyContinue }
        }
    }
}

# 6. Regole del firewall create dall'installazione.
if (Get-NetFirewallRule -DisplayName "$App*" -ErrorAction SilentlyContinue) {
    Fai "Elimino le regole del firewall $App" { Get-NetFirewallRule -DisplayName "$App*" | Remove-NetFirewallRule -ErrorAction SilentlyContinue }
}

if ($Prova) { Write-Host "Fine della prova: niente è stato toccato." } else { Write-Host "Fatto: il client Merlin-RustDesk è stato rimosso." -ForegroundColor Green }
