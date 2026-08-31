# ============================================================
#  crm_admin one-click dev launcher (run on YOUR local machine)
#    [1/4] stop running cms-server (Windows cannot overwrite a running exe)
#    [2/4] cargo build  (incremental, warm CARGO_TARGET_DIR=tgt_build4)
#          skip with:  powershell -ExecutionPolicy Bypass -File scripts\dev.ps1 -NoBuild
#    [3/4] start backend  http://127.0.0.1:8088  (own window)
#    [4/4] start frontend http://localhost:5188  (own window, /api -> 8088)
#  MSVC env mirrors server\build_sections.cmd.
# ============================================================
param([switch]$NoBuild)

$ErrorActionPreference = "Stop"
$root   = Split-Path -Parent $PSScriptRoot
$server = Join-Path $root "server"
$web    = Join-Path $root "web"

# ── MSVC toolchain (same as build_sections.cmd) ──
$env:MSVC   = "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.43.34808"
$env:SDK    = "C:\Program Files (x86)\Windows Kits\10"
$env:SDKVER = "10.0.26100.0"
$env:INCLUDE = "$env:MSVC\include;$env:SDK\Include\$env:SDKVER\ucrt;$env:SDK\Include\$env:SDKVER\um;$env:SDK\Include\$env:SDKVER\shared;$env:SDK\Include\$env:SDKVER\winrt;$env:SDK\Include\$env:SDKVER\cppwinrt"
$env:LIB     = "$env:MSVC\lib\x64;$env:SDK\Lib\$env:SDKVER\ucrt\x64;$env:SDK\Lib\$env:SDKVER\um\x64"
$env:VCToolsInstallDir = $env:MSVC
$env:WindowsSdkDir     = $env:SDK
$env:UCRTVersion       = $env:SDKVER
$env:CARGO_TARGET_DIR  = Join-Path $server "tgt_build4"

Write-Host "[1/4] Stopping old cms-server ..."
Get-Process cms-server -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2

if (-not $NoBuild) {
    Write-Host "[2/4] Building backend (incremental) ..."
    Push-Location $server
    cargo build
    $ok = ($LASTEXITCODE -eq 0)
    Pop-Location
    if (-not $ok) { throw "cargo build failed - nothing was started." }
} else {
    Write-Host "[2/4] Skipped build (-NoBuild)"
}

$exe = Join-Path $env:CARGO_TARGET_DIR "debug\cms-server.exe"
if (-not (Test-Path $exe)) { throw "cms-server.exe not found: $exe" }

Write-Host "[3/4] Starting backend :8088 ..."
Start-Process -WorkingDirectory $server -FilePath $exe

Write-Host "[4/4] Starting frontend :5188 ..."
Start-Process -WorkingDirectory $web -FilePath "cmd.exe" -ArgumentList "/c", "pnpm dev"

Write-Host ""
Write-Host "DONE."
Write-Host "  backend  : http://127.0.0.1:8088"
Write-Host "  frontend : http://localhost:5188   (/api proxied to 8088)"
