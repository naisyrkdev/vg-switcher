//! Well-known folders, resolved through the shell API rather than
//! environment variables (which a non-elevated process can change for the
//! user's future sessions).

use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use winapi::shared::guiddef::GUID;
use winapi::um::combaseapi::CoTaskMemFree;
use winapi::um::knownfolders::{FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86};
use winapi::um::shlobj::SHGetKnownFolderPath;
use winapi::um::sysinfoapi::GetSystemDirectoryW;

use crate::platform::from_wide;

const APP_DIR_NAME: &str = "VG_Switcher";

fn known_folder(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let mut raw = null_mut();
        let hr = SHGetKnownFolderPath(id, 0, null_mut(), &mut raw);
        let path = if hr >= 0 && !raw.is_null() {
            let len = (0..).take_while(|&i| *raw.add(i) != 0).count();
            Some(PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(raw, len))))
        } else {
            None
        };
        CoTaskMemFree(raw.cast());
        path
    }
}

/// `%ProgramData%\VG_Switcher` - config and log. Only administrators can
/// write there once `security::ensure_secure_dir` has run.
pub fn data_dir() -> PathBuf {
    known_folder(&FOLDERID_ProgramData).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")).join(APP_DIR_NAME)
}

/// Where versions up to 0.5 kept the config (`%APPDATA%\VG_Switcher`).
pub fn legacy_config_file() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join(APP_DIR_NAME).join("config.json"))
}

fn system32() -> PathBuf {
    let mut buf = [0u16; 260];
    #[allow(clippy::cast_possible_truncation)]
    let len = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if len == 0 || len >= buf.len() {
        return PathBuf::from(r"C:\Windows\System32");
    }
    PathBuf::from(from_wide(&buf[..len]))
}

/// Absolute path of a tool in System32 (e.g. `cmd.exe`).
pub fn system_tool(name: &str) -> PathBuf {
    system32().join(name)
}

/// Whether `path` is inside `dir` (case-insensitive, Windows-style).
pub fn is_under(path: &Path, dir: &Path) -> bool {
    let path = path.to_string_lossy().to_lowercase();
    let mut dir = dir.to_string_lossy().to_lowercase();
    if !dir.ends_with('\\') {
        dir.push('\\');
    }
    path.starts_with(&dir)
}

/// Whether the running exe sits under Program Files, where only
/// administrators can replace it. Anywhere else (e.g. an unzipped portable
/// copy in Downloads), any program running as the user could swap the exe
/// and get it launched elevated by the autostart task.
pub fn exe_in_protected_location() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    [&FOLDERID_ProgramFiles, &FOLDERID_ProgramFilesX86]
        .into_iter()
        .filter_map(known_folder)
        .any(|dir| is_under(&exe, &dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_under_matches_case_insensitively() {
        assert!(is_under(Path::new(r"C:\Program Files\VG Switcher\VG_Switcher.exe"), Path::new(r"c:\program files")));
    }

    #[test]
    fn is_under_requires_a_path_boundary() {
        assert!(!is_under(Path::new(r"C:\Program Files Evil\x.exe"), Path::new(r"C:\Program Files")));
        assert!(!is_under(Path::new(r"C:\Users\me\Downloads\x.exe"), Path::new(r"C:\Program Files\")));
    }

    #[test]
    fn system_tool_is_absolute() {
        assert!(system_tool("cmd.exe").is_absolute());
    }
}
