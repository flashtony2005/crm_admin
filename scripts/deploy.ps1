# =====================================================================
# coucouya CMS — 一键部署脚本（Windows / PowerShell）
# ---------------------------------------------------------------------
# 流程与 deploy.sh 一致：预检 -> 加载 .env + 配置门禁 -> 构建后端/前端 -> 组装 deploy/
# 用法：
#   pwsh scripts/deploy.ps1
#   $env:ENV_FILE="prod.env"; pwsh scripts/deploy.ps1
# 说明：Windows 上通常作为自托管/开发部署；生产建议用 Linux + deploy.sh。
# =====================================================================
$ErrorActionPreference = "Stop"
$ROOT = Resolve-Path (Join-Path $PSScriptRoot "..")
$EnvFile = if ($env:ENV_FILE) { $env:ENV_FILE } else { ".env" }

Write-Host "==> [1/5] 工具预检"
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Write-Error "需要 Rust 工具链 (cargo)"; exit 1 }
if (-not (Get-Command node  -ErrorAction SilentlyContinue)) { Write-Error "需要 Node.js"; exit 1 }
$pkg = if (Get-Command pnpm -ErrorAction SilentlyContinue) { "pnpm" }
       elseif (Get-Command npm -ErrorAction SilentlyContinue) { "npm" }
       else { Write-Error "需要 pnpm 或 npm"; exit 1 }
if (-not (Get-Command python -ErrorAction SilentlyContinue)) { Write-Error "需要 python（用于配置检查）"; exit 1 }

Write-Host "==> [2/5] 加载 .env 并运行配置检查门禁"
if (Test-Path $EnvFile) {
  Get-Content $EnvFile | ForEach-Object {
    if ($_ -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$') {
      $k = $Matches[1]; $v = $Matches[2].Trim('"').Trim("'")
      if (-not (Test-Path "env:$k")) { Set-Item -Name "env:$k" -Value $v }
    }
  }
  Write-Host "    已加载 $EnvFile"
} else {
  Write-Warning "未找到 $EnvFile，将用内置默认值（生产请先 cp .env.example .env）"
}

python "$ROOT/scripts/check_config.py" --env-file $EnvFile --strict
if ($LASTEXITCODE -ne 0) { Write-Error "配置检查未通过，已中止部署。"; exit 1 }

Write-Host "==> [3/5] 构建后端 (cargo build --release)"
Push-Location (Join-Path $ROOT "server")
cargo build --release
Pop-Location

Write-Host "==> [4/5] 构建前端 ($pkg build)"
Push-Location (Join-Path $ROOT "web")
& $pkg install --prefer-offline
& $pkg run build
Pop-Location

Write-Host "==> [5/5] 组装部署产物 -> deploy/"
if (Test-Path (Join-Path $ROOT "deploy")) { Remove-Item -Recurse -Force (Join-Path $ROOT "deploy") }
$deploy = New-Item -ItemType Directory -Path (Join-Path $ROOT "deploy/dist-app") | Split-Path
Copy-Item (Join-Path $ROOT "server/target/release/cms-server.exe") $deploy
Copy-Item (Join-Path $ROOT "web/dist/*") (Join-Path $deploy "dist-app") -Recurse
if (Test-Path $EnvFile) { Copy-Item $EnvFile (Join-Path $deploy ".env") }

@"
setlocal
if exist "%~dp0.env" ( for /f "tokens=1,* delims==" %%a in (%~dp0.env) do set "%%a=%%b" )
"%~dp0cms-server.exe"
"@ | Set-Content (Join-Path $deploy "start.cmd")

Write-Host ""
Write-Host "✅ 部署产物已生成于 deploy/"
Write-Host "   运行：  cd deploy ; .\start.cmd"
Write-Host "   默认监听 %PORT% (8088)；站点根托管 web/dist，/api 提供后端 API。"
