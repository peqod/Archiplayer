# Sets MSVC + NuGet-provided Windows SDK env for cargo/rustc on this machine.
# The system has VS 2022 BuildTools (MSVC) but no installed Windows SDK, so the SDK
# headers/libs come from NuGet packages extracted under C:\Users\peqod\.winsdk.
# Dot-source before building:  . .\build-env.ps1

$msvc = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207"
$sdkInc = "C:\Users\peqod\.winsdk\headers\c\Include\10.0.28000.0"
$sdkLib = "C:\Users\peqod\.winsdk\libs\c"
$sdkBin = "C:\Users\peqod\.winsdk\headers\c\bin\10.0.28000.0\x64"

$env:INCLUDE = @(
  "$msvc\include",
  "$sdkInc\ucrt",
  "$sdkInc\shared",
  "$sdkInc\um",
  "$sdkInc\winrt",
  "$sdkInc\cppwinrt"
) -join ";"

$env:LIB = @(
  "$msvc\lib\x64",
  "$sdkLib\ucrt\x64",
  "$sdkLib\um\x64"
) -join ";"

$env:PATH = "$msvc\bin\HostX64\x64;$sdkBin;$env:PATH"

Write-Host "MSVC + Windows SDK 10.0.28000.0 env set (x64)."
