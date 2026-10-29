<#
.SYNOPSIS
    Build Dr Reco with the MSVC toolchain.

.DESCRIPTION
    Runs the Tauri build after loading the Visual Studio developer environment.

    This is required because:
      - rustc does not always detect recent Visual Studio installations (e.g. v18),
        so `link.exe` has to be on PATH;
      - `VsDevCmd.bat` may omit the Windows SDK UCRT include/library directories from
        INCLUDE/LIB on some installations, which makes cl.exe fail on <stddef.h>
        and the linker fail with "LNK1104: cannot open ucrt.lib".

    The MSVC target statically links WebView2Loader (and the VC++ runtime through
    `build.windows.staticVCRuntime`), so the produced executable is self-contained.
#>

[CmdletBinding()]
param(
    # Skip NSIS/MSI packaging: builds the executable only (faster local builds).
    [switch]$NoBundle,
    # Build with the debug profile: no LTO, no strip (much faster, not for release).
    [switch]$DebugBuild
)

$ErrorActionPreference = 'Stop'

function Get-VisualStudioPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) {
        throw "vswhere.exe introuvable. Installez les Microsoft C++ Build Tools : https://aka.ms/vs/stable/vs_BuildTools.exe"
    }

    $path = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $path) {
        throw "Aucune installation de Visual Studio avec les outils C++ n'a été trouvée. Installez la charge de travail « Développement Desktop en C++ »."
    }

    return $path.Trim()
}

function Import-DeveloperEnvironment {
    param([Parameter(Mandatory = $true)][string]$VisualStudioPath)

    $devCmd = Join-Path $VisualStudioPath 'Common7\Tools\VsDevCmd.bat'
    if (-not (Test-Path -LiteralPath $devCmd)) {
        throw "VsDevCmd.bat introuvable dans '$VisualStudioPath'."
    }

    # A temporary batch file avoids cmd/PowerShell quoting issues with paths that
    # contain spaces. "call VsDevCmd" then "set" prints the resulting environment.
    $tempBat = Join-Path $env:TEMP ('dr-reco-vsdev-' + [guid]::NewGuid().ToString('N') + '.bat')
    try {
        $lines = @(
            '@echo off'
            ('call "' + $devCmd + '" -arch=x64 -host_arch=x64 -no_logo')
            'if errorlevel 1 exit /b 1'
            'set'
        )
        Set-Content -LiteralPath $tempBat -Value $lines -Encoding ASCII

        $environment = & $env:ComSpec /c $tempBat
        if ($LASTEXITCODE -ne 0) {
            throw "Échec de l'initialisation de l'environnement Visual Studio (code $LASTEXITCODE)."
        }
    }
    finally {
        Remove-Item -LiteralPath $tempBat -Force -ErrorAction SilentlyContinue
    }

    foreach ($line in $environment) {
        if ($line -match '^([^=]+)=(.*)$') {
            Set-Item -Path ("env:" + $matches[1]) -Value $matches[2]
        }
    }
}

function Add-UcrtPaths {
    # Some Visual Studio installations do not register the UCRT part of the Windows
    # SDK, so VsDevCmd.bat leaves the UCRT include/lib directories out of INCLUDE/LIB.
    # cl.exe then fails on <stddef.h> and link.exe on ucrt.lib.
    $sdkLibRoots = @()
    if ($env:WindowsSdkDir) {
        $sdkLibRoots += (Join-Path $env:WindowsSdkDir 'Lib')
    }
    $sdkLibRoots += (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Lib')

    $candidates = @()
    foreach ($root in $sdkLibRoots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        $candidates += Get-ChildItem -LiteralPath $root -Directory |
            Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'ucrt\x64\ucrt.lib') }
    }

    if (-not $candidates) {
        throw "Bibliothèques UCRT (ucrt.lib) introuvables. Réinstallez le SDK Windows 11 depuis le Visual Studio Installer."
    }

    $sdk = $candidates |
        Sort-Object -Property @{ Expression = { try { [version]$_.Name } catch { [version]'0.0.0.0' } } } -Descending |
        Select-Object -First 1
    $sdkVersion = $sdk.Name

    $ucrtInclude = Join-Path $sdk.FullName '..\..\Include'
    $ucrtInclude = [System.IO.Path]::GetFullPath((Join-Path $ucrtInclude "$sdkVersion\ucrt"))
    $ucrtLib = Join-Path $sdk.FullName 'ucrt\x64'

    $includePaths = @()
    if ($env:INCLUDE) { $includePaths = $env:INCLUDE -split ';' }
    if (-not ($includePaths | Where-Object { $_ -match '\\ucrt\\?$' })) {
        $env:INCLUDE = "$ucrtInclude;$env:INCLUDE"
        Write-Host "SDK UCRT ajouté à INCLUDE : $ucrtInclude"
    }

    $libPaths = @()
    if ($env:LIB) { $libPaths = $env:LIB -split ';' }
    if (-not ($libPaths | Where-Object { $_ -match '\\ucrt\\x64\\?$' })) {
        $env:LIB = "$ucrtLib;$env:LIB"
        Write-Host "SDK UCRT ajouté à LIB : $ucrtLib"
    }
}

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $visualStudio = Get-VisualStudioPath
    Import-DeveloperEnvironment -VisualStudioPath $visualStudio
    Add-UcrtPaths

    if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
        throw "link.exe reste introuvable après l'initialisation de l'environnement Visual Studio."
    }

    $tauriArgs = @('build')
    if ($NoBundle) { $tauriArgs += '--no-bundle' }
    if ($DebugBuild) { $tauriArgs += '--debug' }

    Write-Host "Compilation avec $(rustc --version)"
    & npm run tauri -- @tauriArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Échec du build Tauri (code $LASTEXITCODE)."
    }
}
finally {
    Pop-Location
}
