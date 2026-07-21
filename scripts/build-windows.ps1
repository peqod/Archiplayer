$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $repoRoot

. (Join-Path $repoRoot "build-env.ps1")

& npm.cmd run tauri build -- --bundles nsis
if ($LASTEXITCODE -ne 0) {
  exit $LASTEXITCODE
}

$configPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
$config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$installerName = "$($config.productName)_$($config.version)_x64-setup.exe"
$installerPath = Join-Path $repoRoot "src-tauri\target\release\bundle\nsis\$installerName"
$rootInstallerPath = Join-Path $repoRoot $installerName

if (-not (Test-Path -LiteralPath $installerPath)) {
  throw "The Windows installer was not found at '$installerPath'."
}

Copy-Item -LiteralPath $installerPath -Destination $rootInstallerPath -Force
Write-Host "Windows installer copied to $rootInstallerPath"
