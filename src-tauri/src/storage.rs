//! Termix secure storage: DPAPI-encrypted local files in %APPDATA%/Termix.
//! SSH library choice: **russh** (+ russh-sftp) — fixed for the whole project.

use std::path::PathBuf;

#[allow(unused_imports)]
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{de::DeserializeOwned, Serialize};

#[cfg(windows)]
mod dpapi {
    use windows::{
        core::PWSTR,
        Win32::{Foundation::LocalFree, Security::Cryptography::*},
    };

    pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
        let blob_in = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut blob_out = CRYPT_INTEGER_BLOB::default();
        // Safety: CryptProtectData only reads blob_in while UI forbidden (no prompt).
        let ok = unsafe {
            CryptProtectData(
                &blob_in,
                PWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut blob_out,
            )
        };
        if ok.is_err() {
            return Err(crate::errors::err_code::DPAPI_PROTECT_FAILED.to_string());
        }
        let slice =
            unsafe { std::slice::from_raw_parts(blob_out.pbData, blob_out.cbData as usize) };
        let out = slice.to_vec();
        unsafe {
            LocalFree(windows::Win32::Foundation::HLOCAL(blob_out.pbData as _));
        }
        Ok(out)
    }

    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
        let blob_in = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut blob_out = CRYPT_INTEGER_BLOB::default();
        let ok = unsafe {
            CryptUnprotectData(
                &blob_in,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut blob_out,
            )
        };
        if ok.is_err() {
            return Err(crate::errors::err_code::DPAPI_UNPROTECT_FAILED.to_string());
        }
        let slice =
            unsafe { std::slice::from_raw_parts(blob_out.pbData, blob_out.cbData as usize) };
        let out = slice.to_vec();
        unsafe {
            LocalFree(windows::Win32::Foundation::HLOCAL(blob_out.pbData as _));
        }
        Ok(out)
    }
}

#[cfg(not(windows))]
mod dpapi {
    // Non-Windows fallback (dev only): base64, NOT secure. Production target is Windows.
    use super::*;
    pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
        Ok(B64.encode(data).into_bytes())
    }
    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
        let s = std::str::from_utf8(data).map_err(|e| e.to_string())?;
        B64.decode(s).map_err(|e| e.to_string())
    }
}

pub fn app_dir() -> PathBuf {
    dirs::data_dir()
        .map(|d| d.join("Termix"))
        .unwrap_or_else(|| PathBuf::from(".termix"))
}

pub fn storage_path() -> String {
    app_dir().to_string_lossy().to_string()
}

fn enc_path(name: &str) -> PathBuf {
    let d = app_dir();
    let _ = std::fs::create_dir_all(&d);
    d.join(name)
}

/// Absolute path of an encrypted storage file (no creation side effects).
pub fn enc_file_path(name: &str) -> PathBuf {
    app_dir().join(name)
}

/// Raw DPAPI-decrypt of an already-read file blob (used by migration).
pub fn decrypt_bytes(enc: &[u8]) -> Result<Vec<u8>, String> {
    dpapi::unprotect(enc)
}

pub fn write_encrypted_json<T: Serialize>(name: &str, value: &T) -> Result<(), String> {
    let json = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let enc = dpapi::protect(&json)?;
    std::fs::write(enc_path(name), enc).map_err(|e| e.to_string())
}

pub fn read_encrypted_json<T: DeserializeOwned + Default>(name: &str) -> T {
    let path = enc_path(name);
    let Ok(enc) = std::fs::read(&path) else {
        return T::default();
    };
    if enc.is_empty() {
        return T::default();
    }
    let Ok(raw) = dpapi::unprotect(&enc) else {
        return T::default();
    };
    serde_json::from_slice(&raw).unwrap_or_default()
}

pub fn write_plain(name: &str, content: &str) -> Result<(), String> {
    let p = enc_path(name);
    std::fs::write(p, content).map_err(|e| e.to_string())
}

pub fn read_plain(name: &str) -> Option<String> {
    std::fs::read_to_string(enc_path(name)).ok()
}
