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

# Reject reintroduction of legacy renderer-callable security bypasses.
$MainSource = Get-Content -Raw (Join-Path $Root "src-tauri/src/main.rs")
foreach ($ForbiddenCommand in @(
    "commands::unlock_kdbx_on_biometric_authentication,",
    "commands::authenticate_with_biometric,",
    "commands::svg_file,"
)) {
    if ($MainSource.Contains($ForbiddenCommand)) {
        throw "Unsafe/deprecated Tauri command remains registered: $ForbiddenCommand"
    }
}
# Keep the authenticated macOS path but exclude biometric unlock from Windows
# both at registration and function-compilation level.
if ($MainSource -notmatch '(?s)#\[cfg\(target_os = "macos"\)\]\s*commands::unlock_kdbx_with_biometric,') {
    throw "Biometric unlock registration must be macOS-only."
}
$CommandSource = Get-Content -Raw (Join-Path $Root "src-tauri/src/commands.rs")
if ($CommandSource -notmatch '(?s)#\[cfg\(target_os = "macos"\)\]\s*#\[command\]\s*pub\(crate\) async fn unlock_kdbx_with_biometric') {
    throw "Biometric unlock implementation must be macOS-only."
}
$BiometricSource = Get-Content -Raw (Join-Path $Root "src-tauri/src/biometric.rs")
$CargoSource = Get-Content -Raw (Join-Path $Root "src-tauri/Cargo.toml")
$FrontendConstants = Get-Content -Raw (Join-Path $Root "src-cljs/main/onekeepass/frontend/constants.cljs")
if ($BiometricSource.Contains("UserConsentVerifier") -or
    $BiometricSource.Contains("windows_hello") -or
    $CargoSource.Contains('"Security_Credentials_UI"') -or
    $FrontendConstants.Contains('WindowsHello')) {
    throw "Windows Hello code, capability or UI marker is still present."
}

$Configuration = Get-Content -Raw (Join-Path $Root "src-tauri/tauri.conf.json") | ConvertFrom-Json
# WebView2's built-in "Saved Info" / form Suggestions popup can ignore
# HTML autocomplete=off. Disable general autofill at the browser level.
foreach ($AppWindow in $Configuration.app.windows) {
    if ($AppWindow.generalAutofillEnabled -ne $false) {
        throw "WebView2 general autofill must be explicitly disabled on every window."
    }
}
# Both potentially sensitive local imports must retain bounded reads.
# The attachment-viewer open command must require an active registered path;
# mere containment in okp_cache is not authorization to launch a file.
$AttachmentSource = Get-Content -Raw (Join-Path $Root "src-tauri/src/commands.rs")
if ($AttachmentSource -notmatch '(?s)fn open_attachment_temp_file.*?attachment_is_active' -or
    $AttachmentSource -notmatch 'active:\s*false') {
    throw "The temp attachment viewer registration/lock revocation guard is missing."
}
$FileUtilSource = Get-Content -Raw (Join-Path $Root "src-tauri/src/file_util.rs")
$CommandReadSource = Get-Content -Raw (Join-Path $Root "src-tauri/src/commands.rs")
if ($FileUtilSource -notmatch 'fn read_bounded_file' -or
    $CommandReadSource -notmatch '(?s)fn add_custom_icon_from_file.*?read_bounded_file' -or
    $CommandReadSource -notmatch '(?s)fn read_text_file.*?read_bounded_file') {
    throw "Local SSH-key and custom-icon imports must use the bounded reader."
}
$SearchSource = Get-Content -Raw (Join-Path $Root "src-cljs/main/onekeepass/frontend/search.cljs")
if ($SearchSource -notmatch ':autoComplete\s+"off"') {
    throw "Search field does not explicitly disable HTML autocomplete."
}
# Search is an always-present toolbar control. Never restore the modal dialog.
$ToolbarSource = Get-Content -Raw (Join-Path $Root "src-cljs/main/onekeepass/frontend/tool_bar.cljs")
$SearchEventsSource = Get-Content -Raw (Join-Path $Root "src-cljs/main/onekeepass/frontend/events/search.cljs")
$KeyboardSource = Get-Content -Raw (Join-Path $Root "src-cljs/main/onekeepass/frontend/keyboard_shortcuts.cljs")
$TauriEventsSource = Get-Content -Raw (Join-Path $Root "src-cljs/main/onekeepass/frontend/events/tauri_events.cljs")
$CommonEventsSource = Get-Content -Raw (Join-Path $Root "src-cljs/main/onekeepass/frontend/events/common.cljs")
if ($ToolbarSource -notmatch '\[search/toolbar-search\]' -or
    $ToolbarSource.Contains('search/search-dialog-main') -or
    $SearchSource -notmatch 'mui-popper' -or
    $SearchSource -notmatch 'toolbar-search-input' -or
    $KeyboardSource -notmatch 'search-events/focus-search-field' -or
    $TauriEventsSource -notmatch 'search-events/focus-search-field' -or
    $SearchEventsSource -notmatch 'eligible-request\?' -or
    $CommonEventsSource -notmatch 'dissoc :entry-list-otp :search') {
    throw "Permanent toolbar search, keyboard focus or lock-safe results guard is missing."
}
# The Windows EXE must clean portable WebView2/working files after shutdown.
$TempCleanupSource = Get-Content -Raw (Join-Path $Root "src-tauri/src/portable_temp.rs")
$MainLifecycleSource = Get-Content -Raw (Join-Path $Root "src-tauri/src/main.rs")
if ($TempCleanupSource -notmatch 'fn run_cleanup_helper' -or
    $TempCleanupSource -notmatch 'fn empty_dir_contents' -or
    $TempCleanupSource -notmatch 'fn launch_cleanup_after_exit' -or
    $TempCleanupSource -notmatch 'fn prepare_app_start' -or
    $MainLifecycleSource -notmatch 'portable_temp::is_cleanup_invocation' -or
    $MainLifecycleSource -notmatch 'portable_temp::prepare_app_start' -or
    $MainLifecycleSource -notmatch 'portable_temp::launch_cleanup_after_exit' -or
    $MainLifecycleSource -notmatch 'tauri::RunEvent::Exit =>') {
    throw "Portable post-exit temp cleanup, startup coordination or root-preservation guard is missing."
}
# Portable temp paths must be configured before Tauri/WebView2 initialization.
$PathSource = Get-Content -Raw (Join-Path $Root "src-tauri/src/app_paths.rs")
$MainPortableSource = Get-Content -Raw (Join-Path $Root "src-tauri/src/main.rs")
if ($PathSource -notmatch 'fn configure_portable_temp' -or
    $PathSource -notmatch 'set_var\("TEMP"' -or
    $PathSource -notmatch 'set_var\("TMP"' -or
    $PathSource -notmatch 'set_var\("WEBVIEW2_USER_DATA_FOLDER"' -or
    $MainPortableSource -notmatch 'AppDirectoriesOverride::Root' -or
    $MainPortableSource -notmatch 'configure_portable_temp') {
    throw "Portable TEMP/TMP, WebView2 and Tauri cache redirection must remain enabled."
}
$Version = $Configuration.version
$BrowserArgs = $Configuration.app.windows[0].additionalBrowserArgs
# Prevent redundant Tauri default grants from widening UI authority.
$Permissions = @(Get-Content -Raw (Join-Path $Root "src-tauri/capabilities/migrated.json") | ConvertFrom-Json).permissions
foreach ($Unnecessary in @("shell:default", "process:default", "shell:allow-execute")) {
    if ($Permissions -contains $Unnecessary) { throw "Unsafe/redundant capability: $Unnecessary" }
}
foreach ($Required in @("shell:allow-open", "process:allow-exit")) {
    if ($Permissions -notcontains $Required) { throw "Missing required capability: $Required" }
}

# Ensure the release continues to carry the per-application WebView2 network restrictions.
foreach ($Flag in @("--disable-background-networking", "--disable-component-update", "--disable-sync", "--proxy-server=http://127.0.0.1:9")) {
    if (-not $BrowserArgs -or -not $BrowserArgs.Contains($Flag)) {
        throw "Missing Windows WebView2 restriction: $Flag"
    }
}
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

    # Windows deliberately has no browser-native-messaging sidecar.
    # The build-only config clears externalBin, without changing macOS/Linux
    # packaging or the shared Tauri configuration.
    $OfflineTauriConfig = Join-Path $Root "src-tauri/tauri.windows-offline.conf.json"
    if (-not (Test-Path -LiteralPath $OfflineTauriConfig -PathType Leaf)) {
        throw "Missing Windows offline Tauri config: $OfflineTauriConfig"
    }

    Write-Host "==> Validating the desktop Cargo.lock and fetching pinned dependencies"
    Invoke-Checked "cargo.exe" @(
        "fetch", "--locked", "--manifest-path", (Join-Path $Root "src-tauri/Cargo.toml"),
        "--target", $Target
    )

    Write-Host "==> Building unbundled Tauri Windows executable"
    Invoke-Checked "yarn.cmd" @(
        "tauri", "build", "--no-bundle", "--target", $Target,
        "--config", $OfflineTauriConfig
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
            ".portable", "OneKeePass.exe",
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

    if ($Names -contains "onekeepass-proxy.exe") {
        throw "The browser native-messaging executable must not ship in the Windows portable ZIP."
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
