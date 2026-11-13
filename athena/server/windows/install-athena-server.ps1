<#
.SYNOPSIS
  Sets up ATHENA as an always-on station on a Windows PC, reachable from browsers on the home network.

.DESCRIPTION
  Run once, in PowerShell as Administrator, after installing ATHENA with its normal installer.
  It:
    1. finds ATHENA's program file (or uses -ExePath);
    2. points ATHENA at a data folder you choose (-DataDir), for example on a large drive;
    3. sets the AI to unload after a few idle minutes and to load one model at a time;
    4. opens the web port (default 8787) on the Windows firewall, for private networks only;
    5. creates a scheduled task that starts ATHENA in server mode when you sign in, at
       below-normal priority so games and media servers get the CPU first, and restarts it if
       it stops.

  Run again with -Uninstall to remove the task, firewall rule and settings. Your data folder is
  never deleted.

.EXAMPLE
  .\install-athena-server.ps1 -DataDir "D:\AthenaData"

.EXAMPLE
  .\install-athena-server.ps1 -Uninstall
#>
[CmdletBinding()]
param(
  [string]$ExePath,
  [string]$DataDir,
  [int]$Port = 8787,
  [int]$ModelIdleMinutes = 2,
  [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$TaskName = "ATHENA Server"
$RuleName = "ATHENA LAN web ($Port)"
$MarkerDir = Join-Path $env:USERPROFILE ".athena"
$Marker = Join-Path $MarkerDir "portable.athena"
$UserVars = @{
  "ATHENA_LAN_PORT"           = "$Port"
  "ATHENA_MODEL_IDLE_MINUTES" = "$ModelIdleMinutes"
  # These apply when Ollama is installed separately rather than started by ATHENA.
  "OLLAMA_KEEP_ALIVE"         = "${ModelIdleMinutes}m"
  "OLLAMA_MAX_LOADED_MODELS"  = "1"
  "OLLAMA_NUM_PARALLEL"       = "1"
}

function Test-Admin {
  $id = [Security.Principal.WindowsIdentity]::GetCurrent()
  (New-Object Security.Principal.WindowsPrincipal $id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not (Test-Admin)) {
  throw "Run this in PowerShell opened with 'Run as administrator' (needed for the firewall rule)."
}

if ($Uninstall) {
  Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue
  Get-NetFirewallRule -DisplayName "ATHENA LAN web*" -ErrorAction SilentlyContinue | Remove-NetFirewallRule
  foreach ($k in @($UserVars.Keys) + "OLLAMA_MODELS") { [Environment]::SetEnvironmentVariable($k, $null, "User") }
  if (Test-Path $Marker) { Remove-Item $Marker }
  Write-Host "Removed the ATHENA server task, firewall rule and settings. Your data folder was left as it is."
  return
}

# 1. Find the program.
if (-not $ExePath) {
  $candidates = @(
    (Join-Path $env:LOCALAPPDATA "ATHENA"),
    (Join-Path $env:LOCALAPPDATA "Programs\ATHENA"),
    (Join-Path $env:ProgramFiles "ATHENA")
  ) | Where-Object { Test-Path $_ }
  $ExePath = $candidates |
    ForEach-Object { Get-ChildItem $_ -Filter *.exe -ErrorAction SilentlyContinue } |
    Where-Object { $_.Name -match '^athena\.exe$' } |
    Select-Object -First 1 -ExpandProperty FullName
}
if (-not $ExePath -or -not (Test-Path $ExePath)) {
  throw "Could not find ATHENA. Install it first, or pass -ExePath 'C:\path\to\ATHENA.exe'."
}
Write-Host "Program:     $ExePath"

# 2. Data folder.
if ($DataDir) {
  New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
  New-Item -ItemType Directory -Force -Path $MarkerDir | Out-Null
  # No byte-order mark: Windows PowerShell's "-Encoding UTF8" would add one.
  [IO.File]::WriteAllText($Marker, (Resolve-Path $DataDir).Path, (New-Object Text.UTF8Encoding $false))
  Write-Host "Data folder: $((Resolve-Path $DataDir).Path)"
  # Keep AI models with the rest of the data (the bundled Ollama otherwise uses %USERPROFILE%\.ollama on C:).
  $models = Join-Path (Resolve-Path $DataDir).Path "user\models\ollama"
  New-Item -ItemType Directory -Force -Path $models | Out-Null
  $UserVars["OLLAMA_MODELS"] = $models
  Write-Host "AI models:   $models"
} elseif (Test-Path $Marker) {
  Write-Host "Data folder: $(Get-Content $Marker -TotalCount 1) (unchanged)"
} else {
  Write-Host "Data folder: Windows' default app data folder (pass -DataDir to use a larger drive)"
}

# 3. Settings for this Windows account.
foreach ($k in $UserVars.Keys) { [Environment]::SetEnvironmentVariable($k, $UserVars[$k], "User") }
Write-Host "AI model:    unloads after $ModelIdleMinutes idle minutes; one model at a time"

# 4. Firewall: home (private) networks only.
Get-NetFirewallRule -DisplayName "ATHENA LAN web*" -ErrorAction SilentlyContinue | Remove-NetFirewallRule
New-NetFirewallRule -DisplayName $RuleName -Direction Inbound -Protocol TCP -LocalPort $Port `
  -Profile Private -Action Allow -Program $ExePath | Out-Null
Write-Host "Firewall:    port $Port open on private networks"

# 5. Start at sign-in, below-normal priority, restart if it stops.
$action = New-ScheduledTaskAction -Execute $ExePath -Argument "--server" -WorkingDirectory (Split-Path $ExePath)
$Account = [Security.Principal.WindowsIdentity]::GetCurrent().Name  # e.g. HOME-SERVER\operator (USERDOMAIN can read WORKGROUP)
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $Account
$settings = New-ScheduledTaskSettingsSet -Priority 7 -ExecutionTimeLimit ([TimeSpan]::Zero) `
  -RestartCount 5 -RestartInterval (New-TimeSpan -Minutes 1) `
  -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable -MultipleInstances IgnoreNew
$principal = New-ScheduledTaskPrincipal -UserId $Account -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings `
  -Principal $principal -Force | Out-Null
Write-Host "Startup:     '$TaskName' starts ATHENA when $env:USERNAME signs in"

# Start it now.
Start-ScheduledTask -TaskName $TaskName
Start-Sleep -Seconds 8
# The home-network address: the adapter that has the router as its gateway (skips VirtualBox,
# Hyper-V and most VPN adapters, which have none).
$ip = (Get-NetIPConfiguration -ErrorAction SilentlyContinue |
  Where-Object { $_.IPv4DefaultGateway -and $_.NetAdapter.Status -eq "Up" } |
  Sort-Object { $_.IPv4DefaultGateway.RouteMetric } |
  Select-Object -First 1).IPv4Address.IPAddress
if (-not $ip) { $ip = $env:COMPUTERNAME }
$ok = $false
try { $ok = (Invoke-WebRequest "http://127.0.0.1:$Port/api/health" -UseBasicParsing -TimeoutSec 10).StatusCode -eq 200 } catch {}

Write-Host ""
if ($ok) {
  Write-Host "ATHENA is running. Open http://${ip}:$Port on any device on your home network."
} else {
  Write-Host "The task started, but ATHENA has not answered yet. Give it a minute, then open http://${ip}:$Port."
}
Write-Host "Make sure Windows calls this network 'Private' (Settings > Network > Properties), or other devices cannot connect."
