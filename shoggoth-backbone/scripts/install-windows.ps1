# Shoggoth Mesh Machine — Windows Installer (PowerShell)
#
# Installs the Shoggoth node agent as a Windows service using Docker Desktop.
# For RTX 5090/4090 Windows machines serving as edge GPU nodes.
#
# Usage (PowerShell as Administrator):
#   Invoke-WebRequest https://raw.githubusercontent.com/chainchopper/shoggoth-backbone/main/scripts/install-windows.ps1 | Invoke-Expression
#   .\install-windows.ps1 -OrchestratorAddr "192.168.1.100:9100"

param(
    [string]$OrchestratorAddr = "localhost:9100",
    [string]$InstallDir = "$env:ProgramData\shoggoth"
)

Write-Host "╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║     SHOGGOTH MESH MACHINE — Windows Installer               ║" -ForegroundColor Green
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Green
Write-Host ""

# ── 1. Check Prerequisites ────────────────────────────────────────────────────
Write-Host "[1/4] Checking prerequisites..." -ForegroundColor Yellow

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    Write-Host "Docker Desktop is required. Install from: https://www.docker.com/products/docker-desktop/"
    exit 1
}

# Check GPU
$gpu = Get-WmiObject Win32_VideoController | Where-Object { $_.Name -match "NVIDIA|AMD" }
if (-not $gpu) {
    Write-Host "Warning: No NVIDIA/AMD GPU detected. Shoggoth requires a GPU." -ForegroundColor Yellow
} else {
    Write-Host "  GPU: $($gpu.Name)" -ForegroundColor Green
}

# ── 2. Create directories ─────────────────────────────────────────────────────
Write-Host "[2/4] Creating installation directory..." -ForegroundColor Yellow
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Write-Host "  $InstallDir" -ForegroundColor Green

# ── 3. Pull image ─────────────────────────────────────────────────────────────
Write-Host "[3/4] Pulling Shoggoth node agent image..." -ForegroundColor Yellow
docker pull ghcr.io/chainchopper/shoggoth-node-agent:latest
Write-Host "  Image pulled" -ForegroundColor Green

# ── 4. Create Windows scheduled task ──────────────────────────────────────────
Write-Host "[4/4] Creating Windows scheduled task..." -ForegroundColor Yellow

$action = New-ScheduledTaskAction -Execute "docker" `
    -Argument "run --rm --name shoggoth-node-agent --network host --privileged --gpus all -v C:\Windows\System32\DriverStore\FileRepository\nv_disp*\nvlddmkm.sys:C:\Windows\System32\DriverStore\FileRepository\nv_disp*\nvlddmkm.sys:ro -e SHOGGOTH_NODE_ID=$env:COMPUTERNAME -e SHOGGOTH_ORCHESTRATOR_ADDR=$OrchestratorAddr -e RUST_LOG=shoggoth=info ghcr.io/chainchopper/shoggoth-node-agent:latest"

$trigger = New-ScheduledTaskTrigger -AtStartup
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable -RestartCount 5 -RestartInterval (New-TimeSpan -Minutes 1)

Register-ScheduledTask -TaskName "Shoggoth Node Agent" -Action $action -Trigger $trigger -Settings $settings -RunLevel Highest -Force | Out-Null

Start-ScheduledTask -TaskName "Shoggoth Node Agent"

Write-Host ""
Write-Host "╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║     INSTALLATION COMPLETE                                    ║" -ForegroundColor Green
Write-Host "╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Green
Write-Host ""
Write-Host "  Node agent running as scheduled task."
Write-Host "  To uninstall: Unregister-ScheduledTask -TaskName 'Shoggoth Node Agent' -Confirm:`$false"
