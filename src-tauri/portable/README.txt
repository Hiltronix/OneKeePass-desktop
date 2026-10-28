OneKeePass - Windows Portable Version
=====================================

This is the portable version of OneKeePass. No installation is required and
the application keeps all of its data inside this folder.

Getting started
---------------
1. Copy or unzip this whole folder to any writable location - for example a
   USB drive or a folder under your Documents.
2. Run OneKeePass.exe

Requirement: Microsoft Edge WebView2 Runtime
--------------------------------------------
OneKeePass uses the WebView2 Evergreen Runtime, which is preinstalled on
current Windows 10 and Windows 11 systems. If OneKeePass does not start and
reports that WebView2 is missing, install it from:

  https://developer.microsoft.com/en-us/microsoft-edge/webview2/

Where your data is kept
-----------------------
Persistent application data - preferences, logs, backups and word lists -
remains inside 'onekeepass-data' next to OneKeePass.exe.

Portable working files and caches are redirected to the separate 'temp'
folder next to the EXE. This is local to the OneKeePass process: it does NOT
change the Windows system TEMP/TMP settings. Deleting 'temp' is generally
safe only after OneKeePass and its WebView2 child processes have exited.
The folder itself is retained, but the contents are automatically removed
after a normal OneKeePass exit. A short-lived invocation of OneKeePass.exe
waits for the main process to end, then retries deletion while WebView2
releases file handles. No other executable, PowerShell, or system task is
installed. Files kept open by external programs may survive deletion attempts.
After a crash or forced termination, stale contents are also cleaned at the
next launch.

KDBX database files are stored wherever you chose to save them. Windows,
security software, crash reporting and third-party viewers/editors may
still create their own files outside the application directory.

Files in this folder
--------------------
  OneKeePass.exe        The application
  .portable             Marker file that enables portable mode - do NOT
                        delete it. Without it the app stores its data under
                        %USERPROFILE%\.onekeepass like the installed version
  _up_\                 Bundled application resources - do NOT delete
  onekeepass-data\      Persistent application data (created on first run)
  temp\
    work\               Temporary attachment copies and OS/Rust temp files
    webview\            WebView2 browser profile, cache and working data
    tauri\              Tauri/plugin managed app cache and working data

Updating to a new version
-------------------------
Download the new portable zip and replace OneKeePass.exe,
and the '_up_' folder. Keep your 'onekeepass-data'
folder as it is.

Notes and limitations
---------------------
- If this folder is in a read-only location (for example 'C:\Program Files'
  or a write-protected USB drive), OneKeePass shows an error and exits.
  Move the folder to a writable location and start it again.
- The 'Recent files' list stores full file paths. If the drive letter of
  your USB drive changes on another computer, entries in that list may not
  open until you open the database file again manually.
- Remote SFTP/WebDAV access and browser extension integration are disabled
  in this Hiltronix Windows edition. Older browser manifests are not removed
  automatically.
- External-viewer attachment copies are kept under temp\\work\\okp_cache.
  They are deleted on lock/close when possible and swept on the next launch
  after a crash. A file open in an editor may resist deletion on Windows.
- In normal shutdown, the app cleans all children of temp while preserving
  the temp folder itself. It retries for up to 30 seconds when WebView2 or
  another program briefly holds files open. The app does not force-close
  external programs or claim secure SSD erasure.
- Cleanup is serialized with application startup. Do not run multiple
  copies from the same portable directory simultaneously.
- Older OneKeePass/WebView2 versions may have left caches in AppData or Windows
  TEMP. This update does not attempt to migrate or delete older files.
- Running portable builds from a shared/writable directory, USB drive or
  filesystem without user-only permissions may expose temporary plaintext
  attachments to other users. Use a private NTFS directory where possible.
