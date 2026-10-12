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

/// Keeps ALTR to one running copy: two would each hold their own copy of the accounts and
/// overwrite each other's changes. Returns `false` (after bringing the open window to the
/// front) when ALTR is already running. The named mutex lives as long as the process.
pub fn single_instance() -> bool {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    // SAFETY: null-terminated UTF-16 strings that outlive the calls; the mutex handle is
    // deliberately never closed so it is held until the process exits.
    unsafe {
        let mutex = CreateMutexW(std::ptr::null(), 0, wide("Local\\ALTR").as_ptr());
        if mutex.is_null() || GetLastError() != ERROR_ALREADY_EXISTS {
            return true;
        }
        let window = FindWindowW(std::ptr::null(), wide("ALTR").as_ptr());
        if !window.is_null() {
            ShowWindow(window, SW_RESTORE);
            SetForegroundWindow(window);
        }
    }
    false
}
