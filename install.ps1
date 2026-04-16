# efact-printer-agent installer — Windows (PowerShell)
# Usage (run as Administrator or current user):
#   iwr -useb https://raw.githubusercontent.com/nubitio/efact-print-agent/main/install.ps1 | iex
#
# Or save and run:
#   Set-ExecutionPolicy Bypass -Scope Process -Force
#   .\install.ps1
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$REPO    = "nubitio/efact-print-agent"
$BINARY  = "efact-printer-agent.exe"
$ASSET   = "efact-printer-agent-windows-x86_64.zip"

# Install dir — prefer per-user to avoid needing admin
$INSTALL_DIR = "$env:LOCALAPPDATA\efact-printer-agent"
$CONFIG_DIR  = "$env:APPDATA\efact-printer-agent"

function Write-Info  { Write-Host "[efact-printer-agent] $args" -ForegroundColor Cyan }
function Write-Ok    { Write-Host "[efact-printer-agent] $args" -ForegroundColor Green }
function Write-Err   { Write-Host "[efact-printer-agent] $args" -ForegroundColor Red; exit 1 }

# ── latest release tag ────────────────────────────────────────────────────────
Write-Info "Fetching latest release..."
try {
  $release = Invoke-RestMethod "https://api.github.com/repos/$REPO/releases/latest"
  $TAG = $release.tag_name
} catch {
  Write-Err "Could not fetch release info: $_"
}

Write-Info "Latest release: $TAG"

# ── download ──────────────────────────────────────────────────────────────────
$TMP = Join-Path $env:TEMP "efact-printer-agent-install"
New-Item -ItemType Directory -Force -Path $TMP | Out-Null

$DOWNLOAD_URL = "https://github.com/$REPO/releases/download/$TAG/$ASSET"
$ZIP_PATH     = Join-Path $TMP $ASSET

Write-Info "Downloading $ASSET..."
Invoke-WebRequest -Uri $DOWNLOAD_URL -OutFile $ZIP_PATH -UseBasicParsing

Expand-Archive -Path $ZIP_PATH -DestinationPath $TMP -Force

# ── install binary ────────────────────────────────────────────────────────────
New-Item -ItemType Directory -Force -Path $INSTALL_DIR | Out-Null

# Stop any running instance before replacing the binary.
Get-Process "efact-printer-agent" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

Copy-Item (Join-Path $TMP $BINARY) (Join-Path $INSTALL_DIR $BINARY) -Force
Write-Info "Binary installed to $INSTALL_DIR\$BINARY"

# ── default config ────────────────────────────────────────────────────────────
$CONFIG_FILE = Join-Path $CONFIG_DIR "config.toml"
if (-not (Test-Path $CONFIG_FILE)) {
  New-Item -ItemType Directory -Force -Path $CONFIG_DIR | Out-Null
  Copy-Item (Join-Path $TMP "config.toml") $CONFIG_FILE
  Write-Info "Default config written to $CONFIG_FILE"
}

# ── add to PATH (user scope) ──────────────────────────────────────────────────
$USER_PATH = [System.Environment]::GetEnvironmentVariable("PATH", "User")
if ($USER_PATH -notlike "*$INSTALL_DIR*") {
  [System.Environment]::SetEnvironmentVariable(
    "PATH", "$USER_PATH;$INSTALL_DIR", "User"
  )
  Write-Info "Added $INSTALL_DIR to user PATH"
}

# ── autostart via Task Scheduler (current user, no admin required) ────────────
$TASK_NAME = "efact-printer-agent"
$EXE_PATH  = Join-Path $INSTALL_DIR $BINARY

# Remove existing task if present
Stop-ScheduledTask -TaskName $TASK_NAME -ErrorAction SilentlyContinue
Unregister-ScheduledTask -TaskName $TASK_NAME -Confirm:$false -ErrorAction SilentlyContinue

$action  = New-ScheduledTaskAction -Execute $EXE_PATH -WorkingDirectory $INSTALL_DIR
$trigger = New-ScheduledTaskTrigger -AtLogon -User $env:USERNAME
$settings = New-ScheduledTaskSettingsSet `
  -ExecutionTimeLimit (New-TimeSpan -Seconds 0) `
  -RestartCount 3 `
  -RestartInterval (New-TimeSpan -Minutes 1)

Register-ScheduledTask `
  -TaskName $TASK_NAME `
  -Action $action `
  -Trigger $trigger `
  -Settings $settings `
  -RunLevel Limited `
  -Force | Out-Null

# Start it right now without waiting for next logon
Start-ScheduledTask -TaskName $TASK_NAME

Write-Ok "efact-printer-agent $TAG installed successfully."
Write-Ok "Agent running on http://localhost:8765"
Write-Ok "Config: $CONFIG_FILE"
Write-Ok "Logs: $INSTALL_DIR\agent.log"
Write-Ok "Autostart: Task Scheduler task '$TASK_NAME' registered for current user."

# Cleanup
Remove-Item -Recurse -Force $TMP -ErrorAction SilentlyContinue
