//! Windows portable cache cleanup.
//!
//! WebView2 may retain open handles after the Tauri window is destroyed. An
//! in-process shutdown delete is therefore not reliable. A short-lived second
//! invocation of *this same executable* waits for the main process to release
//! a named mutex, then removes the CONTENTS of ./temp (never ./temp itself).
//! The same mutex serializes a new launch against cleanup and prevents erasing
//! a live instance's browser profile. No PowerShell, cmd.exe, or helper EXE.

use crate::app_paths;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs, io,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0},
    System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
};

const HELPER_ARGUMENT: &str = "--hiltronix-portable-temp-cleanup";
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const LOCK_WAIT_MS: u32 = 60_000;
const CLEANUP_RETRY_WINDOW: Duration = Duration::from_secs(30);
const CLEANUP_RETRY_PAUSE: Duration = Duration::from_millis(350);

// The application owns this kernel mutex for its entire lifetime.
// It is intentionally not released until process exit. Abandoned mutexes are
// acquired by the cleanup helper with WAIT_ABANDONED after the owner exits.
static PORTABLE_MUTEX_HANDLE: OnceLock<isize> = OnceLock::new();

fn temp_root() -> Option<PathBuf> {
    if !app_paths::is_portable() {
        return None;
    }
    let executable = std::env::current_exe().ok()?;
    Some(executable.parent()?.join("temp"))
}

fn mutex_name() -> io::Result<Vec<u16>> {
    let executable = std::env::current_exe()?;
    let folder = executable.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Executable has no parent directory")
    })?;
    let normalized = folder.to_string_lossy().to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    let key = format!("Global\\Hiltronix-OneKeePass-Temp-{:x}", digest);
    Ok(key.encode_utf16().chain(std::iter::once(0)).collect())
}

fn acquire_mutex(wait_ms: u32) -> Result<isize, String> {
    let name = mutex_name().map_err(|e| e.to_string())?;
    // SECURITY_ATTRIBUTES is null: normal Windows security defaults.
    // The HANDLE is not inheritable and never passed to the child.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle == 0 {
        return Err(format!("Unable to create portable cleanup mutex: {}", io::Error::last_os_error()));
    }
    let status = unsafe { WaitForSingleObject(handle, wait_ms) };
    if status != WAIT_OBJECT_0 && status != WAIT_ABANDONED {
        unsafe { CloseHandle(handle); }
        return Err(format!("Unable to acquire portable cleanup mutex (wait result {status})"));
    }
    Ok(handle)
}

fn empty_dir_contents(root: &Path) -> io::Result<()> {
    let meta = match fs::symlink_metadata(root) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    // In particular, refuse any attempt to follow a symlink/junction that
    // someone installed in place of ./temp. Never remove the root itself.
    if !meta.is_dir() || (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Portable temp root is not a normal directory",
        ));
    }

    let mut first_error = None;
    for entry in fs::read_dir(root)? {
        match entry {
            Ok(item) => {
                // std::fs::remove_dir_all never follows directory symlinks;
                // it removes the link itself. The root is separately checked.
                let result = item.file_type().and_then(|kind| {
                    if kind.is_dir() {
                        fs::remove_dir_all(item.path())
                    } else {
                        fs::remove_file(item.path())
                    }
                });
                if let Err(e) = result {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Called before initializing Tauri/WebView2 on the normal portable process.
/// A second launch waits until any previous cleaner has finished; two live
/// instances with the same portable directory are deliberately not supported.
pub(crate) fn prepare_app_start() -> Result<(), String> {
    if temp_root().is_none() {
        return Ok(());
    }
    let handle = acquire_mutex(LOCK_WAIT_MS)?;
    if PORTABLE_MUTEX_HANDLE.set(handle).is_err() {
        unsafe { ReleaseMutex(handle); CloseHandle(handle); }
        return Err("Portable temp mutex was already initialized".into());
    }
    // A forced termination/crash cannot launch the post-exit helper. Remove
    // stale files at next startup too, before WebView2 has opened its profile.
    // Windows may still have locked files: leave those, rather than block
    // startup indefinitely or delete the parent directory.
    if let Some(root) = temp_root() {
        let _ = empty_dir_contents(&root);
    }
    Ok(())
}

/// Called on Tauri RunEvent::Exit, after the UI has started shutting down.
/// The child cannot acquire our mutex before this process has fully exited.
pub(crate) fn launch_cleanup_after_exit() {
    if PORTABLE_MUTEX_HANDLE.get().is_none() {
        return; // Installed app, or unsuccessful portable startup.
    }
    if let Ok(executable) = std::env::current_exe() {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if let Err(error) = Command::new(executable)
            .arg(HELPER_ARGUMENT)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
        {
            eprintln!("Unable to launch portable cache cleanup: {error}");
        }
    }
}

/// Recognizes only a fixed internal flag, never a user-supplied delete path.
pub(crate) fn is_cleanup_invocation() -> bool {
    std::env::args_os().nth(1).as_deref() == Some(OsStr::new(HELPER_ARGUMENT))
}

/// Runs without creating a Tauri window or a WebView2 process.
pub(crate) fn run_cleanup_helper() {
    let Some(root) = temp_root() else {
        return;
    };
    let Ok(handle) = acquire_mutex(LOCK_WAIT_MS) else {
        return;
    };
    let end = Instant::now() + CLEANUP_RETRY_WINDOW;
    loop {
        if empty_dir_contents(&root).is_ok() {
            break;
        }
        if Instant::now() >= end {
            break; // Some files may remain held by WebView2 / external viewers.
        }
        thread::sleep(CLEANUP_RETRY_PAUSE);
    }
    unsafe {
        ReleaseMutex(handle);
        CloseHandle(handle);
    }
}

#[cfg(test)]
mod tests {
    use super::empty_dir_contents;

    #[test]
    fn removes_children_and_preserves_root_directory() {
        let root = std::env::temp_dir().join(format!(
            "hiltronix-cleanup-test-{}", uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("webview").join("Cache")).unwrap();
        std::fs::write(root.join("webview").join("Cache").join("file"), b"x").unwrap();
        std::fs::write(root.join("workfile"), b"y").unwrap();
        empty_dir_contents(&root).unwrap();
        assert!(root.is_dir());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir(&root).unwrap();
    }
}
