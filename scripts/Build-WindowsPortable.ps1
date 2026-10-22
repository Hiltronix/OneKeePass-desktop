# Build and package the Windows x64 portable edition from this checkout.
# Requires Windows, Rust/MSVC, Node/Yarn, Java and ClojureScript's build dependencies.
# The .portable marker is mandatory: it redirects application data beside the EXE.

param(
    [string]$OutputDirectory = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Program exited with status $LASTEXITCODE"
    }
}

if ($env:OS -ne "Windows_NT") {
    throw "Run this script on Windows with the MSVC toolchain installed."
}

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Target = "x86_64-pc-windows-msvc"
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $Root "dist"
}

foreach ($Program in @("cargo.exe", "rustc.exe", "yarn.cmd", "java.exe")) {
    if (-not (Get-Command $Program -ErrorAction SilentlyContinue)) {
        throw "Missing prerequisite: $Program"
    }
}

$Configuration = Get-Content -Raw (Join-Path $Root "src-tauri/tauri.conf.json") | ConvertFrom-Json
$Version = $Configuration.version
if (-not $Version -or $Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Unexpected Tauri version: $Version"
}

Push-Location $Root
try {
    Write-Host "==> Installing JavaScript dependencies from yarn.lock"
    Invoke-Checked "yarn.cmd" @("install", "--frozen-lockfile")

    $Shadow = Join-Path $Root "node_modules/.bin/shadow-cljs.cmd"
    if (-not (Test-Path -LiteralPath $Shadow -PathType Leaf)) {
        throw "shadow-cljs.cmd not installed at $Shadow"
    }

    Write-Host "==> Building production ClojureScript"
    $OldOutput = Join-Path $Root "resources/public/cljs-out/dev"
    if (Test-Path -LiteralPath $OldOutput) {
        Remove-Item -LiteralPath $OldOutput -Recurse -Force
    }
    Push-Location (Join-Path $Root "src-cljs")
    try {
        Invoke-Checked $Shadow @("release", "app")
    }
    finally {
        Pop-Location
    }

    Write-Host "==> Compiling browser sidecar required by current Tauri configuration"
    Invoke-Checked "cargo.exe" @(
        "build", "--manifest-path", (Join-Path $Root "onekeepass-proxy/Cargo.toml"),
        "--release", "--locked", "--target", $Target
    )
    $SidecarBuilt = Join-Path $Root "onekeepass-proxy/target/$Target/release/onekeepass-proxy.exe"
    if (-not (Test-Path -LiteralPath $SidecarBuilt -PathType Leaf)) {
        throw "Compiled sidecar missing: $SidecarBuilt"
    }

    $SidecarDirectory = Join-Path $Root "onekeepass-proxy/binaries"
    New-Item -Path $SidecarDirectory -ItemType Directory -Force | Out-Null
    Copy-Item -LiteralPath $SidecarBuilt -Destination (
        Join-Path $SidecarDirectory "onekeepass-proxy-$Target.exe"
    ) -Force

    Write-Host "==> Building unbundled Tauri Windows executable"
    Invoke-Checked "yarn.cmd" @(
        "tauri", "build", "--no-bundle", "--target", $Target, "--", "--locked"
    )

    $Exe = Join-Path $Root "src-tauri/target/$Target/release/OneKeePass.exe"
    if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) {
        throw "Compiled Windows application missing: $Exe"
    }

    New-Item -Path $OutputDirectory -ItemType Directory -Force | Out-Null
    $Name = "OneKeePass_${Version}_Windows_x64_Portable"
    $Stage = Join-Path $OutputDirectory $Name
    $Zip = Join-Path $OutputDirectory "$Name.zip"
    if (Test-Path -LiteralPath $Stage) { Remove-Item -LiteralPath $Stage -Recurse -Force }
    if (Test-Path -LiteralPath $Zip) { Remove-Item -LiteralPath $Zip -Force }
    New-Item -Path $Stage -ItemType Directory | Out-Null

    Copy-Item -LiteralPath $Exe -Destination (Join-Path $Stage "OneKeePass.exe")
    Copy-Item -LiteralPath $SidecarBuilt -Destination (Join-Path $Stage "onekeepass-proxy.exe")
    Copy-Item -LiteralPath (Join-Path $Root "src-tauri/portable/README.txt") -Destination $Stage

    # Maps bundle.resources ../resources/public/... to the path Tauri resolves:
    # $RESOURCE/_up_/resources/public/...
    $ResourceDest = Join-Path $Stage "_up_/resources/public"
    New-Item -Path $ResourceDest -ItemType Directory -Force | Out-Null
    foreach ($Folder in @("icons", "translations", "wordlists")) {
        $From = Join-Path $Root "resources/public/$Folder"
        if (-not (Test-Path -LiteralPath $From -PathType Container)) {
            throw "Bundled resource directory is missing: $From"
        }
        Copy-Item -LiteralPath $From -Destination $ResourceDest -Recurse
    }

    # Do not pre-populate onekeepass-data: retain it across future upgrades.
    New-Item -Path (Join-Path $Stage ".portable") -ItemType File | Out-Null

    Write-Host "==> Creating portable ZIP including the dot-marker file"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::CreateFromDirectory(
        $Stage, $Zip, [System.IO.Compression.CompressionLevel]::Optimal, $false
    )

    $Archive = [System.IO.Compression.ZipFile]::OpenRead($Zip)
    try {
        $Names = @($Archive.Entries | ForEach-Object { $_.FullName })
        foreach ($Required in @(
            ".portable", "OneKeePass.exe", "onekeepass-proxy.exe",
            "_up_/resources/public/icons/", "_up_/resources/public/translations/",
            "_up_/resources/public/wordlists/"
        )) {
            if (-not ($Names | Where-Object { $_ -eq $Required -or $_.StartsWith($Required) })) {
                throw "Portable ZIP is missing required file or resource: $Required"
            }
        }
    }
    finally {
        $Archive.Dispose()
    }

    $Hash = (Get-FileHash -LiteralPath $Zip -Algorithm SHA256).Hash.ToLowerInvariant()
    "$Hash  $Name.zip" | Set-Content -Path "$Zip.sha256" -Encoding ASCII
    Write-Host "Portable archive: $Zip"
    Write-Host "SHA256: $Hash"
    Write-Host "Portable package created. Test application launch and KDBX operations before using real secrets."
}
finally {
    Pop-Location
}
