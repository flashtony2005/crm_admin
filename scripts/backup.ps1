# ============================================================
#  crm_admin SQLite backup (run on YOUR local machine)
#  Usage:
#    powershell -ExecutionPolicy Bypass -File scripts\backup.ps1
#    powershell -ExecutionPolicy Bypass -File scripts\backup.ps1 -Keep 15
#  - Prefers `sqlite3 .backup` (consistent snapshot even while
#    cms-server is running). Falls back to raw file copy of the
#    db + -wal/-shm (most consistent when the server is stopped).
#  - Output: server\backups\<db>_<timestamp>.db
#  - Prunes old backups, keeping the newest $Keep per source db.
# ============================================================
param([int]$Keep = 30)

$ErrorActionPreference = "Stop"
$root      = Split-Path -Parent $PSScriptRoot
$serverDir = Join-Path $root "server"
$backupDir = Join-Path $serverDir "backups"
New-Item -ItemType Directory -Force -Path $backupDir | Out-Null

$stamp   = Get-Date -Format "yyyyMMdd_HHmmss"
$targets = @("cms_live.db", "cms.db")
$sqlite3 = Get-Command sqlite3 -ErrorAction SilentlyContinue

foreach ($name in $targets) {
    $src = Join-Path $serverDir $name
    if (-not (Test-Path $src)) { Write-Host "skip (not found): $name"; continue }

    $dest = Join-Path $backupDir ("{0}_{1}.db" -f [IO.Path]::GetFileNameWithoutExtension($name), $stamp)

    if ($sqlite3) {
        & sqlite3 $src ".backup '$dest'" 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path $dest)) {
            Write-Host "backup (sqlite3 online): $dest"
            continue
        }
        Write-Warning "sqlite3 backup failed, falling back to file copy"
    }

    # Fallback: raw copy (run while cms-server is stopped for a consistent snapshot)
    Copy-Item $src $dest -Force
    foreach ($ext in @("-wal", "-shm")) {
        $side = Join-Path $serverDir ($name + $ext)
        if (Test-Path $side) { Copy-Item $side ($dest + $ext) -Force }
    }
    Write-Host "backup (file copy): $dest"
}

# Prune: keep the newest $Keep backups per source db (grouped by name prefix)
$groups = @{}
Get-ChildItem $backupDir -Filter "*.db" | ForEach-Object {
    $key = $_.Name
    $idx = $key.IndexOf("_20")
    if ($idx -gt 0) { $key = $key.Substring(0, $idx) }
    if (-not $groups.ContainsKey($key)) { $groups[$key] = @() }
    $groups[$key] += $_
}
foreach ($key in $groups.Keys) {
    $old = $groups[$key] | Sort-Object LastWriteTime -Descending | Select-Object -Skip $Keep
    foreach ($f in $old) { Remove-Item $f.FullName -Force; Write-Host ("pruned: " + $f.Name) }
}

Write-Host "done. backups in: $backupDir"
