# Selects the Rust MSVC toolchain and loads the MSVC linker and Windows SDK
# environment for Rust/Tauri.
# Dot-source this file so the variables remain in the current shell:
#   . .\build-env.ps1

$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
  throw "build-env.ps1 is only needed on Windows."
}

$rustToolchain = "stable-x86_64-pc-windows-msvc"
if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
  throw "rustup was not found. Install Rust from https://rustup.rs/."
}
$installedToolchains = & rustup toolchain list
if ($LASTEXITCODE -ne 0) {
  throw "Could not read the installed Rust toolchains."
}
if (-not ($installedToolchains | Where-Object { $_ -match "^$([regex]::Escape($rustToolchain))(\s|$)" })) {
  throw "The Rust MSVC toolchain is not installed. Run 'rustup toolchain install $rustToolchain'."
}
$env:RUSTUP_TOOLCHAIN = $rustToolchain

$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path -LiteralPath $vswhere)) {
  throw "Visual Studio Build Tools were not found. Install 'Desktop development with C++'."
}

$vsRoot = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vsRoot) {
  throw "MSVC x64 tools were not found. Modify Visual Studio Build Tools and add 'Desktop development with C++'."
}

$msvc = Get-ChildItem (Join-Path $vsRoot "VC\Tools\MSVC") -Directory |
  Sort-Object { [version]$_.Name } -Descending |
  Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName "bin\HostX64\x64\link.exe") } |
  Select-Object -First 1
if (-not $msvc) {
  throw "link.exe was not found in the Visual Studio Build Tools installation."
}

# Prefer a normally installed Windows 10/11 SDK. This machine may instead have the
# same headers and libraries extracted under ~/.winsdk, which is supported as a fallback.
$sdkRoot = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10"
$sdkVersion = $null
$sdkInclude = $null
$sdkLib = $null
$sdkBin = $null

if (Test-Path -LiteralPath (Join-Path $sdkRoot "Include")) {
  $sdkVersion = Get-ChildItem (Join-Path $sdkRoot "Include") -Directory |
    Sort-Object { [version]$_.Name } -Descending |
    Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName "um\Windows.h") } |
    Select-Object -First 1
  if ($sdkVersion) {
    $sdkVersion = $sdkVersion.Name
    $sdkInclude = Join-Path $sdkRoot "Include\$sdkVersion"
    $sdkLib = Join-Path $sdkRoot "Lib\$sdkVersion"
    $sdkBin = Join-Path $sdkRoot "bin\$sdkVersion\x64"
  }
}

if (-not $sdkVersion) {
  $fallback = Join-Path $env:USERPROFILE ".winsdk"
  $includeRoot = Join-Path $fallback "headers\c\Include"
  if (Test-Path -LiteralPath $includeRoot) {
    $sdkVersion = Get-ChildItem $includeRoot -Directory |
      Sort-Object { [version]$_.Name } -Descending |
      Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName "um\Windows.h") } |
      Select-Object -First 1
    if ($sdkVersion) {
      $sdkVersion = $sdkVersion.Name
      $sdkInclude = Join-Path $includeRoot $sdkVersion
      $sdkLib = Join-Path $fallback "libs\c"
      $sdkBin = Join-Path $fallback "headers\c\bin\$sdkVersion\x64"
    }
  }
}

if (-not $sdkVersion) {
  throw "A Windows SDK was not found. Add a Windows 10/11 SDK in Visual Studio Installer."
}

$env:INCLUDE = @(
  (Join-Path $msvc.FullName "include"),
  (Join-Path $sdkInclude "ucrt"),
  (Join-Path $sdkInclude "shared"),
  (Join-Path $sdkInclude "um"),
  (Join-Path $sdkInclude "winrt"),
  (Join-Path $sdkInclude "cppwinrt")
) -join ";"

$env:LIB = @(
  (Join-Path $msvc.FullName "lib\x64"),
  (Join-Path $sdkLib "ucrt\x64"),
  (Join-Path $sdkLib "um\x64")
) -join ";"

$msvcBin = Join-Path $msvc.FullName "bin\HostX64\x64"
$env:PATH = "$msvcBin;$sdkBin;$env:PATH"

Write-Host "Rust $rustToolchain + MSVC $($msvc.Name) + Windows SDK $sdkVersion environment loaded (x64)."
