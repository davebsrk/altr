pub mod accounts;
pub mod config;
pub mod riot;
pub mod switcher;
pub mod vault;

use std::{fs, io, path::Path, path::PathBuf};

/// `%APPDATA%\ALTR`, where config, accounts and the session vault live.
pub fn data_dir() -> PathBuf {
    let app_data = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let dir = app_data.join("ALTR");
    // The app was called ValoLauncher before; carry its accounts and settings over once.
    // Its saved sessions were sealed under the old name and can't be decrypted any more,
    // so they are dropped: each account just needs saving again with +.
    let old = app_data.join("ValoLauncher");
    if !dir.exists() && old.exists() && fs::rename(&old, &dir).is_ok() {
        let _ = fs::remove_dir_all(dir.join("vault"));
    }
    dir
}

/// Writes via a temp file + rename so a crash never leaves a half-written file behind.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(tmp, path)
}

/// The running copy's "show your window" event (a HANDLE stored as an integer).
static SHOW_EVENT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

/// Keeps ALTR to one running copy: two would each hold their own copy of the accounts and
/// overwrite each other's changes. Returns `false` when ALTR is already running, after asking
/// that copy to show its window (it may be hidden in the tray). The named mutex and event
/// live as long as the process.
pub fn single_instance() -> bool {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent};
    use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    // Dev builds get their own names so they run next to the installed ALTR.
    let (mutex_name, event_name) =
        if cfg!(debug_assertions) { ("Local\\ALTR-dev", "Local\\ALTR-dev-show") } else { ("Local\\ALTR", "Local\\ALTR-show") };
    // SAFETY: null-terminated UTF-16 names that outlive the calls; the handles are
    // deliberately never closed so they are held until the process exits.
    unsafe {
        let mutex = CreateMutexW(std::ptr::null(), 0, wide(mutex_name).as_ptr());
        let first = mutex.is_null() || GetLastError() != ERROR_ALREADY_EXISTS;
        // Auto-reset: each signal is seen once by `show_requested`.
        let event = CreateEventW(std::ptr::null(), 0, 0, wide(event_name).as_ptr());
        if first {
            let _ = SHOW_EVENT.set(event as usize);
        } else {
            // We hold the foreground (the user just launched us): let the running copy take it.
            AllowSetForegroundWindow(ASFW_ANY);
            SetEvent(event);
        }
        first
    }
}

/// Whether a second launch has asked this copy to show its window since the last call.
pub fn show_requested() -> bool {
    use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    // SAFETY: the handle comes from CreateEventW in `single_instance` and is never closed.
    SHOW_EVENT.get().is_some_and(|&event| unsafe { WaitForSingleObject(event as HANDLE, 0) == WAIT_OBJECT_0 })
}
