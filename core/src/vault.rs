//! Session vault: each account's Riot Client session file, encrypted with DPAPI
//! so only this Windows user on this PC can decrypt it.

use std::{io, path::PathBuf, ptr};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

const ENTROPY: &[u8] = b"ALTR session vault";

/// Only letters, digits and dashes, so an id can never point outside the vault folder.
fn file_name(id: &str) -> String {
    let name: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    format!("{name}.bin")
}

fn path(id: &str) -> PathBuf {
    crate::data_dir().join("vault").join(file_name(id))
}

pub fn save(id: &str, session: &[u8]) -> io::Result<()> {
    crate::write_atomic(&path(id), &protect(session)?)
}

pub fn load(id: &str) -> io::Result<Vec<u8>> {
    unprotect(&std::fs::read(path(id))?)
}

pub fn remove(id: &str) -> io::Result<()> {
    match std::fs::remove_file(path(id)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

pub fn protect(data: &[u8]) -> io::Result<Vec<u8>> {
    dpapi(data, true)
}

pub fn unprotect(data: &[u8]) -> io::Result<Vec<u8>> {
    dpapi(data, false)
}

fn dpapi(data: &[u8], encrypt: bool) -> io::Result<Vec<u8>> {
    let input = blob(data);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
    // SAFETY: all blobs point to live buffers for the duration of the call; the
    // output buffer is allocated by DPAPI, copied, then released with LocalFree.
    unsafe {
        let ok = if encrypt {
            CryptProtectData(&input, ptr::null(), &entropy, ptr::null(), ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        } else {
            CryptUnprotectData(&input, ptr::null_mut(), &entropy, ptr::null(), ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let secret = b"ssid: not-a-real-token";
        let sealed = protect(secret).unwrap();
        assert!(!sealed.windows(secret.len()).any(|w| w == secret));
        assert_eq!(unprotect(&sealed).unwrap(), secret);
    }

    #[test]
    fn tampered_data_is_rejected() {
        let mut sealed = protect(b"hello").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;
        assert!(unprotect(&sealed).is_err());
    }

    #[test]
    fn ids_cannot_escape_the_vault() {
        // Pure: tests must never touch the real %APPDATA% folder.
        assert_eq!(file_name("..\\..\\evil"), "evil.bin");
    }
}
