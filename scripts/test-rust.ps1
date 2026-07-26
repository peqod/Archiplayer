# Runs the Rust unit tests on Windows. Forward extra args to cargo, e.g.
#   ./scripts/test-rust.ps1 wfmu::
#
# Two toolchain problems block a plain `cargo test` under x86_64-pc-windows-gnu.
# Neither is a problem with the code:
#
#   1. `cargo test` without a target filter links the cdylib, and mingw's ld fails
#      with "export ordinal too large". The unit tests live in the lib, so --lib
#      skips that link entirely.
#   2. A cargo test harness has no application manifest, so the loader binds the
#      legacy comctl32 v5 in System32. rfd (via tauri-plugin-dialog) imports
#      TaskDialogIndirect, which only comctl32 v6 exports, and the process dies at
#      startup with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139). Embedding a manifest
#      that requests Common-Controls 6.0.0.0 makes the loader pick v6.
#
# The MSVC toolchain sidesteps both, but it needs a Windows SDK for windows.h.
#
# Note: this sets RUSTFLAGS, so cargo fingerprints these artifacts separately from
# a plain `cargo build`. Alternating between the two rebuilds the dependency graph.

$ErrorActionPreference = "Stop"

$root = Split-Path $PSScriptRoot -Parent
$work = Join-Path $root "src-tauri\target\test-manifest"
New-Item -ItemType Directory -Force -Path $work | Out-Null

Set-Content -Path (Join-Path $work "test.manifest") -Encoding UTF8 -Value @'
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="amd64" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
'@
Set-Content -Path (Join-Path $work "test.rc") -Encoding UTF8 -Value '1 24 "test.manifest"'

$windres = (Get-Command windres.exe -ErrorAction SilentlyContinue).Source
if (-not $windres) {
    throw "windres.exe not found on PATH. Install the mingw toolchain, or use an MSVC toolchain with a Windows SDK."
}

Push-Location $work
try {
    & $windres "test.rc" "test_manifest.o"
    if ($LASTEXITCODE -ne 0) { throw "windres failed with exit code $LASTEXITCODE" }
}
finally {
    Pop-Location
}

$env:RUSTFLAGS = "-C link-arg=$(Join-Path $work 'test_manifest.o')"
cargo test --manifest-path (Join-Path $root "src-tauri\Cargo.toml") --lib @args
exit $LASTEXITCODE
