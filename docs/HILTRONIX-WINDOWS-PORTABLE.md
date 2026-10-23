# Hiltronix Windows x64 portable build

This describes the Hiltronix-modified Windows edition of OneKeePass v0.27.0. Automatic updates, URL icon downloads, SFTP/WebDAV transport, and browser integration are disabled. Actual runtime network isolation has not yet been verified.

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
3. Validate the Windows-only Tauri override (`src-tauri/tauri.windows-offline.conf.json`) which removes the browser proxy from the build.
4. `cargo fetch --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc`, followed by `yarn tauri build --no-bundle --target x86_64-pc-windows-msvc --config <Windows-only override>`.
5. Portable staging, ZIP creation, file checks, and SHA-256.

Output:

```text
dist/OneKeePass_0.27.0_Windows_x64_Portable.zip
dist/OneKeePass_0.27.0_Windows_x64_Portable.zip.sha256
```

The ZIP includes `OneKeePass.exe`, the `.portable` marker, `README.txt`, and `_up_/resources/public/`. The browser proxy is neither built nor packaged for Windows. The Windows-only Tauri override clears `bundle.externalBin`; other operating systems retain the upstream configuration.

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

Windows remote-storage operations reject calls before transport, including internal save/merge entrypoints. Browser integration and native messaging startup are disabled on Windows. User-requested external URL opening through the default browser remains available. Dependencies and runtime network traffic still require auditing; packaging does not enforce a process firewall.

## GitHub Actions: downloadable Windows ZIP

The workflow [Hiltronix Windows x64 portable ZIP](../.github/workflows/hiltronix-windows-portable.yml) executes the packaging script on a GitHub-hosted Windows 2022 runner with Visual Studio/MSVC, Java, Node, Yarn and Rust.

To download the build without installing development tools locally:

1. Open the [Windows Actions runs](https://github.com/Hiltronix/OneKeePass-desktop/actions/workflows/hiltronix-windows-portable.yml).
2. Open a **successful** run (green checkmark). If the workflow failed, inspect its log rather than using an incomplete artifact.
3. Under **Artifacts**, download the \`Hiltronix-Windows-x64-Portable-...\` ZIP.
4. Extract the downloaded GitHub Actions artifact into a writable folder. The app files are uploaded directly; no second ZIP is required. Keep `OneKeePass.exe`, `.portable`, and `_up_` together.
5. Run `OneKeePass.exe` and test with a disposable KDBX database. Keep the hidden `.portable` marker beside the EXE to enable portable data storage.

The Windows portable build now disables built-in update, URL-icon, SFTP/WebDAV, and browser-integration paths. This is **not yet proof of complete network isolation**; dynamic tests are outstanding.

## Hiltronix desktop usability changes (source revision in progress)

- **Window geometry:** Windows position, normal size and maximized state are stored in the existing `preference.toml`; portable builds keep this in `onekeepass-data`. If the saved monitor is disconnected, the app uses the normal centered startup location.
- **Help/About:** The Windows About item is under Help. The About panel identifies Hiltronix as the fork maintainer and links both to the Hiltronix repository and the original OneKeePass desktop developers.
- **Updates:** The three-second automatic update check and native update-check command are removed. Help → Check for Updates opens a local dialog reading **"Not currently in use."**
- **Custom icons:** Entry and group custom-icon pickers no longer offer a URL download. The Rust command for Internet favicon downloads is unregistered and removed; selecting local image files remains available.
- **Dialogs:** Escape invokes the existing Cancel/Close callback for database settings, application settings, Manage Custom Icons, and Search. The nested browser reconnect prompt has its own Escape callback.
- **Tooltips:** Toolbar buttons are wrapped to support disabled-button tooltips, and hover delay is reduced from 2000 ms to 700 ms.
- **Password Generator:** Escape closes the dialog without copying a password.
- **About dialog:** Escape invokes the same close event as the Close button; following the MUI dialog convention, clicking outside does not close it.
- **Revision:** About shows a source revision between Version and Platform. The value is updated for reviewed source revisions, not for repeated builds of the same commit.
- **Windows offline controls:** SFTP/WebDAV operations are guarded, remote-storage and browser-integration UI entrypoints are hidden/disabled, and browser proxy excluded from portable ZIP.

**Not a network-isolation guarantee:** The broader audit of SFTP/WebDAV, browser integration, any remaining network-capable commands, plugins, and third-party dependencies remains outstanding. These changes do not make a defensible claim that the Windows executable is unable to communicate over the network.

### Acceptance test checklist

With a disposable database, verify all of these on Windows 10:

1. Resize and move the main window, exit normally, and launch again. Check position/size; repeat after maximizing. Disconnect a secondary display (if applicable) and verify the window remains visible.
2. Select **Help → Check for Updates** and confirm the exact offline message, without an HTTP request.
3. Select **Help → About OneKeePass**. Verify both original authors' GitHub link and the Hiltronix fork link open correctly.
4. Open database settings, application settings, Manage Custom Icons, and Search; press Escape. Verify the same outcome as the dialog's Cancel/Close button, including discarding edits.
5. Hover both enabled and disabled toolbar buttons to check consistent tooltip appearance at approximately 700 ms.
6. Open entry and group icon dialogs: verify **Add from File** works and **Add from URL** is absent. Existing icons embedded in KDBX databases must still render.
7. Exercise opening, editing and saving KDBX4 files, and custom templates, using test data only.

