//! Registry access through `RegGetValueW` / `RegSetKeyValueW`, replacing
//! the old `reg.exe` calls and their text parsing.

use std::ptr::null_mut;

use winapi::shared::minwindef::{DWORD, HKEY};
use winapi::shared::winerror::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS};
use winapi::um::winnt::REG_BINARY;
use winapi::um::winreg::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_ANY, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegGetValueW, RegSetKeyValueW,
};

use crate::error::{AppError, Result};
use crate::platform::{from_wide, wide};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    LocalMachine,
    CurrentUser,
}

impl Hive {
    fn hkey(self) -> HKEY {
        match self {
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
            Hive::CurrentUser => HKEY_CURRENT_USER,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Hive::LocalMachine => "HKLM",
            Hive::CurrentUser => "HKCU",
        }
    }
}

#[allow(clippy::cast_sign_loss)]
fn status_code(rc: i32) -> u32 {
    rc as u32
}

/// Reads a value's raw bytes. `Ok(None)` if the key or value doesn't exist.
fn get_raw(hive: Hive, subkey: &str, value: &str, flags: DWORD) -> Result<Option<Vec<u8>>> {
    let (k, v) = (wide(subkey), wide(value));
    let context = || format!(r"reading {}\{subkey}\{value}", hive.name());
    // Retry in case the value grows between the size query and the read.
    for _ in 0..3 {
        let mut size: DWORD = 0;
        let rc = unsafe { RegGetValueW(hive.hkey(), k.as_ptr(), v.as_ptr(), flags, null_mut(), null_mut(), &mut size) };
        match status_code(rc) {
            ERROR_SUCCESS => {}
            ERROR_FILE_NOT_FOUND => return Ok(None),
            code => return Err(AppError::win32(context(), code)),
        }
        let mut buf = vec![0u8; size as usize];
        let rc = unsafe {
            RegGetValueW(hive.hkey(), k.as_ptr(), v.as_ptr(), flags, null_mut(), buf.as_mut_ptr().cast(), &mut size)
        };
        match status_code(rc) {
            ERROR_SUCCESS => {
                buf.truncate(size as usize);
                return Ok(Some(buf));
            }
            ERROR_FILE_NOT_FOUND => return Ok(None),
            ERROR_MORE_DATA => {}
            code => return Err(AppError::win32(context(), code)),
        }
    }
    Err(AppError::win32(context(), ERROR_MORE_DATA))
}

/// Whether a value of any type exists.
pub fn value_exists(hive: Hive, subkey: &str, value: &str) -> Result<bool> {
    Ok(get_raw(hive, subkey, value, RRF_RT_ANY)?.is_some())
}

pub fn read_binary(hive: Hive, subkey: &str, value: &str) -> Result<Option<Vec<u8>>> {
    get_raw(hive, subkey, value, RRF_RT_REG_BINARY)
}

pub fn read_string(hive: Hive, subkey: &str, value: &str) -> Result<Option<String>> {
    Ok(get_raw(hive, subkey, value, RRF_RT_REG_SZ)?.map(|bytes| {
        let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).collect();
        from_wide(&units)
    }))
}

/// Writes a `REG_BINARY` value, creating the key if needed.
pub fn write_binary(hive: Hive, subkey: &str, value: &str, data: &[u8]) -> Result<()> {
    let (k, v) = (wide(subkey), wide(value));
    #[allow(clippy::cast_possible_truncation)]
    let rc = unsafe {
        RegSetKeyValueW(hive.hkey(), k.as_ptr(), v.as_ptr(), REG_BINARY, data.as_ptr().cast(), data.len() as DWORD)
    };
    match status_code(rc) {
        ERROR_SUCCESS => Ok(()),
        code => Err(AppError::win32(format!(r"writing {}\{subkey}\{value}", hive.name()), code)),
    }
}
