# Hiltronix Windows x64 portable build

This is a build procedure for the **current, unmodified OneKeePass v0.27.0 feature set** in this fork. It does **not** yet disable updater checks, network requests, browser integration, or remote storage. Do not treat it as the later offline/security-hardened edition.

## Prerequisites

Build on **Windows x64** with:

- Visual Studio 2022 C++ Build Tools (MSVC and Windows SDK), with the compiler environment available to Rust/Botan.
- Rust and Cargo, with the `x86_64-pc-windows-msvc` target.
- Node.js (20 or newer), Yarn Classic 1.22.x, and **JDK 21** (current Closure Compiler requires Java class-file version 65).
- Microsoft Edge WebView2 Evergreen Runtime on the **computer where you run** the portable executable.

The build needs network access the first time to retrieve Cargo, Yarn and ClojureScript dependencies. Inspect/pin the toolchain versions before treating builds as reproducible; `yarn.lock` and `Cargo.lock` constrain packages but do not alone freeze the compiler or downloaded build tools.

## Build

From a PowerShell prompt in the repository root:

```powershell
rustup target add x86_64-pc-windows-msvc
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Build-WindowsPortable.ps1
```

The script runs these phases, aborting on a failed external command:

1. `yarn install --frozen-lockfile`
2. Production `shadow-cljs release app`
3. `cargo build --release --locked` for the required browser sidecar
4. `cargo fetch --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc`, followed by `yarn tauri build --no-bundle --target x86_64-pc-windows-msvc`
5. Portable staging, ZIP creation, file checks, and SHA-256.

Output:

```text
dist/OneKeePass_0.27.0_Windows_x64_Portable.zip
dist/OneKeePass_0.27.0_Windows_x64_Portable.zip.sha256
```

The ZIP includes `OneKeePass.exe`, `onekeepass-proxy.exe`, the `.portable` marker, `README.txt`, and the `_up_/resources/public/` directories configured under Tauri's `bundle.resources`.

The `.portable` marker is important: existing Rust code detects it and saves app preferences, logs and backups in `onekeepass-data` beside the EXE. Never delete your existing `onekeepass-data` folder during an update. The script intentionally does not package that user-data directory.

## Verify before using it with a real password database

1. Extract the ZIP into a writable folder outside `Program Files`.
2. Verify that `.portable` and `_up_/resources/public/` survived ZIP extraction.
3. Launch `OneKeePass.exe` and verify the app creates `onekeepass-data`.
4. Test opening and saving a disposable KDBX4 database. Exercise OneKeePass custom templates, TOTP and attachments.
5. Confirm preferences and logs stay in the portable directory and that reopening the test database works.
6. Test launching on a second Windows 10 machine with WebView2 installed.

**Status:** This procedure is committed but **not compiled or runtime-tested** in the ChatGPT execution environment (no Windows/MSVC, PowerShell or Cargo toolchain available). ZIP assembly and application runtime behavior must be verified on Windows before distributing or trusting the archive.

## Scope of later security work

The current desktop source still contains an online update-checking command and optional networking/browser features. These will be separately audited and restricted after this build baseline is tested. Building a portable ZIP does not itself block Internet connectivity.

## GitHub Actions: downloadable Windows ZIP

The workflow [Hiltronix Windows x64 portable ZIP](../.github/workflows/hiltronix-windows-portable.yml) executes the packaging script on a GitHub-hosted Windows 2022 runner with Visual Studio/MSVC, Java, Node, Yarn and Rust.

To download the build without installing development tools locally:

1. Open the [Windows Actions runs](https://github.com/Hiltronix/OneKeePass-desktop/actions/workflows/hiltronix-windows-portable.yml).
2. Open a **successful** run (green checkmark). If the workflow failed, inspect its log rather than using an incomplete artifact.
3. Under **Artifacts**, download the \`Hiltronix-Windows-x64-Portable-...\` ZIP.
4. Extract the downloaded GitHub Actions artifact into a writable folder. The app files are uploaded directly; no second ZIP is required. Keep `OneKeePass.exe`, `.portable`, and `_up_` together.
5. Run `OneKeePass.exe` and test with a disposable KDBX database. Keep the hidden `.portable` marker beside the EXE to enable portable data storage.

The Windows portable build is **not** yet security-hardened against unwanted network access. The ZIP retains the current application's optional features and updater until they are separately audited and modified.
