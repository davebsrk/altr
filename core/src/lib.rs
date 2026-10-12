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
